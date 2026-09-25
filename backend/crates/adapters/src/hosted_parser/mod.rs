//! Hosted parser facade that owns prompt/schema semantics, validation, circuit, and parser
//! telemetry while delegating provider protocol work through the structured-generation SPI.

use crate::{
    ModelIdentity, ProviderIdentity, StrictJsonSchema, StructuredGenerationRequest,
    StructuredModel, StructuredModelErrorClassification, UntrustedInput,
};
use application::{
    ApplicationError, MealTextParser, ParseRequest, ParsedMealDocument, ParserInvocationRecord,
    ParserTelemetrySink, normalize_vi_search_key,
};
use async_trait::async_trait;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

mod circuit_breaker;
mod config;
mod error;
mod providers;
mod telemetry;
mod validation;

pub use config::{
    APPROVED_HOSTED_CIRCUIT_COOLDOWN_SECONDS, APPROVED_HOSTED_CIRCUIT_FAILURE_THRESHOLD,
    APPROVED_HOSTED_ENDPOINT, APPROVED_HOSTED_MAXIMUM_RESPONSE_BYTES, APPROVED_HOSTED_MODEL,
    APPROVED_HOSTED_PROVIDER, APPROVED_HOSTED_TIMEOUT_MS, HOSTED_PROMPT_VERSION,
    HostedParserConfig, PARSER_SCHEMA_VERSION,
};
pub use providers::openai_responses::OpenAiResponsesProvider;

pub(crate) use circuit_breaker::CircuitState;
pub(crate) use config::{PARSER_SCHEMA, SYSTEM_PROMPT};
pub(crate) use telemetry::NoopParserTelemetry;
pub(crate) use validation::{OutputFailure, validate_output, validate_parse_request};

#[derive(Clone)]
pub struct HostedMealParser {
    config: HostedParserConfig,
    model: Arc<dyn StructuredModel>,
    telemetry: Arc<dyn ParserTelemetrySink>,
    circuit: Arc<Mutex<CircuitState>>,
    schema: Value,
}

impl HostedMealParser {
    /// Creates a hosted parser with a supplied structured model implementation.
    ///
    /// # Errors
    ///
    /// Returns `InvalidInput` when configuration or the embedded schema is invalid.
    pub fn new(
        config: HostedParserConfig,
        model: Arc<dyn StructuredModel>,
    ) -> Result<Self, ApplicationError> {
        config.validate()?;
        let schema = serde_json::from_str(PARSER_SCHEMA).map_err(|_| {
            ApplicationError::InvalidInput("embedded parser schema invalid".to_owned())
        })?;
        jsonschema::validator_for(&schema).map_err(|_| {
            ApplicationError::InvalidInput("embedded parser schema invalid".to_owned())
        })?;
        Ok(Self {
            config,
            model,
            telemetry: Arc::new(NoopParserTelemetry),
            circuit: Arc::new(Mutex::new(CircuitState::default())),
            schema,
        })
    }

    #[must_use]
    pub fn with_telemetry(mut self, telemetry: Arc<dyn ParserTelemetrySink>) -> Self {
        self.telemetry = telemetry;
        self
    }
}

impl HostedMealParser {
    fn generation_request(
        &self,
        request: &ParseRequest,
        repair_schema_output: bool,
    ) -> StructuredGenerationRequest {
        let system_instruction = if repair_schema_output {
            format!(
                "{SYSTEM_PROMPT} Return a schema-compliant JSON object on this repair attempt; do not add any explanation."
            )
        } else {
            SYSTEM_PROMPT.to_owned()
        };
        StructuredGenerationRequest {
            provider: ProviderIdentity::new(self.config.provider.clone()),
            model: ModelIdentity::new(self.config.model.clone()),
            system_instruction,
            schema: StrictJsonSchema::new(self.schema.clone()),
            untrusted_input: UntrustedInput::new(format!(
                "locale: {}\nmeal: {}",
                request.locale, request.text
            )),
        }
    }
}
#[async_trait]
impl MealTextParser for HostedMealParser {
    async fn parse(&self, request: ParseRequest) -> Result<ParsedMealDocument, ApplicationError> {
        validate_parse_request(&request)?;
        let started = Instant::now();
        if !self.circuit_allows_request().await {
            let error_code = "provider_circuit_open".to_owned();
            self.emit_telemetry(started, 0, (None, None), None, Some(error_code.clone()))
                .await;
            return Err(ApplicationError::ParserUnavailable(error_code));
        }
        let mut repair = false;
        for attempt in 0..=1 {
            let generation_request = self.generation_request(&request, repair);
            let response = tokio::time::timeout(
                self.config.timeout,
                self.model
                    .generate(&generation_request, self.config.maximum_response_bytes),
            )
            .await;
            match response {
                Ok(Ok(response)) => {
                    let usage = (
                        response.metadata.input_tokens,
                        response.metadata.output_tokens,
                    );
                    let output_sha256 = serde_json::to_vec(&response.output)
                        .ok()
                        .map(|encoded| hex::encode(Sha256::digest(encoded)));
                    if usage.0.is_some_and(|tokens| tokens < 0)
                        || usage.1.is_some_and(|tokens| tokens < 0)
                    {
                        return Err(self
                            .fail(
                                started,
                                attempt,
                                (None, None),
                                output_sha256,
                                "provider_usage_invalid".to_owned(),
                            )
                            .await);
                    }
                    match validate_output(&request, response.output) {
                        Ok(document) => {
                            self.record_success().await;
                            self.emit_telemetry(started, attempt, usage, output_sha256, None)
                                .await;
                            return Ok(document);
                        }
                        Err(OutputFailure::Schema) if attempt == 0 => repair = true,
                        Err(OutputFailure::Schema) => {
                            return Err(self
                                .fail(
                                    started,
                                    attempt,
                                    usage,
                                    output_sha256,
                                    "provider_schema_validation_failed".to_owned(),
                                )
                                .await);
                        }
                        Err(OutputFailure::Semantic(message)) => {
                            return Err(self
                                .fail(started, attempt, usage, output_sha256, message)
                                .await);
                        }
                    }
                }
                Ok(Err(error))
                    if error.classification == StructuredModelErrorClassification::Transient
                        && attempt == 0 => {}
                Ok(Err(error)) => {
                    return Err(self
                        .fail(
                            started,
                            attempt,
                            (None, None),
                            None,
                            error.code().to_owned(),
                        )
                        .await);
                }
                Err(_) if attempt == 0 => {}
                Err(_) => {
                    return Err(self
                        .fail(
                            started,
                            attempt,
                            (None, None),
                            None,
                            "provider_timeout".to_owned(),
                        )
                        .await);
                }
            }
        }
        unreachable!("bounded parser attempts always return or retry")
    }
}

#[derive(Clone)]
pub enum ConfiguredMealParser {
    Fixture(crate::FixtureParser),
    Hosted(Box<HostedMealParser>),
}

#[async_trait]
impl MealTextParser for ConfiguredMealParser {
    async fn parse(&self, request: ParseRequest) -> Result<ParsedMealDocument, ApplicationError> {
        match self {
            Self::Fixture(parser) => parser.parse(request).await,
            Self::Hosted(parser) => parser.parse(request).await,
        }
    }
}

#[cfg(test)]
mod tests;
