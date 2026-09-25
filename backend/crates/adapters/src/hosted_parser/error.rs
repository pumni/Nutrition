//! Hosted parser terminal-failure handling responsibility.

#![allow(clippy::wildcard_imports)]

use super::*;
use crate::StructuredModelError;

impl HostedMealParser {
    pub(crate) async fn fail_model_error(
        &self,
        started: Instant,
        retry_count: i32,
        error: StructuredModelError,
    ) -> ApplicationError {
        if error.classification == StructuredModelErrorClassification::CapacityRejected {
            self.fail_without_circuit(started, retry_count, error.code().to_owned())
                .await
        } else {
            self.fail(
                started,
                retry_count,
                (None, None),
                None,
                error.code().to_owned(),
            )
            .await
        }
    }

    pub(crate) async fn fail(
        &self,
        started: Instant,
        retry_count: i32,
        usage: (Option<i64>, Option<i64>),
        output_sha256: Option<String>,
        error_code: String,
    ) -> ApplicationError {
        self.record_failure().await;
        self.emit_terminal_failure(started, retry_count, usage, output_sha256, error_code)
            .await
    }

    pub(crate) async fn fail_without_circuit(
        &self,
        started: Instant,
        retry_count: i32,
        error_code: String,
    ) -> ApplicationError {
        self.emit_terminal_failure(started, retry_count, (None, None), None, error_code)
            .await
    }

    async fn emit_terminal_failure(
        &self,
        started: Instant,
        retry_count: i32,
        usage: (Option<i64>, Option<i64>),
        output_sha256: Option<String>,
        error_code: String,
    ) -> ApplicationError {
        self.emit_telemetry(
            started,
            retry_count,
            usage,
            output_sha256,
            Some(error_code.clone()),
        )
        .await;
        ApplicationError::ParserUnavailable(error_code)
    }
}
