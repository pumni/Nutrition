use jsonwebtoken::jwk::{Jwk, JwkSet, KeyAlgorithm};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use persistence_postgres::PostgresAnalysisRepository;
use reqwest::{Client, StatusCode, Url, redirect::Policy};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::HashMap,
    env,
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{Duration, Instant},
};
use thiserror::Error;
use tokio::sync::{Mutex, RwLock};
use tracing::debug;

const JWKS_CACHE_TTL: Duration = Duration::from_mins(15);
const UNKNOWN_KID_REFRESH_INTERVAL: Duration = Duration::from_secs(30);
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
    fetcher: Arc<dyn OidcFetcher>,
    cache: Arc<RwLock<Option<CachedJwks>>>,
    refresh_lock: Arc<Mutex<()>>,
    last_unknown_kid_refresh: Arc<Mutex<Option<Instant>>>,
}

impl OidcAuthenticator {
    pub fn new(config: OidcConfig) -> Result<Self, String> {
        let client = Client::builder()
            .redirect(Policy::none())
            .build()
            .map_err(|_| "OIDC HTTP client could not be configured".to_owned())?;
        Ok(Self {
            config,
            fetcher: Arc::new(ReqwestFetcher { client }),
            cache: Arc::new(RwLock::new(None)),
            refresh_lock: Arc::new(Mutex::new(())),
            last_unknown_kid_refresh: Arc::new(Mutex::new(None)),
        })
    }

