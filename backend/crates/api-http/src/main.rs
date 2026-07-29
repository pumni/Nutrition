use adapters::FixtureParser;
use application::{
    AnalysisRequest, AnalysisRevisionService, AnalysisSnapshot, AnalysisSnapshotReader,
    AnalyzeMeal, AnswerClarification, ApplicationError, BehaviorVersions,
    ClarificationAnswerRequest, CorrectAnalysis, CorrectionRequest, IdempotencyContext,
    MealAnalysisService,
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use domain::{AnalysisId, NutrientCode, UserId};
use persistence_postgres::{
    PostgresAnalysisRepository, PostgresCatalogEvidenceProvider, PostgresPortionEvidenceProvider,
    active_catalog_release_id,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{env, net::SocketAddr, sync::Arc};
use tower_http::{
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::TraceLayer,
};
use tracing::info;
use tracing_subscriber::EnvFilter;

#[derive(Clone)]
struct AppState {
    analyzer: Arc<dyn AnalyzeMeal>,
    clarification: Arc<dyn AnswerClarification>,
    correction: Arc<dyn CorrectAnalysis>,
    reader: Arc<dyn AnalysisSnapshotReader>,
    repository: PostgresAnalysisRepository,
    pool: sqlx::PgPool,
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    application_version: &'static str,
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

struct ApiError(ApplicationError);

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

#[tokio::main]
async fn main() {
    initialize_tracing();
    let auth_mode = env::var("AUTH_MODE").expect("AUTH_MODE is required");
    assert!(
        auth_mode == "development",
        "only AUTH_MODE=development is available; production requires an OIDC adapter"
    );
    let bind_addr = env::var("APP_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_owned());
    let address: SocketAddr = bind_addr
        .parse()
        .expect("APP_BIND_ADDR must be a valid socket address");
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL is required");
    let pool = persistence_postgres::connect(&database_url, 8)
        .await
        .expect("API could not connect to PostgreSQL");
    let catalog_release_id = active_catalog_release_id(&pool)
        .await
        .expect("an active catalog release is required");
    let versions = BehaviorVersions {
        catalog_release_id,
        ..BehaviorVersions::default()
    };
    let repository = PostgresAnalysisRepository::new(pool.clone());
    let food_evidence = PostgresCatalogEvidenceProvider::new(pool.clone());
    let portion_evidence = PostgresPortionEvidenceProvider::new(pool.clone());

    let analyzer = MealAnalysisService::new(
        FixtureParser,
        food_evidence.clone(),
        portion_evidence.clone(),
        repository.clone(),
        versions.clone(),
        required_nutrients(),
    );
    let revision_service = Arc::new(AnalysisRevisionService::new(
        food_evidence,
        portion_evidence,
        repository.clone(),
        versions,
        required_nutrients(),
    ));
    let state = AppState {
        analyzer: Arc::new(analyzer),
        clarification: revision_service.clone(),
        correction: revision_service,
        reader: Arc::new(repository.clone()),
        repository,
        pool,
    };
    let request_id_header = axum::http::HeaderName::from_static("x-request-id");
    let app = Router::new()
        .route("/health/live", get(live))
        .route("/health/ready", get(ready))
        .route("/v1/nutrition/analyses", post(analyze))
        .route("/v1/nutrition/analyses/{analysis_id}", get(find_analysis))
        .route(
            "/v1/nutrition/analyses/{analysis_id}/clarifications",
            post(answer_clarification),
        )
        .route(
            "/v1/nutrition/analyses/{analysis_id}/corrections",
            post(correct_analysis),
        )
        .route(
            "/v1/nutrition/analyses/{analysis_id}/revisions/{revision_number}",
            get(find_revision),
        )
        .with_state(state)
        .layer(DefaultBodyLimit::max(16 * 1024))
        .layer(PropagateRequestIdLayer::new(request_id_header.clone()))
        .layer(TraceLayer::new_for_http())
        .layer(SetRequestIdLayer::new(request_id_header, MakeRequestUuid));

    let listener = tokio::net::TcpListener::bind(address)
        .await
        .expect("failed to bind HTTP listener");
    info!(%address, "nutrition API listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("HTTP server failed");
}

async fn live() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        application_version: env!("CARGO_PKG_VERSION"),
    })
}

async fn ready(State(state): State<AppState>) -> Response {
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

async fn analyze(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(mut request): Json<AnalysisRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticate(&headers)?;
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

async fn answer_clarification(
    State(state): State<AppState>,
    Path(analysis_id): Path<String>,
    headers: HeaderMap,
    Json(request): Json<ClarificationAnswerRequest>,
) -> Result<Json<AnalysisSnapshot>, ApiError> {
    let analysis_id = parse_analysis_id(&analysis_id)?;
    authorize(&state, analysis_id, authenticate(&headers)?).await?;
    state
        .clarification
        .answer(analysis_id, request)
        .await
        .map(Json)
        .map_err(ApiError)
}

async fn correct_analysis(
    State(state): State<AppState>,
    Path(analysis_id): Path<String>,
    headers: HeaderMap,
    Json(mut request): Json<CorrectionRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let analysis_id = parse_analysis_id(&analysis_id)?;
    let principal = authenticate(&headers)?;
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

async fn find_analysis(
    State(state): State<AppState>,
    Path(analysis_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<AnalysisSnapshot>, ApiError> {
    let analysis_id = parse_analysis_id(&analysis_id)?;
    authorize(&state, analysis_id, authenticate(&headers)?).await?;
    state
        .reader
        .find(analysis_id)
        .await
        .and_then(|snapshot| snapshot.ok_or(ApplicationError::NotFound))
        .map(Json)
        .map_err(ApiError)
}

async fn find_revision(
    State(state): State<AppState>,
    Path((analysis_id, revision_number)): Path<(String, u32)>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let analysis_id = parse_analysis_id(&analysis_id)?;
    authorize(&state, analysis_id, authenticate(&headers)?).await?;
    state
        .reader
        .find_revision(analysis_id, revision_number)
        .await
        .and_then(|snapshot| snapshot.ok_or(ApplicationError::NotFound))
        .map(Json)
        .map_err(ApiError)
}

fn parse_analysis_id(value: &str) -> Result<AnalysisId, ApiError> {
    value.parse::<AnalysisId>().map_err(|_| {
        ApiError(ApplicationError::InvalidInput(
            "invalid analysis ID".to_owned(),
        ))
    })
}

fn authenticate(headers: &HeaderMap) -> Result<UserId, ApiError> {
    let value = headers
        .get("authorization")
        .ok_or(ApiError(ApplicationError::Unauthorized))?
        .to_str()
        .map_err(|_| ApiError(ApplicationError::Unauthorized))?;
    let user_id = value
        .strip_prefix("Bearer dev:")
        .ok_or(ApiError(ApplicationError::Unauthorized))?;
    user_id
        .parse::<UserId>()
        .map_err(|_| ApiError(ApplicationError::Unauthorized))
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
        Err(ApiError(ApplicationError::Forbidden))
    }
}

fn idempotency_key(headers: &HeaderMap) -> Result<Option<String>, ApiError> {
    let Some(value) = headers.get("idempotency-key") else {
        return Ok(None);
    };
    let key = value
        .to_str()
        .map_err(|_| {
            ApiError(ApplicationError::InvalidInput(
                "Idempotency-Key must be valid ASCII".to_owned(),
            ))
        })?
        .trim();
    if key.is_empty() || key.len() > 128 {
        return Err(ApiError(ApplicationError::InvalidInput(
            "Idempotency-Key must contain between 1 and 128 characters".to_owned(),
        )));
    }
    Ok(Some(key.to_owned()))
}

fn json_hash(value: &impl Serialize) -> Result<String, ApiError> {
    let encoded = serde_json::to_vec(value).map_err(|_| {
        ApiError(ApplicationError::InvalidInput(
            "request could not be canonicalized".to_owned(),
        ))
    })?;
    Ok(hex::encode(Sha256::digest(encoded)))
}

fn to_json_value(value: impl Serialize) -> Result<serde_json::Value, ApplicationError> {
    serde_json::to_value(value).map_err(|_| ApplicationError::Persistence)
}

fn required_nutrients() -> Vec<NutrientCode> {
    ["energy_kcal", "protein_g", "carbohydrate_g", "fat_g"]
        .into_iter()
        .map(|code| NutrientCode::new(code).expect("built-in nutrient code must be valid"))
        .collect()
}

fn initialize_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .json()
        .with_current_span(false)
        .with_span_list(false)
        .init();
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}
