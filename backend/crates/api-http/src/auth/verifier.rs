//! Signature, issuer, audience, expiry, and algorithm verification responsibility.

#![allow(clippy::wildcard_imports)]

use super::*;

#[derive(Clone)]
pub struct OidcAuthenticator {
    config: OidcConfig,
    fetcher: Arc<dyn OidcFetcher>,
    pub(crate) cache: Arc<RwLock<Option<CachedJwks>>>,
    refresh_lock: Arc<Mutex<()>>,
    pub(crate) last_unknown_kid_refresh: Arc<Mutex<Option<Instant>>>,
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
    pub(crate) fn with_fetcher(config: OidcConfig, fetcher: Arc<dyn OidcFetcher>) -> Self {
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

    pub(crate) async fn verify_token(
        &self,
        token: &str,
    ) -> Result<OidcClaims, application::ApplicationError> {
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

    pub(crate) async fn key_for(&self, kid: &str) -> Result<Jwk, OidcError> {
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

    pub(crate) async fn cache_is_fresh(&self) -> bool {
        self.cache
            .read()
            .await
            .as_ref()
            .is_some_and(|cached| cached.fetched_at.elapsed() < JWKS_CACHE_TTL)
    }

    pub(crate) async fn cached_key(&self, kid: &str) -> Option<Jwk> {
        let cache = self.cache.read().await;
        let cached = cache.as_ref()?;
        if cached.fetched_at.elapsed() >= JWKS_CACHE_TTL {
            return None;
        }
        cached.keys.get(kid).cloned()
    }

    pub(crate) async fn refresh_jwks(&self) -> Result<(), OidcError> {
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
