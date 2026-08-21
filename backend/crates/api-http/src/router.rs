use crate::{app::AppState, handlers};
use axum::{
    Router,
    extract::DefaultBodyLimit,
    http::HeaderName,
    middleware,
    routing::{delete, get, post},
};
use tower_http::{
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::TraceLayer,
};

pub fn build_router(state: AppState) -> Router {
    let request_id_header = HeaderName::from_static("x-request-id");
    Router::new()
        .route("/health/live", get(handlers::live))
        .route("/health/ready", get(handlers::ready))
        .route(
            "/v1/nutrition/analyses",
            get(handlers::list_analyses).post(handlers::analyze),
        )
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
            "/v1/nutrition/analyses/{analysis_id}/workflow",
            get(handlers::find_workflow),
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
        .layer(
            TraceLayer::new_for_http().make_span_with(|request: &axum::extract::Request| {
                let request_id = crate::observability::safe_request_id(request);
                tracing::info_span!(
                    "http_request",
                    method = %request.method(),
                    request_id = %request_id
                )
            }),
        )
        .layer(middleware::from_fn(crate::observability::observe_http))
        .layer(SetRequestIdLayer::new(request_id_header, MakeRequestUuid))
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    #[test]
    fn hand_authored_openapi_covers_the_approved_v1_surface() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../openapi/nutrition-api-v1.json"
        );
        let document: Value = serde_json::from_str(
            &std::fs::read_to_string(path).expect("OpenAPI contract must be readable"),
        )
        .expect("OpenAPI contract must be valid JSON");
        assert_eq!(
            document["info"]["x-owner-decision-ref"],
            "docs/decisions/product-api-v1.md#adr-product-api-v1"
        );
        let paths = document["paths"].as_object().expect("paths object");
        assert!(paths.contains_key("/v1/nutrition/analyses"));
        assert!(paths.contains_key("/v1/nutrition/analyses/{analysis_id}/workflow"));
        for path in [
            "/v1/nutrition/analyses",
            "/v1/nutrition/analyses/{analysis_id}/clarifications",
            "/v1/nutrition/analyses/{analysis_id}/corrections",
        ] {
            let parameters = document["paths"][path]["post"]["parameters"]
                .as_array()
                .expect("mutation parameters");
            assert!(parameters.iter().any(|parameter| {
                parameter["$ref"] == "#/components/parameters/IdempotencyKey"
            }));
        }
        assert_eq!(
            document["components"]["parameters"]["PageSize"]["schema"]["default"],
            20
        );
        assert_eq!(
            document["components"]["parameters"]["PageSize"]["schema"]["maximum"],
            50
        );
    }
}
