use crate::{app::AppState, handlers};
use axum::{
    Router,
    extract::DefaultBodyLimit,
    http::HeaderName,
    routing::{delete, get, post},
};
use tower_http::{
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::TraceLayer,
};

pub(crate) fn build_router(state: AppState) -> Router {
    let request_id_header = HeaderName::from_static("x-request-id");
    Router::new()
        .route("/health/live", get(handlers::live))
        .route("/health/ready", get(handlers::ready))
        .route("/v1/nutrition/analyses", post(handlers::analyze))
        .route(
            "/v1/nutrition/me",
            delete(handlers::delete_user_data_handler),
        )
        .route(
            "/v1/nutrition/me/export",
            get(handlers::export_user_data_handler),
        )
        .route(
            "/v1/nutrition/analyses/{analysis_id}",
            get(handlers::find_analysis),
        )
        .route(
            "/v1/nutrition/analyses/{analysis_id}/clarifications",
            post(handlers::answer_clarification),
        )
        .route(
            "/v1/nutrition/analyses/{analysis_id}/corrections",
            post(handlers::correct_analysis),
        )
        .route(
            "/v1/nutrition/analyses/{analysis_id}/revisions/{revision_number}",
            get(handlers::find_revision),
        )
        .with_state(state)
        .layer(DefaultBodyLimit::max(16 * 1024))
        .layer(PropagateRequestIdLayer::new(request_id_header.clone()))
        .layer(TraceLayer::new_for_http())
        .layer(SetRequestIdLayer::new(request_id_header, MakeRequestUuid))
}
