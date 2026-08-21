//! Content-free hosted parser telemetry responsibility.

#![allow(clippy::wildcard_imports)]

use super::*;

pub(crate) struct NoopParserTelemetry;

#[async_trait]
impl ParserTelemetrySink for NoopParserTelemetry {
    async fn record(&self, _invocation: ParserInvocationRecord) -> Result<(), ApplicationError> {
        Ok(())
    }
}

impl HostedMealParser {
    pub(crate) async fn emit_telemetry(
        &self,
        started: Instant,
        retry_count: i32,
        usage: (Option<i64>, Option<i64>),
        output_sha256: Option<String>,
        error_code: Option<String>,
    ) {
        let status = if error_code.is_some() {
            "failed"
        } else {
            "succeeded"
        };
        let _ = self
            .telemetry
            .record(ParserInvocationRecord {
                provider: self.config.provider.clone(),
                model: self.config.model.clone(),
                prompt_version: HOSTED_PROMPT_VERSION.to_owned(),
                schema_version: PARSER_SCHEMA_VERSION.to_owned(),
                latency_ms: i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX),
                retry_count,
                input_tokens: usage.0,
                output_tokens: usage.1,
                output_sha256,
                status: status.to_owned(),
                error_code,
            })
            .await;
    }
}
