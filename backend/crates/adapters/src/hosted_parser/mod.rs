//! Hosted parser facade. Provider transport, response, validation, circuit, telemetry, and
//! mapping responsibilities are kept behind the application parser port.

use application::{
    ApplicationError, MealTextParser, ParseRequest, ParsedMealDocument, ParserInvocationRecord,
    ParserTelemetrySink, normalize_vi_search_key,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
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
mod request;
mod response;
mod telemetry;
mod transport;
mod validation;

pub use config::{
    APPROVED_HOSTED_CIRCUIT_COOLDOWN_SECONDS, APPROVED_HOSTED_CIRCUIT_FAILURE_THRESHOLD,
    APPROVED_HOSTED_ENDPOINT, APPROVED_HOSTED_MAXIMUM_RESPONSE_BYTES, APPROVED_HOSTED_MODEL,
    APPROVED_HOSTED_PROVIDER, APPROVED_HOSTED_TIMEOUT_MS, HOSTED_PROMPT_VERSION,
    HostedParserConfig, PARSER_SCHEMA_VERSION,
};
pub use error::{TransportError, TransportErrorKind};
pub use request::{ProviderInput, ProviderRequest};
pub use response::ProviderResponse;
pub use transport::{HostedLlmTransport, ReqwestHostedLlmTransport};

pub(crate) use circuit_breaker::CircuitState;
pub(crate) use config::{PARSER_SCHEMA, SYSTEM_PROMPT};
pub(crate) use error::classify_reqwest_error;
pub(crate) use providers::openai_responses::{openai_responses_request, parse_openai_response};
pub(crate) use telemetry::NoopParserTelemetry;
pub(crate) use validation::{OutputFailure, validate_output, validate_parse_request};

#[derive(Clone)]
pub struct HostedMealParser {
    config: HostedParserConfig,
    transport: Arc<dyn HostedLlmTransport>,
    telemetry: Arc<dyn ParserTelemetrySink>,
    circuit: Arc<Mutex<CircuitState>>,
    schema: Value,
}

impl HostedMealParser {
    /// Creates a hosted parser with a supplied transport.
    ///
    /// # Errors
    ///
    /// Returns `InvalidInput` when configuration or the embedded schema is invalid.
    pub fn new(
        config: HostedParserConfig,
        transport: Arc<dyn HostedLlmTransport>,
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
            transport,
            telemetry: Arc::new(NoopParserTelemetry),
            circuit: Arc::new(Mutex::new(CircuitState::default())),
            schema,
        })
    }

    /// Creates a hosted parser using the bounded HTTPS transport.
    ///
    /// # Errors
    ///
    /// Returns `InvalidInput` when configuration or client construction fails.
    pub fn with_reqwest(config: HostedParserConfig) -> Result<Self, ApplicationError> {
        let transport = Arc::new(ReqwestHostedLlmTransport::new(&config)?);
        Self::new(config, transport)
    }

    #[must_use]
    pub fn with_telemetry(mut self, telemetry: Arc<dyn ParserTelemetrySink>) -> Self {
        self.telemetry = telemetry;
        self
    }
}

impl HostedMealParser {
    fn provider_request(
        &self,
        request: &ParseRequest,
        repair_schema_output: bool,
    ) -> ProviderRequest {
        ProviderRequest {
            provider: self.config.provider.clone(),
            model: self.config.model.clone(),
            system_instruction: SYSTEM_PROMPT,
            schema: self.schema.clone(),
            input: ProviderInput {
                locale: request.locale.clone(),
                untrusted_meal_text: request.text.clone(),
            },
            repair_schema_output,
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
            let provider_request = self.provider_request(&request, repair);
            let response = tokio::time::timeout(
                self.config.timeout,
                self.transport
                    .complete(&provider_request, self.config.maximum_response_bytes),
            )
            .await;
            match response {
                Ok(Ok(response)) => {
                    let usage = (response.input_tokens, response.output_tokens);
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
                Ok(Err(error)) if error.kind == TransportErrorKind::Transient && attempt == 0 => {}
                Ok(Err(error)) => {
                    return Err(self
                        .fail(started, attempt, (None, None), None, error.code)
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
