//! Hosted parser terminal-failure handling responsibility.

#![allow(clippy::wildcard_imports)]

use super::*;
impl HostedMealParser {
    pub(crate) async fn fail(
        &self,
        started: Instant,
        retry_count: i32,
        usage: (Option<i64>, Option<i64>),
        output_sha256: Option<String>,
        error_code: String,
    ) -> ApplicationError {
        self.record_failure().await;
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
