#[path = "cursor.rs"]
mod cursor;
#[path = "extractors.rs"]
mod extractors;

use self::{
    cursor::CursorPosition,
    extractors::{ApiJson, ApiPath, ApiQuery},
};
use crate::app::{ApiError, AppState, HealthResponse};
use application::{
    AnalysisListEntry, AnalysisListQuery, AnalysisRequest, AnalysisSnapshot, ApplicationError,
    ClarificationAnswerRequest, CorrectionRequest, IdempotencyContext,
};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use domain::{AnalysisId, UserId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const API_CONTRACT_VERSION: &str = "nutrition-api-v1";

#[derive(Debug, Deserialize)]
pub(crate) struct AnalysisListParams {
    pub(crate) status: Option<String>,
    pub(crate) locale: Option<String>,
    pub(crate) page_size: Option<String>,
    pub(crate) cursor: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct AnalysisListResponse {
    items: Vec<AnalysisListEntry>,
    next_cursor: Option<String>,
}

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
    ApiJson(mut request): ApiJson<AnalysisRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticate(&state, &headers).await?;
    request.owner_id = Some(principal);
    let scope = format!("user:{principal}:create");
    let key = required_idempotency_key(&headers)?;
    let request_hash = json_hash(&request)?;
    if let Some(response) = state
        .repository
        .reserve_idempotency(&scope, &key, &request_hash)
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
    ApiPath(analysis_id): ApiPath<String>,
    headers: HeaderMap,
    ApiJson(mut request): ApiJson<ClarificationAnswerRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let analysis_id = parse_analysis_id(&analysis_id)?;
    let principal = authenticate(&state, &headers).await?;
    authorize(&state, analysis_id, principal).await?;
    let scope = format!("user:{principal}:clarification:{analysis_id}");
    let key = required_idempotency_key(&headers)?;
    let request_hash = json_hash(&request)?;
    if let Some(response) = state
        .repository
        .reserve_idempotency(&scope, &key, &request_hash)
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
    state
        .clarification
        .answer(analysis_id, request)
        .await
        .and_then(to_json_value)
        .map(Json)
        .map_err(ApiError)
}

pub(crate) async fn correct_analysis(
    State(state): State<AppState>,
    ApiPath(analysis_id): ApiPath<String>,
    headers: HeaderMap,
    ApiJson(mut request): ApiJson<CorrectionRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let analysis_id = parse_analysis_id(&analysis_id)?;
    let principal = authenticate(&state, &headers).await?;
    authorize(&state, analysis_id, principal).await?;
    let scope = format!("user:{principal}:correction:{analysis_id}");
    let key = required_idempotency_key(&headers)?;
    let request_hash = json_hash(&request)?;
    if let Some(response) = state
        .repository
        .reserve_idempotency(&scope, &key, &request_hash)
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
    ApiPath(analysis_id): ApiPath<String>,
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

pub(crate) async fn list_analyses(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiQuery(params): ApiQuery<AnalysisListParams>,
) -> Result<Json<AnalysisListResponse>, ApiError> {
    let principal = authenticate(&state, &headers).await?;
    let page_size = parse_page_size(params.page_size.as_deref())?;
    validate_analysis_filters(params.status.as_deref(), params.locale.as_deref())?;
    let now = cursor::now_epoch_seconds().map_err(ApiError)?;
    let (snapshot_epoch_seconds, after_created_at, after_analysis_id) =
        if let Some(encoded) = params.cursor.as_deref() {
            let position = cursor::decode(
                state.cursor_hmac_secret.as_slice(),
                encoded,
                principal,
                params.status.as_deref(),
                params.locale.as_deref(),
                now,
            )
            .map_err(ApiError)?;
            (
                position.snapshot_epoch_seconds,
                Some(position.after_created_at),
                Some(position.after_analysis_id),
            )
        } else {
            (now, None, None)
        };
    let entries = state
        .reader
        .list(
            principal,
            AnalysisListQuery {
                status: params.status.clone(),
                locale: params.locale.clone(),
                snapshot_epoch_seconds,
                after_created_at,
                after_analysis_id,
                limit: i64::from(page_size) + 1,
            },
        )
        .await
        .map_err(ApiError)?;
    let mut items = entries;
    let next_cursor = if items.len() > page_size as usize {
        let last = items.pop().ok_or(ApiError(ApplicationError::Persistence))?;
        Some(
            cursor::encode(
                state.cursor_hmac_secret.as_slice(),
                principal,
                params.status.as_deref(),
                params.locale.as_deref(),
                &CursorPosition {
                    snapshot_epoch_seconds,
                    after_created_at: last.created_at.clone(),
                    after_analysis_id: last.analysis_id,
                },
                now,
            )
            .map_err(ApiError)?,
        )
    } else {
        None
    };
    Ok(Json(AnalysisListResponse { items, next_cursor }))
}

pub(crate) async fn find_workflow(
    State(state): State<AppState>,
    ApiPath(analysis_id): ApiPath<String>,
    headers: HeaderMap,
) -> Result<Json<application::AnalysisWorkflow>, ApiError> {
    let analysis_id = parse_analysis_id(&analysis_id)?;
    authorize(&state, analysis_id, authenticate(&state, &headers).await?).await?;
    state
        .reader
        .workflow(analysis_id)
        .await
        .and_then(|workflow| workflow.ok_or(application::ApplicationError::NotFound))
        .map(Json)
        .map_err(ApiError)
}

pub(crate) async fn find_revision(
    State(state): State<AppState>,
    ApiPath((analysis_id, revision_number)): ApiPath<(String, u32)>,
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

fn required_idempotency_key(headers: &HeaderMap) -> Result<String, ApiError> {
    let Some(value) = headers.get("idempotency-key") else {
        return Err(ApiError(application::ApplicationError::InvalidInput(
            "Idempotency-Key is required".to_owned(),
        )));
    };
    let key = value.to_str().map_err(|_| {
        ApiError(application::ApplicationError::InvalidInput(
            "Idempotency-Key must be valid ASCII".to_owned(),
        ))
    })?;
    if key.is_empty()
        || key.len() > 128
        || !key.is_ascii()
        || key.bytes().any(|byte| !(0x20..=0x7e).contains(&byte))
    {
        return Err(ApiError(application::ApplicationError::InvalidInput(
            "Idempotency-Key must contain 1 to 128 printable ASCII characters".to_owned(),
        )));
    }
    Ok(key.to_owned())
}

fn json_hash(value: &impl Serialize) -> Result<String, ApiError> {
    #[derive(Serialize)]
    struct VersionedRequest<'a, T: Serialize> {
        contract_version: &'static str,
        request: &'a T,
    }
    let encoded = serde_json::to_vec(&VersionedRequest {
        contract_version: API_CONTRACT_VERSION,
        request: value,
    })
    .map_err(|_| {
        ApiError(application::ApplicationError::InvalidInput(
            "request could not be canonicalized".to_owned(),
        ))
    })?;
    Ok(hex::encode(Sha256::digest(encoded)))
}

fn parse_page_size(value: Option<&str>) -> Result<u32, ApiError> {
    let page_size = value
        .map(|value| {
            value.parse::<u32>().map_err(|_| {
                ApiError(application::ApplicationError::InvalidInput(
                    "page_size must be an integer between 1 and 50".to_owned(),
                ))
            })
        })
        .transpose()?
        .unwrap_or(20);
    if !(1..=50).contains(&page_size) {
        return Err(ApiError(application::ApplicationError::InvalidInput(
            "page_size must be an integer between 1 and 50".to_owned(),
        )));
    }
    Ok(page_size)
}

fn validate_analysis_filters(status: Option<&str>, locale: Option<&str>) -> Result<(), ApiError> {
    if let Some(status) = status
        && !matches!(
            status,
            "received"
                | "parsing"
                | "resolving"
                | "needs_clarification"
                | "completed"
                | "insufficient_evidence"
                | "confirmed"
                | "corrected"
                | "abandoned"
        )
    {
        return Err(ApiError(application::ApplicationError::InvalidInput(
            "status is not a supported analysis status".to_owned(),
        )));
    }
    if let Some(locale) = locale
        && (locale.is_empty()
            || locale.len() > 32
            || !locale.is_ascii()
            || locale.bytes().any(|byte| !(0x20..=0x7e).contains(&byte)))
    {
        return Err(ApiError(application::ApplicationError::InvalidInput(
            "locale must contain 1 to 32 ASCII characters".to_owned(),
        )));
    }
    Ok(())
}

fn to_json_value(
    value: impl Serialize,
) -> Result<serde_json::Value, application::ApplicationError> {
    serde_json::to_value(value).map_err(|_| application::ApplicationError::Persistence)
}
