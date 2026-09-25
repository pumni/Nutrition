//! Provider-neutral structured-generation SPI used by hosted adapters.

use async_trait::async_trait;
use serde_json::Value;

/// Typed, provider-neutral identity for the configured model provider.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderIdentity(String);

impl ProviderIdentity {
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Typed identity for the configured model.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelIdentity(String);

impl ModelIdentity {
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// JSON Schema owned and validated by the hosted parser before generation.
#[derive(Clone, Debug, PartialEq)]
pub struct StrictJsonSchema(Value);

impl StrictJsonSchema {
    #[must_use]
    pub fn new(value: Value) -> Self {
        Self(value)
    }

    #[must_use]
    pub fn value(&self) -> &Value {
        &self.0
    }
}

/// Opaque input that remains untrusted throughout provider generation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UntrustedInput(String);

impl UntrustedInput {
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Provider-neutral request assembled by the parser.
#[derive(Clone, Debug, PartialEq)]
pub struct StructuredGenerationRequest {
    pub provider: ProviderIdentity,
    pub model: ModelIdentity,
    pub system_instruction: String,
    pub untrusted_input: UntrustedInput,
    pub schema: StrictJsonSchema,
}

/// Optional bounded usage metadata used by parser telemetry.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StructuredResponseMetadata {
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
}

/// Structured JSON output plus the limited metadata consumed by the parser.
#[derive(Clone, Debug, PartialEq)]
pub struct StructuredGenerationResponse {
    pub output: Value,
    pub metadata: StructuredResponseMetadata,
}

/// Retry-relevant failure classification shared by structured model implementations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StructuredModelErrorClassification {
    Transient,
    Permanent,
}

/// Content-free structured model failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StructuredModelError {
    pub classification: StructuredModelErrorClassification,
    code: String,
}

impl StructuredModelError {
    /// Creates a failure with a bounded, content-free telemetry code.
    #[must_use]
    pub fn new(
        classification: StructuredModelErrorClassification,
        code: impl Into<String>,
    ) -> Self {
        let code = code.into();
        let code_is_safe = !code.is_empty()
            && code.len() <= 64
            && code
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
        Self {
            classification,
            code: if code_is_safe {
                code
            } else {
                "provider_error".to_owned()
            },
        }
    }

    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }
}

/// Provider-neutral interface for bounded structured generation.
#[async_trait]
pub trait StructuredModel: Send + Sync {
    /// Generates one structured response, enforcing the supplied response-size limit.
    async fn generate(
        &self,
        request: &StructuredGenerationRequest,
        maximum_response_bytes: usize,
    ) -> Result<StructuredGenerationResponse, StructuredModelError>;
}

#[cfg(test)]
mod tests {
    use super::{StructuredModelError, StructuredModelErrorClassification};

    #[test]
    fn error_codes_are_bounded_and_content_free() {
        let raw = StructuredModelError::new(
            StructuredModelErrorClassification::Permanent,
            "provider failed with secret meal text",
        );
        let oversized = StructuredModelError::new(
            StructuredModelErrorClassification::Transient,
            "x".repeat(65),
        );

        assert_eq!(raw.code(), "provider_error");
        assert_eq!(oversized.code(), "provider_error");
    }
}