    #[cfg(test)]
    fn with_fetcher(config: OidcConfig, fetcher: Arc<dyn OidcFetcher>) -> Self {
        Self {
            config,
            fetcher,
            cache: Arc::new(RwLock::new(None)),
            refresh_lock: Arc::new(Mutex::new(())),
            last_unknown_kid_refresh: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn authenticate(
        &self,
        authorization_header: Option<&str>,
        repository: &PostgresAnalysisRepository,
    ) -> Result<domain::UserId, application::ApplicationError> {
        let token = bearer_token(authorization_header)?;
        let claims = self.verify_token(token).await?;
        repository
            .resolve_external_identity(&self.config.issuer_url, &claims.sub)
            .await
    }

    async fn verify_token(&self, token: &str) -> Result<OidcClaims, application::ApplicationError> {
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
        Ok(token_data.claims)
    }

    async fn key_for(&self, kid: &str) -> Result<Jwk, OidcError> {
        if self.cache_is_fresh().await
            && let Some(key) = self.cached_key(kid).await
        {
            return Ok(key);
        }

        let _refresh_guard = self.refresh_lock.lock().await;
        let cache_is_fresh = self.cache_is_fresh().await;
        if cache_is_fresh && let Some(key) = self.cached_key(kid).await {
            return Ok(key);
        }

        if cache_is_fresh {
            let mut last_refresh = self.last_unknown_kid_refresh.lock().await;
            if last_refresh
                .as_ref()
                .is_some_and(|instant| instant.elapsed() < UNKNOWN_KID_REFRESH_INTERVAL)
            {
                return Err(OidcError::KeyNotFound);
            }
            *last_refresh = Some(Instant::now());
        }

        // A stale cache is never consulted after refresh failure. The refresh lock and the
        // post-lock cache check make concurrent unknown-kid requests share one refresh.
        self.refresh_jwks().await?;
        if let Some(key) = self.cached_key(kid).await {
            return Ok(key);
        }

        if !cache_is_fresh {
            *self.last_unknown_kid_refresh.lock().await = Some(Instant::now());
        }
        Err(OidcError::KeyNotFound)
    }

    async fn cache_is_fresh(&self) -> bool {
        self.cache
            .read()
            .await
            .as_ref()
            .is_some_and(|cached| cached.fetched_at.elapsed() < JWKS_CACHE_TTL)
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
        let discovery_url = Url::parse(&format!(
            "{}/.well-known/openid-configuration",
            self.config.issuer_url.trim_end_matches('/')
        ))
        .map_err(|_| OidcError::InvalidDocument)?;
        let discovery_value = self.fetcher.get_json(discovery_url).await?;
        let discovery: DiscoveryDocument =
            serde_json::from_value(discovery_value).map_err(|_| OidcError::InvalidDocument)?;
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
        let jwks_value = self.fetcher.get_json(jwks_url).await?;
        let jwks: JwkSet =
            serde_json::from_value(jwks_value).map_err(|_| OidcError::InvalidDocument)?;
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

type FetchFuture = Pin<Box<dyn Future<Output = Result<Value, OidcError>> + Send>>;

trait OidcFetcher: Send + Sync {
    fn get_json(&self, url: Url) -> FetchFuture;
}

#[derive(Clone)]
struct ReqwestFetcher {
    client: Client,
}

impl OidcFetcher for ReqwestFetcher {
    fn get_json(&self, url: Url) -> FetchFuture {
        let client = self.client.clone();
        Box::pin(async move {
            let response = client.get(url).send().await.map_err(|_| OidcError::Http)?;
            if response.status() != StatusCode::OK {
                return Err(OidcError::Http);
            }
            response
                .json::<Value>()
                .await
                .map_err(|_| OidcError::InvalidDocument)
        })
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
    use super::{
        CLOCK_SKEW_SECONDS, FetchFuture, JWKS_CACHE_TTL, OidcAuthenticator, OidcConfig,
        OidcFetcher, UNKNOWN_KID_REFRESH_INTERVAL, bearer_token,
    };
    use jsonwebtoken::jwk::Jwk;
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
    use serde::Serialize;
    use serde_json::{Value, json};
    use std::{
        collections::{HashMap, VecDeque},
        sync::Arc,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };
    use tokio::sync::Mutex;

    const ISSUER: &str = "https://issuer.example/";
    const AUDIENCE: &str = "nutrition-api";
    const DISCOVERY_URL: &str = "https://issuer.example/.well-known/openid-configuration";
    const JWKS_URL: &str = "https://issuer.example/keys";
    const RSA_PRIVATE_KEY_DER: &[u8] = &[
        48, 130, 4, 164, 2, 1, 0, 2, 130, 1, 1, 0, 201, 17, 58, 172, 123, 141, 71, 68, 27, 28, 237,
        199, 220, 171, 118, 164, 226, 134, 86, 20, 42, 25, 149, 200, 156, 231, 110, 64, 220, 87,
        206, 226, 165, 189, 4, 203, 81, 59, 248, 151, 139, 32, 130, 30, 127, 9, 134, 34, 253, 203,
        200, 249, 37, 213, 79, 217, 15, 89, 34, 151, 196, 149, 193, 93, 223, 248, 46, 75, 220, 62,
        229, 26, 144, 26, 0, 145, 248, 126, 122, 33, 85, 50, 29, 149, 173, 76, 150, 202, 61, 204,
        22, 93, 7, 77, 81, 125, 43, 4, 87, 44, 7, 48, 145, 17, 34, 75, 121, 233, 78, 17, 209, 200,
        140, 110, 203, 70, 76, 121, 151, 241, 84, 190, 90, 172, 200, 112, 213, 36, 68, 44, 31, 7,
        160, 103, 198, 252, 11, 71, 243, 208, 72, 19, 216, 195, 4, 118, 125, 116, 183, 165, 43,
        214, 181, 243, 140, 192, 127, 194, 240, 160, 242, 241, 188, 150, 247, 34, 94, 103, 157,
        202, 143, 113, 39, 202, 12, 58, 29, 48, 80, 72, 49, 206, 37, 67, 48, 202, 47, 152, 47, 154,
        37, 203, 92, 29, 64, 24, 185, 188, 40, 24, 223, 19, 203, 55, 47, 156, 106, 139, 236, 148,
        161, 223, 163, 240, 203, 111, 34, 63, 53, 217, 217, 18, 225, 3, 34, 69, 83, 127, 111, 45,
        161, 221, 150, 60, 45, 133, 70, 174, 166, 87, 101, 55, 32, 159, 107, 163, 159, 203, 138,
        141, 114, 217, 84, 62, 83, 117, 2, 3, 1, 0, 1, 2, 130, 1, 0, 116, 68, 147, 66, 52, 59, 208,
        239, 16, 34, 157, 89, 74, 64, 152, 93, 230, 99, 186, 24, 244, 243, 80, 138, 238, 56, 97,
        167, 254, 2, 132, 174, 201, 26, 81, 80, 100, 204, 34, 7, 55, 187, 119, 162, 188, 225, 60,
        0, 181, 244, 6, 102, 20, 59, 146, 109, 185, 19, 89, 14, 155, 237, 225, 32, 48, 168, 41,
        213, 82, 41, 200, 127, 160, 190, 120, 127, 240, 74, 234, 194, 165, 170, 82, 51, 64, 64, 30,
        158, 26, 56, 14, 226, 130, 155, 114, 56, 200, 12, 61, 128, 71, 52, 13, 94, 244, 14, 227,
        19, 71, 66, 196, 165, 140, 151, 112, 210, 88, 220, 163, 65, 57, 56, 205, 228, 70, 16, 1,
        31, 108, 216, 199, 174, 174, 152, 10, 216, 199, 73, 148, 139, 153, 104, 134, 243, 72, 103,
        126, 118, 40, 179, 216, 78, 175, 146, 103, 46, 124, 146, 237, 153, 133, 19, 254, 79, 221,
        117, 107, 196, 183, 63, 181, 14, 115, 186, 109, 17, 225, 198, 235, 131, 49, 226, 64, 254,
        81, 129, 252, 70, 37, 161, 34, 237, 119, 32, 133, 234, 194, 8, 232, 46, 172, 148, 146, 109,
        129, 246, 244, 108, 252, 187, 151, 197, 109, 47, 152, 131, 203, 62, 58, 32, 172, 179, 119,
        44, 214, 180, 160, 39, 223, 222, 29, 155, 179, 24, 71, 84, 101, 80, 17, 31, 168, 85, 50,
        52, 174, 194, 250, 149, 76, 86, 132, 240, 8, 45, 149, 225, 2, 129, 129, 0, 250, 226, 204,
        159, 137, 13, 168, 121, 73, 75, 96, 57, 184, 9, 194, 146, 63, 116, 103, 17, 45, 54, 109,
        196, 242, 16, 49, 82, 184, 108, 178, 75, 88, 229, 116, 142, 0, 240, 101, 199, 39, 249, 70,
        228, 70, 32, 137, 134, 181, 69, 72, 175, 129, 238, 37, 38, 185, 77, 126, 60, 159, 117, 93,
        183, 211, 24, 10, 154, 206, 254, 38, 37, 234, 106, 13, 69, 35, 237, 230, 170, 48, 32, 131,
        137, 152, 136, 216, 218, 106, 143, 13, 240, 237, 255, 60, 62, 125, 192, 195, 222, 89, 130,
        38, 177, 101, 205, 201, 225, 210, 231, 154, 212, 42, 33, 196, 70, 208, 156, 189, 220, 202,
        151, 139, 183, 229, 155, 83, 169, 2, 129, 129, 0, 205, 42, 117, 246, 109, 90, 142, 39, 133,
        146, 168, 40, 7, 70, 19, 166, 223, 206, 174, 128, 241, 223, 203, 239, 35, 155, 167, 34,
        114, 101, 17, 227, 163, 67, 234, 8, 1, 7, 77, 219, 10, 113, 6, 244, 235, 70, 138, 204, 83,
        163, 244, 238, 97, 84, 110, 221, 99, 248, 42, 58, 9, 179, 98, 195, 229, 252, 29, 192, 154,
        169, 130, 209, 229, 201, 91, 36, 227, 150, 53, 29, 250, 62, 236, 206, 165, 2, 228, 124,
        175, 48, 162, 234, 65, 199, 53, 189, 7, 200, 25, 153, 143, 43, 7, 89, 155, 39, 239, 214,
        76, 250, 196, 39, 46, 152, 26, 98, 4, 6, 69, 178, 149, 13, 149, 165, 111, 14, 224, 237, 2,
        129, 129, 0, 165, 214, 239, 189, 241, 65, 151, 0, 253, 99, 22, 235, 112, 108, 251, 49, 227,
        66, 21, 179, 51, 248, 203, 156, 169, 9, 237, 32, 170, 10, 39, 221, 72, 152, 252, 111, 181,
        205, 50, 233, 239, 209, 134, 123, 244, 163, 35, 101, 105, 67, 176, 37, 91, 180, 200, 216,
        13, 27, 93, 123, 195, 72, 56, 183, 255, 144, 223, 201, 175, 84, 46, 113, 87, 191, 220, 159,
        188, 125, 80, 193, 100, 232, 201, 42, 86, 42, 247, 117, 129, 132, 168, 137, 127, 56, 253,
        96, 173, 141, 147, 171, 209, 237, 7, 152, 178, 27, 210, 252, 52, 134, 166, 64, 226, 204,
        201, 74, 170, 49, 66, 11, 197, 219, 97, 31, 33, 102, 136, 105, 2, 129, 129, 0, 159, 223,
        143, 142, 152, 198, 48, 241, 241, 47, 72, 35, 241, 15, 21, 111, 59, 5, 182, 139, 186, 111,
        87, 94, 33, 20, 64, 216, 33, 230, 74, 91, 101, 181, 29, 96, 97, 84, 204, 193, 124, 80, 121,
        72, 79, 50, 104, 36, 50, 165, 23, 206, 206, 63, 90, 40, 224, 166, 216, 88, 227, 136, 74,
        213, 54, 153, 16, 162, 46, 163, 26, 210, 239, 73, 201, 225, 100, 11, 127, 150, 189, 147,
        93, 58, 229, 240, 236, 21, 144, 250, 200, 104, 221, 27, 27, 240, 18, 105, 70, 49, 152, 19,
        194, 165, 123, 183, 82, 56, 95, 66, 37, 170, 22, 175, 230, 23, 44, 229, 170, 88, 47, 184,
        39, 61, 131, 222, 71, 61, 2, 129, 128, 11, 28, 40, 255, 38, 47, 23, 74, 192, 232, 23, 243,
        33, 86, 25, 198, 116, 139, 54, 63, 223, 77, 53, 217, 168, 221, 29, 143, 191, 64, 174, 101,
        176, 202, 49, 78, 121, 244, 178, 71, 82, 250, 202, 75, 197, 164, 185, 100, 67, 184, 170,
        243, 213, 111, 196, 64, 160, 41, 22, 48, 198, 165, 218, 147, 161, 154, 171, 137, 65, 80,
        207, 12, 187, 153, 29, 218, 200, 136, 168, 131, 51, 109, 29, 203, 219, 12, 124, 124, 160,
        89, 198, 99, 67, 133, 146, 197, 255, 201, 237, 63, 92, 106, 114, 217, 33, 113, 97, 28, 6,
        83, 223, 7, 61, 135, 129, 188, 4, 16, 173, 88, 221, 147, 55, 242, 236, 25, 151, 65,
    ];
    const RSA_MODULUS: &str = "yRE6rHuNR0QbHO3H3Kt2pOKGVhQqGZXInOduQNxXzuKlvQTLUTv4l4sggh5_CYYi_cvI-SXVT9kPWSKXxJXBXd_4LkvcPuUakBoAkfh-eiFVMh2VrUyWyj3MFl0HTVF9KwRXLAcwkREiS3npThHRyIxuy0ZMeZfxVL5arMhw1SRELB8HoGfG_AtH89BIE9jDBHZ9dLelK9a184zAf8LwoPLxvJb3Il5nncqPcSfKDDodMFBIMc4lQzDKL5gvmiXLXB1AGLm8KBjfE8s3L5xqi-yUod-j8MtvIj812dkS4QMiRVN_by2h3ZY8LYVGrqZXZTcgn2ujn8uKjXLZVD5TdQ";

    #[derive(Clone)]
    struct MockFetcher {
        state: Arc<Mutex<MockState>>,
    }

    struct MockState {
        responses: HashMap<String, VecDeque<MockResponse>>,
        requests: Vec<String>,
    }

    enum MockResponse {
        Json(Value),
        Failure,
    }

    impl MockFetcher {
        fn new() -> Self {
            Self {
                state: Arc::new(Mutex::new(MockState {
                    responses: HashMap::new(),
                    requests: Vec::new(),
                })),
            }
        }

        async fn push_json(&self, url: &str, value: Value) {
            self.state
                .lock()
                .await
                .responses
                .entry(url.to_owned())
                .or_default()
                .push_back(MockResponse::Json(value));
        }

        async fn push_failure(&self, url: &str) {
            self.state
                .lock()
                .await
                .responses
                .entry(url.to_owned())
                .or_default()
                .push_back(MockResponse::Failure);
        }

        async fn request_count(&self, url: &str) -> usize {
            self.state
                .lock()
                .await
                .requests
                .iter()
                .filter(|request| request.as_str() == url)
                .count()
        }
    }

    impl OidcFetcher for MockFetcher {
        fn get_json(&self, url: reqwest::Url) -> FetchFuture {
            let state = self.state.clone();
            Box::pin(async move {
                let mut state = state.lock().await;
                let url = url.to_string();
                state.requests.push(url.clone());
                match state.responses.get_mut(&url).and_then(VecDeque::pop_front) {
                    Some(MockResponse::Json(value)) => Ok(value),
                    Some(MockResponse::Failure) | None => Err(super::OidcError::Http),
                }
            })
        }
    }

    #[derive(Serialize)]
    struct TestClaims {
        iss: String,
        aud: String,
        sub: String,
        exp: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        nbf: Option<u64>,
    }

    fn now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock must be after Unix epoch")
            .as_secs()
    }

    fn valid_claims() -> TestClaims {
        TestClaims {
            iss: ISSUER.to_owned(),
            aud: AUDIENCE.to_owned(),
            sub: "subject-1".to_owned(),
            exp: now() + 3600,
            nbf: None,
        }
    }

    fn signed_token(kid: &str, claims: &TestClaims) -> String {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(kid.to_owned());
        encode(
            &header,
            claims,
            &EncodingKey::from_rsa_der(RSA_PRIVATE_KEY_DER),
        )
        .expect("test token must sign")
    }

    fn jwk(kid: &str) -> Value {
        json!({
            "kty": "RSA",
            "n": RSA_MODULUS,
            "e": "AQAB",
            "kid": kid,
            "alg": "RS256",
            "use": "sig"
        })
    }

    async fn install_refresh(fetcher: &MockFetcher, kids: &[&str]) {
        fetcher
            .push_json(
                DISCOVERY_URL,
                json!({"issuer": ISSUER, "jwks_uri": JWKS_URL}),
            )
            .await;
        fetcher
            .push_json(
                JWKS_URL,
                json!({"keys": kids.iter().map(|kid| jwk(kid)).collect::<Vec<_>>() }),
            )
            .await;
    }

    fn test_authenticator(fetcher: &MockFetcher) -> OidcAuthenticator {
        let config = OidcConfig::from_values(ISSUER, AUDIENCE).expect("test config must parse");
        OidcAuthenticator::with_fetcher(config, Arc::new(fetcher.clone()))
    }

    async fn warm_cache(authenticator: &OidcAuthenticator, fetcher: &MockFetcher) {
        install_refresh(fetcher, &["key-1"]).await;
        authenticator
            .key_for("key-1")
            .await
            .expect("initial key refresh must succeed");
    }

    #[test]
    fn oidc_config_requires_https_and_values() {
        assert!(OidcConfig::from_values("http://issuer.example", AUDIENCE).is_err());
        let config = OidcConfig::from_values(ISSUER, AUDIENCE).expect("valid OIDC config");
        assert_eq!(config.issuer_url, ISSUER);
        assert_eq!(config.audience, AUDIENCE);
        assert!(OidcConfig::from_values("https://issuer.example", AUDIENCE).is_ok());
    }

    #[test]
    fn bearer_token_does_not_accept_other_schemes() {
        assert!(bearer_token(Some("Basic secret")).is_err());
        assert!(bearer_token(Some("Bearer ")).is_err());
        assert_eq!(bearer_token(Some("Bearer token")).expect("bearer"), "token");
    }

    #[tokio::test]
    async fn valid_rs256_token_is_accepted_and_subject_is_stable() {
        let fetcher = MockFetcher::new();
        let authenticator = test_authenticator(&fetcher);
        warm_cache(&authenticator, &fetcher).await;
        let token = signed_token("key-1", &valid_claims());

        let first = authenticator
            .verify_token(&token)
            .await
            .expect("valid RS256 token must verify");
        let second = authenticator
            .verify_token(&token)
            .await
            .expect("same valid token must verify again");
        assert_eq!(first.iss, ISSUER);
        assert_eq!(first.sub, second.sub);
        assert_eq!(fetcher.request_count(DISCOVERY_URL).await, 1);
        assert_eq!(fetcher.request_count(JWKS_URL).await, 1);
    }

    #[tokio::test]
    async fn claims_and_algorithm_must_match_the_oidc_contract() {
        let fetcher = MockFetcher::new();
        let authenticator = test_authenticator(&fetcher);
        warm_cache(&authenticator, &fetcher).await;

        let mut claims = valid_claims();
        claims.iss = "https://wrong-issuer.example/".to_owned();
        assert!(
            authenticator
                .verify_token(&signed_token("key-1", &claims))
                .await
                .is_err()
        );

        let mut claims = valid_claims();
        claims.aud = "wrong-audience".to_owned();
        assert!(
            authenticator
                .verify_token(&signed_token("key-1", &claims))
                .await
                .is_err()
        );

        let mut claims = valid_claims();
        claims.sub = "   ".to_owned();
        assert!(
            authenticator
                .verify_token(&signed_token("key-1", &claims))
                .await
                .is_err()
        );

        let mut claims = valid_claims();
        claims.exp = now() - CLOCK_SKEW_SECONDS - 1;
        assert!(
            authenticator
                .verify_token(&signed_token("key-1", &claims))
                .await
                .is_err()
        );

        let mut claims = valid_claims();
        claims.exp = now() - (CLOCK_SKEW_SECONDS - 1);
        assert!(
            authenticator
                .verify_token(&signed_token("key-1", &claims))
                .await
                .is_ok()
        );

        let mut claims = valid_claims();
        claims.nbf = Some(now() + CLOCK_SKEW_SECONDS - 1);
        assert!(
            authenticator
                .verify_token(&signed_token("key-1", &claims))
                .await
                .is_ok()
        );

        let mut claims = valid_claims();
        claims.nbf = Some(now() + CLOCK_SKEW_SECONDS + 1);
        assert!(
            authenticator
                .verify_token(&signed_token("key-1", &claims))
                .await
                .is_err()
        );

        let mut header = Header::new(Algorithm::HS256);
        header.kid = Some("key-1".to_owned());
        let wrong_algorithm = encode(
            &header,
            &valid_claims(),
            &EncodingKey::from_secret(b"test-secret"),
        )
        .expect("test HS256 token must sign");
        assert!(authenticator.verify_token(&wrong_algorithm).await.is_err());
    }

    #[tokio::test]
    async fn unknown_kid_refreshes_once_and_accepts_a_rotated_key() {
        let fetcher = MockFetcher::new();
        let authenticator = test_authenticator(&fetcher);
        warm_cache(&authenticator, &fetcher).await;
        install_refresh(&fetcher, &["key-1", "rotated-key"]).await;

        let token = signed_token("rotated-key", &valid_claims());
        authenticator
            .verify_token(&token)
            .await
            .expect("rotated key must be accepted after one refresh");
        assert_eq!(fetcher.request_count(DISCOVERY_URL).await, 2);
        assert_eq!(fetcher.request_count(JWKS_URL).await, 2);
    }

    #[tokio::test]
    async fn stale_cache_refresh_failure_never_uses_stale_key() {
        let fetcher = MockFetcher::new();
        let authenticator = test_authenticator(&fetcher);
        warm_cache(&authenticator, &fetcher).await;
        {
            let mut cache = authenticator.cache.write().await;
            cache.as_mut().expect("cache must be warm").fetched_at = std::time::Instant::now()
                .checked_sub(JWKS_CACHE_TTL + Duration::from_secs(1))
                .expect("test clock must support stale cache");
        }
        fetcher.push_failure(DISCOVERY_URL).await;

        assert!(authenticator.key_for("key-1").await.is_err());
        assert_eq!(fetcher.request_count(DISCOVERY_URL).await, 2);
        assert_eq!(fetcher.request_count(JWKS_URL).await, 1);
    }

    #[tokio::test]
    async fn stale_cache_requires_a_refresh_even_when_key_is_present() {
        let fetcher = MockFetcher::new();
        let authenticator = test_authenticator(&fetcher);
        warm_cache(&authenticator, &fetcher).await;
        {
            let mut cache = authenticator.cache.write().await;
            cache.as_mut().expect("cache must be warm").fetched_at = std::time::Instant::now()
                .checked_sub(JWKS_CACHE_TTL + Duration::from_secs(1))
                .expect("test clock must support stale cache");
        }
        install_refresh(&fetcher, &["key-1"]).await;

        authenticator
            .key_for("key-1")
            .await
            .expect("fresh key must be loaded");
        assert_eq!(fetcher.request_count(DISCOVERY_URL).await, 2);
        assert_eq!(fetcher.request_count(JWKS_URL).await, 2);
    }

    #[tokio::test]
    async fn discovery_and_jwks_origin_documents_fail_closed() {
        let cases = [
            json!({"issuer": "https://wrong-issuer.example/", "jwks_uri": JWKS_URL}),
            json!({"issuer": ISSUER, "jwks_uri": "http://issuer.example/keys"}),
            json!({"issuer": ISSUER, "jwks_uri": "https://keys.example/keys"}),
        ];
        for discovery in cases {
            let fetcher = MockFetcher::new();
            fetcher.push_json(DISCOVERY_URL, discovery).await;
            let authenticator = test_authenticator(&fetcher);
            assert!(authenticator.refresh_jwks().await.is_err());
            assert_eq!(fetcher.request_count(JWKS_URL).await, 0);
        }
    }

    #[tokio::test]
    async fn concurrent_unknown_kids_share_one_refresh() {
        let fetcher = MockFetcher::new();
        let authenticator = test_authenticator(&fetcher);
        warm_cache(&authenticator, &fetcher).await;
        install_refresh(&fetcher, &["key-1"]).await;

        let (first, second, third, fourth) = tokio::join!(
            authenticator.key_for("random-a"),
            authenticator.key_for("random-b"),
            authenticator.key_for("random-c"),
            authenticator.key_for("random-d"),
        );
        assert!(first.is_err());
        assert!(second.is_err());
        assert!(third.is_err());
        assert!(fourth.is_err());
        assert_eq!(fetcher.request_count(DISCOVERY_URL).await, 2);
        assert_eq!(fetcher.request_count(JWKS_URL).await, 2);
    }

    #[tokio::test]
    async fn unknown_kid_refreshes_are_throttled_for_thirty_seconds() {
        let fetcher = MockFetcher::new();
        let authenticator = test_authenticator(&fetcher);
        warm_cache(&authenticator, &fetcher).await;
        install_refresh(&fetcher, &["key-1"]).await;

        assert!(authenticator.key_for("random-a").await.is_err());
        assert!(authenticator.key_for("random-b").await.is_err());
        assert_eq!(fetcher.request_count(DISCOVERY_URL).await, 2);
        assert_eq!(fetcher.request_count(JWKS_URL).await, 2);
        assert!(
            authenticator
                .last_unknown_kid_refresh
                .lock()
                .await
                .as_ref()
                .expect("unknown-kid refresh must be recorded")
                .elapsed()
                < UNKNOWN_KID_REFRESH_INTERVAL
        );
    }

    #[test]
    fn jwk_fixture_is_an_rs256_signing_key() {
        let key: Jwk = serde_json::from_value(jwk("key-1")).expect("JWK must parse");
        assert_eq!(
            key.common.key_algorithm,
            Some(jsonwebtoken::jwk::KeyAlgorithm::RS256)
        );
    }
}
