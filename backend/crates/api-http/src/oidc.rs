use jsonwebtoken::jwk::{Jwk, JwkSet, KeyAlgorithm};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use persistence_postgres::PostgresAnalysisRepository;
use reqwest::{Client, StatusCode, Url, redirect::Policy};
use serde::Deserialize;
use std::{
    collections::HashMap,
    env,
    sync::Arc,
    time::{Duration, Instant},
};
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::debug;

const JWKS_CACHE_TTL: Duration = Duration::from_mins(15);
const CLOCK_SKEW_SECONDS: u64 = 60;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OidcConfig {
    pub issuer_url: String,
    pub audience: String,
}

impl OidcConfig {
    pub fn from_env() -> Result<Self, String> {
        let issuer_url = required_env("OIDC_ISSUER_URL")?;
        let audience = required_env("OIDC_AUDIENCE")?;
        Self::from_values(&issuer_url, &audience)
    }

    fn from_values(issuer_url: &str, audience: &str) -> Result<Self, String> {
        if audience.trim().is_empty() {
            return Err("OIDC_AUDIENCE must not be empty".to_owned());
        }
        let parsed =
            Url::parse(issuer_url).map_err(|_| "OIDC_ISSUER_URL must be a valid URL".to_owned())?;
        if parsed.scheme() != "https" {
            return Err("OIDC_ISSUER_URL must use HTTPS".to_owned());
        }
        if parsed.host_str().is_none() {
            return Err("OIDC_ISSUER_URL must contain a host".to_owned());
        }
        if parsed.query().is_some() || parsed.fragment().is_some() {
            return Err("OIDC_ISSUER_URL must not contain a query or fragment".to_owned());
        }
        Ok(Self {
            issuer_url: issuer_url.to_owned(),
            audience: audience.to_owned(),
        })
    }
}

#[derive(Clone)]
pub enum Authenticator {
    Development,
    Oidc(OidcAuthenticator),
}

impl Authenticator {
    pub fn from_env(auth_mode: &str) -> Result<Self, String> {
        match auth_mode {
            "development" => Ok(Self::Development),
            "oidc" => Ok(Self::Oidc(OidcAuthenticator::new(OidcConfig::from_env()?)?)),
            _ => Err("AUTH_MODE must be development or oidc".to_owned()),
        }
    }

    pub async fn authenticate(
        &self,
        authorization_header: Option<&str>,
        repository: &PostgresAnalysisRepository,
    ) -> Result<domain::UserId, application::ApplicationError> {
        match self {
            Self::Development => authenticate_development(authorization_header),
            Self::Oidc(authenticator) => {
                authenticator
                    .authenticate(authorization_header, repository)
                    .await
            }
        }
    }
}

#[derive(Clone)]
pub struct OidcAuthenticator {
    config: OidcConfig,
    client: Client,
    cache: Arc<RwLock<Option<CachedJwks>>>,
}

impl OidcAuthenticator {
    pub fn new(config: OidcConfig) -> Result<Self, String> {
        let client = Client::builder()
            .redirect(Policy::none())
            .build()
            .map_err(|_| "OIDC HTTP client could not be configured".to_owned())?;
        Ok(Self {
            config,
            client,
            cache: Arc::new(RwLock::new(None)),
        })
    }

    pub async fn authenticate(
        &self,
        authorization_header: Option<&str>,
        repository: &PostgresAnalysisRepository,
    ) -> Result<domain::UserId, application::ApplicationError> {
        let token = bearer_token(authorization_header)?;
        let header =
            decode_header(token).map_err(|_| application::ApplicationError::Unauthorized)?;
        if header.alg != Algorithm::RS256 {
            return Err(application::ApplicationError::Unauthorized);
        }
        let kid = header
            .kid
            .as_deref()
            .filter(|value| !value.is_empty())
            .ok_or(application::ApplicationError::Unauthorized)?;
        let jwk = self
            .key_for(kid)
            .await
            .map_err(|_| application::ApplicationError::Unauthorized)?;
        let decoding_key =
            DecodingKey::from_jwk(&jwk).map_err(|_| application::ApplicationError::Unauthorized)?;
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(std::slice::from_ref(&self.config.issuer_url));
        validation.set_audience(std::slice::from_ref(&self.config.audience));
        validation.leeway = CLOCK_SKEW_SECONDS;
        validation.validate_nbf = true;
        let token_data = decode::<OidcClaims>(token, &decoding_key, &validation)
            .map_err(|_| application::ApplicationError::Unauthorized)?;
        if token_data.claims.iss != self.config.issuer_url
            || token_data.claims.exp == 0
            || token_data.claims.sub.trim().is_empty()
            || !token_data.claims.aud.contains(&self.config.audience)
        {
            return Err(application::ApplicationError::Unauthorized);
        }
        repository
            .resolve_external_identity(&self.config.issuer_url, &token_data.claims.sub)
            .await
    }

    async fn key_for(&self, kid: &str) -> Result<Jwk, OidcError> {
        let cache_is_fresh = {
            let cache = self.cache.read().await;
            cache
                .as_ref()
                .is_some_and(|cached| cached.fetched_at.elapsed() < JWKS_CACHE_TTL)
        };
        if cache_is_fresh && let Some(key) = self.cached_key(kid).await {
            return Ok(key);
        }

        // A missing kid causes exactly one refresh for this authentication attempt. A stale cache
        // is never consulted after the refresh fails.
        self.refresh_jwks().await?;
        self.cached_key(kid).await.ok_or(OidcError::KeyNotFound)
    }

