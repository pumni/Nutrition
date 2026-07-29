use adapters::{FixtureCatalog, FixtureParser, InMemoryAnalysisRepository};
use application::{
    AnalysisRequest, AnalysisSnapshot, AnalyzeMeal, ApplicationError, BehaviorVersions,
    DirectAnalysisService,
};
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use domain::NutrientCode;
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

    let analyzer = DirectAnalysisService::new(
        FixtureParser,
        FixtureCatalog::foundation_seed(),
        InMemoryAnalysisRepository::default(),
        BehaviorVersions::default(),
        required_nutrients(),
    );
    let state = AppState {
        analyzer: Arc::new(analyzer),
    };
    let request_id_header = axum::http::HeaderName::from_static("x-request-id");
    let app = Router::new()
        .route("/health/live", get(live))
        .route("/v1/nutrition/analyses", post(analyze))
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
