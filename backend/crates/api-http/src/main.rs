use adapters::FixtureParser;
use application::{
    AnalysisRequest, AnalysisSnapshot, AnalysisSnapshotReader, AnalyzeMeal, ApplicationError,
    BehaviorVersions, MealAnalysisService,
};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use domain::{AnalysisId, NutrientCode};
use persistence_postgres::{
    PostgresAnalysisRepository, PostgresCatalogEvidenceProvider, PostgresPortionEvidenceProvider,
    active_catalog_release_id,
};
use serde::Serialize;
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
    reader: Arc<dyn AnalysisSnapshotReader>,
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

    let analyzer = MealAnalysisService::new(
        FixtureParser,
        PostgresCatalogEvidenceProvider::new(pool.clone()),
        PostgresPortionEvidenceProvider::new(pool.clone()),
        repository.clone(),
        versions,
        required_nutrients(),
    );
    let state = AppState {
        analyzer: Arc::new(analyzer),
        reader: Arc::new(repository),
        pool,
    };
    let request_id_header = axum::http::HeaderName::from_static("x-request-id");
    let app = Router::new()
        .route("/health/live", get(live))
        .route("/health/ready", get(ready))
        .route("/v1/nutrition/analyses", post(analyze))
        .route("/v1/nutrition/analyses/{analysis_id}", get(find_analysis))
        .with_state(state)
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
    Json(request): Json<AnalysisRequest>,
) -> Result<Json<AnalysisSnapshot>, ApiError> {
    state
        .analyzer
        .execute(request)
        .await
        .map(Json)
        .map_err(ApiError)
}

async fn find_analysis(
    State(state): State<AppState>,
    Path(analysis_id): Path<String>,
) -> Result<Json<AnalysisSnapshot>, ApiError> {
    let analysis_id = analysis_id.parse::<AnalysisId>().map_err(|_| {
        ApiError(ApplicationError::InvalidInput(
            "invalid analysis ID".to_owned(),
        ))
    })?;
    state
        .reader
        .find(analysis_id)
        .await
        .and_then(|snapshot| snapshot.ok_or(ApplicationError::NotFound))
        .map(Json)
        .map_err(ApiError)
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
