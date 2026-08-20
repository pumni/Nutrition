use crate::app::{ApiError, AppState, HealthResponse};
use application::{
    AnalysisRequest, AnalysisSnapshot, ClarificationAnswerRequest, CorrectionRequest,
    IdempotencyContext,
};
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use domain::{AnalysisId, UserId};
use serde::Serialize;
use sha2::{Digest, Sha256};

pub(crate) async fn live() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        application_version: env!("CARGO_PKG_VERSION"),
    })
}

pub(crate) async fn ready(State(state): State<AppState>) -> Response {
    match sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.pool)
        .await
    {
        Ok(_) => (
            StatusCode::OK,
            Json(HealthResponse {
                status: "ready",
                application_version: env!("CARGO_PKG_VERSION"),
            }),
        )
            .into_response(),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthResponse {
                status: "not_ready",
                application_version: env!("CARGO_PKG_VERSION"),
            }),
        )
            .into_response(),
    }
}

pub(crate) async fn analyze(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(mut request): Json<AnalysisRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticate(&state, &headers).await?;
    request.owner_id = Some(principal);
    let scope = format!("user:{principal}:create");
    if let Some(key) = idempotency_key(&headers)? {
        let request_hash = json_hash(&request)?;
        if let Some(response) = state
            .repository
            .find_idempotent_response(&scope, &key, &request_hash)
            .await
            .map_err(ApiError)?
        {
            return Ok(Json(response));
        }
        request.idempotency = Some(IdempotencyContext {
            scope_key: scope,
            key,
            request_hash,
        });
    }
    state
        .analyzer
        .execute(request)
        .await
        .and_then(to_json_value)
        .map(Json)
        .map_err(ApiError)
}

pub(crate) async fn export_user_data_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticate(&state, &headers).await?;
    persistence_postgres::export_user_data(&state.pool, principal)
        .await
        .map(Json)
        .map_err(|_| ApiError(application::ApplicationError::Persistence))
}

pub(crate) async fn delete_user_data_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticate(&state, &headers).await?;
    let request_reference = format!("privacy-delete-{}", uuid::Uuid::now_v7());
    persistence_postgres::delete_user_data(&state.pool, principal, &request_reference)
        .await
        .map_err(|_| application::ApplicationError::Persistence)
        .and_then(to_json_value)
        .map(Json)
        .map_err(ApiError)
}

pub(crate) async fn answer_clarification(
    State(state): State<AppState>,
    Path(analysis_id): Path<String>,
    headers: HeaderMap,
    Json(request): Json<ClarificationAnswerRequest>,
) -> Result<Json<AnalysisSnapshot>, ApiError> {
    let analysis_id = parse_analysis_id(&analysis_id)?;
    authorize(&state, analysis_id, authenticate(&state, &headers).await?).await?;
    state
        .clarification
        .answer(analysis_id, request)
        .await
        .map(Json)
        .map_err(ApiError)
}

pub(crate) async fn correct_analysis(
    State(state): State<AppState>,
    Path(analysis_id): Path<String>,
    headers: HeaderMap,
    Json(mut request): Json<CorrectionRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let analysis_id = parse_analysis_id(&analysis_id)?;
    let principal = authenticate(&state, &headers).await?;
    authorize(&state, analysis_id, principal).await?;
    let scope = format!("user:{principal}:correction:{analysis_id}");
    if let Some(key) = idempotency_key(&headers)? {
        let request_hash = json_hash(&request)?;
        if let Some(response) = state
            .repository
            .find_idempotent_response(&scope, &key, &request_hash)
            .await
            .map_err(ApiError)?
        {
            return Ok(Json(response));
        }
        request.idempotency = Some(IdempotencyContext {
            scope_key: scope,
            key,
            request_hash,
        });
    }
    state
        .correction
        .correct(analysis_id, request)
        .await
        .and_then(to_json_value)
        .map(Json)
        .map_err(ApiError)
}

pub(crate) async fn find_analysis(
    State(state): State<AppState>,
    Path(analysis_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<AnalysisSnapshot>, ApiError> {
    let analysis_id = parse_analysis_id(&analysis_id)?;
    authorize(&state, analysis_id, authenticate(&state, &headers).await?).await?;
    state
        .reader
        .find(analysis_id)
        .await
        .and_then(|snapshot| snapshot.ok_or(application::ApplicationError::NotFound))
        .map(Json)
        .map_err(ApiError)
}

pub(crate) async fn find_revision(
    State(state): State<AppState>,
    Path((analysis_id, revision_number)): Path<(String, u32)>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let analysis_id = parse_analysis_id(&analysis_id)?;
    authorize(&state, analysis_id, authenticate(&state, &headers).await?).await?;
    state
        .reader
        .find_revision(analysis_id, revision_number)
        .await
        .and_then(|snapshot| snapshot.ok_or(application::ApplicationError::NotFound))
        .map(Json)
        .map_err(ApiError)
}

fn parse_analysis_id(value: &str) -> Result<AnalysisId, ApiError> {
    value.parse::<AnalysisId>().map_err(|_| {
        ApiError(application::ApplicationError::InvalidInput(
            "invalid analysis ID".to_owned(),
        ))
    })
}

async fn authenticate(state: &AppState, headers: &HeaderMap) -> Result<UserId, ApiError> {
    let authorization = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok());
    state
        .authenticator
        .authenticate(authorization, &state.repository)
        .await
        .map_err(ApiError)
}

async fn authorize(
    state: &AppState,
    analysis_id: AnalysisId,
    user_id: UserId,
) -> Result<(), ApiError> {
    if state
        .repository
        .authorize_analysis(analysis_id, user_id)
        .await
        .map_err(ApiError)?
    {
        Ok(())
    } else {
        Err(ApiError(application::ApplicationError::Forbidden))
    }
}

fn idempotency_key(headers: &HeaderMap) -> Result<Option<String>, ApiError> {
    let Some(value) = headers.get("idempotency-key") else {
        return Ok(None);
    };
    let key = value
        .to_str()
        .map_err(|_| {
            ApiError(application::ApplicationError::InvalidInput(
                "Idempotency-Key must be valid ASCII".to_owned(),
            ))
        })?
        .trim();
    if key.is_empty() || key.len() > 128 {
        return Err(ApiError(application::ApplicationError::InvalidInput(
            "Idempotency-Key must contain between 1 and 128 characters".to_owned(),
        )));
    }
    Ok(Some(key.to_owned()))
}

fn json_hash(value: &impl Serialize) -> Result<String, ApiError> {
    let encoded = serde_json::to_vec(value).map_err(|_| {
        ApiError(application::ApplicationError::InvalidInput(
            "request could not be canonicalized".to_owned(),
        ))
    })?;
    Ok(hex::encode(Sha256::digest(encoded)))
}

fn to_json_value(
    value: impl Serialize,
) -> Result<serde_json::Value, application::ApplicationError> {
    serde_json::to_value(value).map_err(|_| application::ApplicationError::Persistence)
}
