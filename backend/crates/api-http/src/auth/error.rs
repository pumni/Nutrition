//! Stable authentication failure classification responsibility.

#![allow(clippy::wildcard_imports)]

use super::*;

#[derive(Debug, Error)]
pub(crate) enum OidcError {
    #[error("OIDC HTTP request failed")]
    Http,
    #[error("OIDC document is invalid")]
    InvalidDocument,
    #[error("OIDC signing key was not found")]
    KeyNotFound,
}

pub(crate) fn required_env(name: &str) -> Result<String, String> {
    match env::var(name) {
        Ok(value) if !value.trim().is_empty() => Ok(value),
        Ok(_) | Err(env::VarError::NotPresent) => {
            Err(format!("{name} is required when AUTH_MODE=oidc"))
        }
        Err(env::VarError::NotUnicode(_)) => Err(format!("{name} must be valid Unicode")),
    }
}
