use application::{ApplicationError, ParserInvocationRecord, ParserTelemetrySink};
use async_trait::async_trait;
use metrics::{counter, gauge, histogram};
use sqlx::PgPool;
use std::time::Instant;
use uuid::Uuid;

#[derive(Clone)]
pub struct PostgresParserTelemetrySink {
    pool: PgPool,
}

impl PostgresParserTelemetrySink {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ParserTelemetrySink for PostgresParserTelemetrySink {
    async fn record(&self, invocation: ParserInvocationRecord) -> Result<(), ApplicationError> {
        let started = Instant::now();
        let outcome = match invocation.status.as_str() {
            "succeeded" => "succeeded",
            "failed" => "failed",
            _ => "other",
        };
        let retry_class = if invocation.retry_count == 0 {
            "none"
        } else {
            "bounded_retry"
        };
        let error_class = invocation
            .error_code
            .as_deref()
            .map_or("none", parser_error_class);
        counter!(
            "nutrition_parser_invocations_total",
            "mode" => "hosted",
            "outcome" => outcome,
            "retry_class" => retry_class,
            "error_class" => error_class
        )
        .increment(1);
        histogram!(
            "nutrition_parser_duration_seconds",
            "mode" => "hosted",
            "outcome" => outcome
        )
        .record(
            f64::from(u32::try_from(invocation.latency_ms.max(0)).unwrap_or(u32::MAX)) / 1_000.0,
        );
        if error_class == "circuit_open" {
            gauge!("nutrition_parser_circuit_open").set(1.0);
        } else if outcome == "succeeded" {
            gauge!("nutrition_parser_circuit_open").set(0.0);
        }
        let result = sqlx::query(
            r"
            INSERT INTO ops.parser_invocation (
                id, provider, model, prompt_version, schema_version,
                latency_ms, retry_count, input_tokens, output_tokens,
                output_sha256, status, error_code
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            ",
        )
        .bind(Uuid::now_v7())
        .bind(invocation.provider)
        .bind(invocation.model)
        .bind(invocation.prompt_version)
        .bind(invocation.schema_version)
        .bind(invocation.latency_ms)
        .bind(invocation.retry_count)
        .bind(invocation.input_tokens)
        .bind(invocation.output_tokens)
        .bind(invocation.output_sha256)
        .bind(invocation.status)
        .bind(invocation.error_code)
        .execute(&self.pool)
        .await;
        crate::telemetry::record_db_operation(
            "parser_telemetry_insert",
            started,
            if result.is_ok() { "success" } else { "failure" },
        );
        result
            .map(|_| ())
            .map_err(|_| ApplicationError::Persistence)
    }
}

fn parser_error_class(error_code: &str) -> &'static str {
    match error_code {
        "provider_circuit_open" => "circuit_open",
        "provider_timeout" => "timeout",
        "provider_schema_validation_failed" => "schema_validation",
        "provider_usage_invalid" => "usage_invalid",
        "provider_http_4xx" => "provider_client_error",
        "provider_http_5xx" => "provider_server_error",
        _ => "other",
    }
}
