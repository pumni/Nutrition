//! JWKS loading, caching, refresh, and unknown-key responsibility.

#![allow(clippy::wildcard_imports)]

use super::*;

pub(crate) type FetchFuture = Pin<Box<dyn Future<Output = Result<Value, OidcError>> + Send>>;

pub(crate) trait OidcFetcher: Send + Sync {
    fn get_json(&self, url: Url) -> FetchFuture;
}

#[derive(Clone)]
pub(crate) struct ReqwestFetcher {
    pub(crate) client: Client,
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
pub(crate) struct CachedJwks {
    pub(crate) fetched_at: Instant,
    pub(crate) keys: HashMap<String, Jwk>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct DiscoveryDocument {
    pub(crate) issuer: String,
    pub(crate) jwks_uri: String,
}
