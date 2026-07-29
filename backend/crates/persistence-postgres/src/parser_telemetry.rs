use application::{ApplicationError, ParserInvocationRecord, ParserTelemetrySink};
use async_trait::async_trait;
use sqlx::PgPool;
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
        sqlx::query(
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
        .await
        .map_err(|_| ApplicationError::Persistence)?;
        Ok(())
    }
}
