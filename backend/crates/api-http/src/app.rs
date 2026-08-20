use crate::oidc::Authenticator;
use application::{
    AnalysisSnapshotReader, AnalyzeMeal, AnswerClarification, ApplicationError, CorrectAnalysis,
};
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use persistence_postgres::PostgresAnalysisRepository;
use serde::Serialize;
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) authenticator: Authenticator,
    pub(crate) analyzer: Arc<dyn AnalyzeMeal>,
    pub(crate) clarification: Arc<dyn AnswerClarification>,
    pub(crate) correction: Arc<dyn CorrectAnalysis>,
    pub(crate) reader: Arc<dyn AnalysisSnapshotReader>,
    pub(crate) repository: PostgresAnalysisRepository,
    pub(crate) pool: sqlx::PgPool,
    pub(crate) cursor_hmac_secret: Arc<Vec<u8>>,
}

#[derive(Serialize)]
pub(crate) struct HealthResponse {
    pub(crate) status: &'static str,
    pub(crate) application_version: &'static str,
}

#[derive(Serialize)]
struct ErrorEnvelope {
    error: ErrorBody,
}

#[derive(Serialize)]
struct ErrorBody {
    code: &'static str,
    message: String,
}

pub(crate) struct ApiError(pub(crate) ApplicationError);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match &self.0 {
            ApplicationError::InvalidInput(_) => (
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "invalid request",
            ),
            ApplicationError::InvalidCursor => {
                (StatusCode::BAD_REQUEST, "invalid_cursor", "invalid cursor")
            }
            ApplicationError::ParserUnavailable(_) => (
                StatusCode::SERVICE_UNAVAILABLE,
                "parser_unavailable",
                "parser unavailable",
            ),
            ApplicationError::InsufficientEvidence(_) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "analysis_insufficient",
                "analysis evidence is insufficient",
            ),
            ApplicationError::Calculation(_) | ApplicationError::Persistence => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "internal server error",
            ),
            ApplicationError::NotFound => (
                StatusCode::NOT_FOUND,
                "analysis_not_found",
                "analysis not found",
            ),
            ApplicationError::RevisionConflict => (
                StatusCode::CONFLICT,
                "revision_conflict",
                "analysis revision conflict",
            ),
            ApplicationError::StaleClarification => (
                StatusCode::CONFLICT,
                "stale_clarification",
                "clarification is stale",
            ),
            ApplicationError::IdempotencyConflict => (
                StatusCode::CONFLICT,
                "idempotency_conflict",
                "idempotency key conflict",
            ),
            ApplicationError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "authentication is required",
            ),
            ApplicationError::Forbidden => (
                StatusCode::FORBIDDEN,
                "forbidden",
                "resource access is forbidden",
            ),
        };
        (
            status,
            Json(ErrorEnvelope {
                error: ErrorBody {
                    code,
                    message: message.to_owned(),
                },
            }),
        )
            .into_response()
    }
}
