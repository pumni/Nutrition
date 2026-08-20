//! Hosted parser facade. Provider transport, response, validation, circuit, telemetry, and
//! mapping responsibilities are kept behind the application parser port.

mod circuit_breaker;
mod config;
mod error;
mod implementation;
mod providers;
mod request;
mod response;
mod telemetry;
mod transport;
mod validation;

pub use implementation::{
    APPROVED_HOSTED_CIRCUIT_COOLDOWN_SECONDS, APPROVED_HOSTED_CIRCUIT_FAILURE_THRESHOLD,
    APPROVED_HOSTED_ENDPOINT, APPROVED_HOSTED_MAXIMUM_RESPONSE_BYTES, APPROVED_HOSTED_MODEL,
    APPROVED_HOSTED_PROVIDER, APPROVED_HOSTED_TIMEOUT_MS, ConfiguredMealParser,
    HOSTED_PROMPT_VERSION, HostedLlmTransport, HostedMealParser, HostedParserConfig,
    PARSER_SCHEMA_VERSION, ProviderInput, ProviderRequest, ProviderResponse,
    ReqwestHostedLlmTransport, TransportError, TransportErrorKind,
};