    async fn cached_key(&self, kid: &str) -> Option<Jwk> {
        let cache = self.cache.read().await;
        let cached = cache.as_ref()?;
        if cached.fetched_at.elapsed() >= JWKS_CACHE_TTL {
            return None;
        }
        cached.keys.get(kid).cloned()
    }

    async fn refresh_jwks(&self) -> Result<(), OidcError> {
        let discovery_url = format!(
            "{}/.well-known/openid-configuration",
            self.config.issuer_url.trim_end_matches('/')
        );
        let discovery_response = self
            .client
            .get(&discovery_url)
            .send()
            .await
            .map_err(|_| OidcError::Http)?;
        if discovery_response.status() != StatusCode::OK {
            return Err(OidcError::Http);
        }
        let discovery: DiscoveryDocument = discovery_response
            .json()
            .await
            .map_err(|_| OidcError::InvalidDocument)?;
        if discovery.issuer != self.config.issuer_url {
            return Err(OidcError::InvalidDocument);
        }
        let jwks_url = Url::parse(&discovery.jwks_uri).map_err(|_| OidcError::InvalidDocument)?;
        if jwks_url.scheme() != "https" {
            return Err(OidcError::InvalidDocument);
        }
        let issuer_url =
            Url::parse(&self.config.issuer_url).map_err(|_| OidcError::InvalidDocument)?;
        if jwks_url.scheme() != issuer_url.scheme()
            || jwks_url.host_str() != issuer_url.host_str()
            || jwks_url.port_or_known_default() != issuer_url.port_or_known_default()
        {
            return Err(OidcError::InvalidDocument);
        }
        let jwks_response = self
            .client
            .get(jwks_url)
            .send()
            .await
            .map_err(|_| OidcError::Http)?;
        if jwks_response.status() != StatusCode::OK {
            return Err(OidcError::Http);
        }
        let jwks: JwkSet = jwks_response
            .json()
            .await
            .map_err(|_| OidcError::InvalidDocument)?;
        let keys = jwks
            .keys
            .into_iter()
            .filter(|key| key.common.key_algorithm == Some(KeyAlgorithm::RS256))
            .filter_map(|key| key.common.key_id.clone().map(|kid| (kid, key)))
            .collect::<HashMap<_, _>>();
        if keys.is_empty() {
            return Err(OidcError::InvalidDocument);
        }
        let mut cache = self.cache.write().await;
        *cache = Some(CachedJwks {
            fetched_at: Instant::now(),
            keys,
        });
        debug!("OIDC JWKS cache refreshed");
        Ok(())
    }
}

#[derive(Debug)]
struct CachedJwks {
    fetched_at: Instant,
    keys: HashMap<String, Jwk>,
}

#[derive(Debug, Deserialize)]
struct DiscoveryDocument {
    issuer: String,
    jwks_uri: String,
}

#[derive(Debug, Deserialize)]
struct OidcClaims {
    iss: String,
    aud: AudienceClaim,
    sub: String,
    exp: u64,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum AudienceClaim {
    Single(String),
    Multiple(Vec<String>),
}

impl AudienceClaim {
    fn contains(&self, expected: &str) -> bool {
        match self {
            Self::Single(value) => value == expected,
            Self::Multiple(values) => values.iter().any(|value| value == expected),
        }
    }
}

#[derive(Debug, Error)]
enum OidcError {
    #[error("OIDC HTTP request failed")]
    Http,
    #[error("OIDC document is invalid")]
    InvalidDocument,
    #[error("OIDC signing key was not found")]
    KeyNotFound,
}

fn required_env(name: &str) -> Result<String, String> {
    match env::var(name) {
        Ok(value) if !value.trim().is_empty() => Ok(value),
        Ok(_) | Err(env::VarError::NotPresent) => {
            Err(format!("{name} is required when AUTH_MODE=oidc"))
        }
        Err(env::VarError::NotUnicode(_)) => Err(format!("{name} must be valid Unicode")),
    }
}

fn bearer_token(header: Option<&str>) -> Result<&str, application::ApplicationError> {
    header
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| !value.trim().is_empty())
        .ok_or(application::ApplicationError::Unauthorized)
}

fn authenticate_development(
    authorization_header: Option<&str>,
) -> Result<domain::UserId, application::ApplicationError> {
    let value = bearer_token(authorization_header)?;
    let user_id = value
        .strip_prefix("dev:")
        .ok_or(application::ApplicationError::Unauthorized)?;
    user_id
        .parse::<domain::UserId>()
        .map_err(|_| application::ApplicationError::Unauthorized)
}

#[cfg(test)]
mod tests {
    use super::{OidcConfig, bearer_token};

    #[test]
    fn oidc_config_requires_https_and_values() {
        assert!(OidcConfig::from_values("http://issuer.example", "nutrition-api").is_err());
        let config = OidcConfig::from_values("https://issuer.example/", "nutrition-api")
            .expect("valid OIDC config");
        assert_eq!(config.issuer_url, "https://issuer.example/");
        assert_eq!(config.audience, "nutrition-api");
        assert!(OidcConfig::from_values("https://issuer.example", "nutrition-api").is_ok());
    }

    #[test]
    fn bearer_token_does_not_accept_other_schemes() {
        assert!(bearer_token(Some("Basic secret")).is_err());
        assert!(bearer_token(Some("Bearer ")).is_err());
        assert_eq!(bearer_token(Some("Bearer token")).expect("bearer"), "token");
    }
}
