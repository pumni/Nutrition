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
        let (status, code) = match &self.0 {
            ApplicationError::InvalidInput(_) => (StatusCode::BAD_REQUEST, "invalid_request"),
            ApplicationError::ParserUnavailable(_) => {
                (StatusCode::SERVICE_UNAVAILABLE, "parser_unavailable")
            }
            ApplicationError::InsufficientEvidence(_) => {
                (StatusCode::UNPROCESSABLE_ENTITY, "analysis_insufficient")
            }
            ApplicationError::Calculation(_) | ApplicationError::Persistence => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
            }
            ApplicationError::NotFound => (StatusCode::NOT_FOUND, "analysis_not_found"),
            ApplicationError::RevisionConflict => (StatusCode::CONFLICT, "revision_conflict"),
            ApplicationError::StaleClarification => (StatusCode::CONFLICT, "stale_clarification"),
            ApplicationError::IdempotencyConflict => (StatusCode::CONFLICT, "idempotency_conflict"),
            ApplicationError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            ApplicationError::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
        };
        (
            status,
            Json(ErrorEnvelope {
                error: ErrorBody {
                    code,
                    message: self.0.to_string(),
                },
            }),
        )
            .into_response()
    }
}
