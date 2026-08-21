//! OIDC validation and cache configuration responsibility.

#![allow(clippy::wildcard_imports)]

use super::*;

pub(crate) const JWKS_CACHE_TTL: Duration = Duration::from_mins(15);
pub(crate) const UNKNOWN_KID_REFRESH_INTERVAL: Duration = Duration::from_secs(30);
pub(crate) const CLOCK_SKEW_SECONDS: u64 = 60;

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

    pub(crate) fn from_values(issuer_url: &str, audience: &str) -> Result<Self, String> {
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
