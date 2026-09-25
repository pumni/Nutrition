//! Stable hosted parser error classification responsibility.

#![allow(clippy::wildcard_imports)]

use super::*;
use crate::{StructuredModelError, StructuredModelErrorClassification};

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

pub(crate) fn classify_reqwest_error(error: &reqwest::Error) -> StructuredModelError {
    StructuredModelError::new(
        if error.is_timeout() || error.is_connect() {
            StructuredModelErrorClassification::Transient
        } else {
            StructuredModelErrorClassification::Permanent
        },
        if error.is_timeout() {
            "provider_timeout"
        } else {
            "provider_transport_error"
        }
        .to_owned(),
    )
}
