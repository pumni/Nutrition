//! Minimum-data provider-neutral request responsibility.

#![allow(clippy::wildcard_imports)]

use super::*;

#[derive(Clone, Debug, Serialize)]
pub struct ProviderRequest {
    pub provider: String,
    pub model: String,
    pub system_instruction: &'static str,
    pub schema: Value,
    pub input: ProviderInput,
    pub repair_schema_output: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProviderInput {
    pub locale: String,
    pub untrusted_meal_text: String,
}
