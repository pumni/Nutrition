//! Strict provider response envelope responsibility.

#![allow(clippy::wildcard_imports)]

use super::*;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderResponse {
    pub output: Value,
    #[serde(default)]
    pub input_tokens: Option<i64>,
    #[serde(default)]
    pub output_tokens: Option<i64>,
}
