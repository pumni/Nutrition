//! External issuer/subject to internal identity mapping responsibility.

#![allow(clippy::wildcard_imports)]

use super::*;

#[derive(Clone)]
pub enum Authenticator {
    Development,
    Oidc(OidcAuthenticator),
}

impl Authenticator {
    /// Builds the configured development or OIDC authenticator.
    ///
    /// # Errors
    ///
    /// Returns an error when the mode is unsupported or OIDC configuration is invalid.
    pub fn from_env(auth_mode: &str) -> Result<Self, String> {
        match auth_mode {
            "development" => Ok(Self::Development),
            "oidc" => Ok(Self::Oidc(OidcAuthenticator::new(OidcConfig::from_env()?)?)),
            _ => Err("AUTH_MODE must be development or oidc".to_owned()),
        }
    }

    /// Authenticates a request and resolves its stable internal subject.
    ///
    /// # Errors
    ///
    /// Returns an unauthorized or persistence error when the bearer credential cannot be
    /// verified or its external identity cannot be resolved.
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

pub(crate) fn bearer_token(header: Option<&str>) -> Result<&str, application::ApplicationError> {
    header
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| !value.trim().is_empty())
        .ok_or(application::ApplicationError::Unauthorized)
}

pub(crate) fn authenticate_development(
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
