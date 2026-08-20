//! OIDC claims decoding responsibility.

#![allow(clippy::wildcard_imports)]

use super::*;

#[derive(Debug, Deserialize)]
pub(crate) struct OidcClaims {
    pub(crate) iss: String,
    pub(crate) aud: AudienceClaim,
    pub(crate) sub: String,
    pub(crate) exp: u64,
    #[allow(dead_code)]
    #[serde(default)]
    pub(crate) nbf: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum AudienceClaim {
    Single(String),
    Multiple(Vec<String>),
}

impl AudienceClaim {
    pub(crate) fn contains(&self, expected: &str) -> bool {
        match self {
            Self::Single(value) => value == expected,
            Self::Multiple(values) => values.iter().any(|value| value == expected),
        }
    }
}
