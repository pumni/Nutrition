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
    let router = Router::new()
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
        .with_state(state);
    apply_http_observability(router)
}

fn apply_http_observability<S>(router: Router<S>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    let request_id_header = HeaderName::from_static("x-request-id");
    router
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
    use super::{apply_http_observability, build_router};
    use crate::{
        app::{ApiError, AppState},
        auth::Authenticator,
    };
    use application::{
        AnalysisListEntry, AnalysisListQuery, AnalysisOutcome, AnalysisRequest, AnalysisSnapshot,
        AnalysisSnapshotReader, AnalysisWorkflow, AnalyzeMeal, AnswerClarification,
        ApplicationError, ClarificationAnswerRequest, CorrectAnalysis, CorrectionRequest,
    };
    use async_trait::async_trait;
    use axum::{
        Router,
        body::Body,
        http::{Method, Request, StatusCode},
        routing::post,
    };
    use domain::{AnalysisId, AnalysisRevisionId, UserId};
    use persistence_postgres::PostgresAnalysisRepository;
    use serde_json::Value;
    use sqlx::postgres::PgPoolOptions;
    use std::{
        io::{self, Write},
        sync::{Arc, Mutex},
    };
    use tower::ServiceExt;
    use tracing_subscriber::fmt::MakeWriter;

    const AUTH_SENTINEL: &str = "AUTH_SENTINEL_DO_NOT_LOG";
    const MEAL_SENTINEL: &str = "MEAL_SENTINEL_DO_NOT_LOG";
    const PROVIDER_SENTINEL: &str = "PROVIDER_RESPONSE_SENTINEL_DO_NOT_LOG";
    const SECRET_SENTINEL: &str = "SECRET_SENTINEL_DO_NOT_LOG";
    const CORRELATION_REQUEST_ID: &str = "0198f100-0000-7000-8000-000000000098";
    const VALID_AUTHORIZATION: &str = "Bearer dev:0198f100-0000-7000-8000-000000000098";
    const INVALID_AUTHORIZATION: &str = "Bearer AUTH_SENTINEL_DO_NOT_LOG";

    #[derive(Clone, Default)]
    struct LogCapture(Arc<Mutex<Vec<u8>>>);

    impl LogCapture {
        fn text(&self) -> String {
            String::from_utf8(
                self.0
                    .lock()
                    .expect("log capture mutex must not be poisoned")
                    .clone(),
            )
            .expect("captured tracing output must be UTF-8")
        }
    }

    struct LogWriter {
        buffer: Arc<Mutex<Vec<u8>>>,
    }

    impl Write for LogWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.buffer
                .lock()
                .expect("log capture mutex must not be poisoned")
                .extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for LogCapture {
        type Writer = LogWriter;

        fn make_writer(&'a self) -> Self::Writer {
            LogWriter {
                buffer: Arc::clone(&self.0),
            }
        }
    }

    #[derive(Clone, Default)]
    struct FailingServices;

    #[async_trait]
    impl AnalyzeMeal for FailingServices {
        async fn execute(
            &self,
            _request: AnalysisRequest,
        ) -> Result<AnalysisOutcome, ApplicationError> {
            Err(ApplicationError::ParserUnavailable(
                PROVIDER_SENTINEL.to_owned(),
            ))
        }
    }

    #[async_trait]
    impl AnswerClarification for FailingServices {
        async fn answer(
            &self,
            _analysis_id: AnalysisId,
            _request: ClarificationAnswerRequest,
        ) -> Result<AnalysisSnapshot, ApplicationError> {
            Err(ApplicationError::Persistence)
        }
    }

    #[async_trait]
    impl CorrectAnalysis for FailingServices {
        async fn correct(
            &self,
            _analysis_id: AnalysisId,
            _request: CorrectionRequest,
        ) -> Result<AnalysisSnapshot, ApplicationError> {
            Err(ApplicationError::Persistence)
        }
    }

    #[async_trait]
    impl AnalysisSnapshotReader for FailingServices {
        async fn find(
            &self,
            _analysis_id: AnalysisId,
        ) -> Result<Option<AnalysisSnapshot>, ApplicationError> {
            Err(ApplicationError::Persistence)
        }

        async fn find_revision(
            &self,
            _analysis_id: AnalysisId,
            _revision_number: u32,
        ) -> Result<Option<Value>, ApplicationError> {
            Err(ApplicationError::Persistence)
        }

        async fn current_revision_id(
            &self,
            _analysis_id: AnalysisId,
        ) -> Result<Option<AnalysisRevisionId>, ApplicationError> {
            Err(ApplicationError::Persistence)
        }

        async fn list(
            &self,
            _user_id: UserId,
            _query: AnalysisListQuery,
        ) -> Result<Vec<AnalysisListEntry>, ApplicationError> {
            Err(ApplicationError::Persistence)
        }

        async fn workflow(
            &self,
            _analysis_id: AnalysisId,
        ) -> Result<Option<AnalysisWorkflow>, ApplicationError> {
            Err(ApplicationError::Persistence)
        }
    }

    fn test_state() -> AppState {
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect_lazy("postgres://nutrition:nutrition@127.0.0.1:1/nutrition")
            .expect("lazy test pool must be constructible");
        let repository = PostgresAnalysisRepository::new(pool.clone());
        AppState {
            authenticator: Authenticator::Development,
            analyzer: Arc::new(FailingServices),
            clarification: Arc::new(FailingServices),
            correction: Arc::new(FailingServices),
            reader: Arc::new(FailingServices),
            repository,
            pool,
            cursor_hmac_secret: Arc::new(vec![b'c'; 32]),
        }
    }

    fn request(
        method: Method,
        uri: &str,
        headers: &[(&str, &str)],
        body: impl Into<Body>,
    ) -> Request<Body> {
        let mut builder = Request::builder().method(method).uri(uri);
        for &(name, value) in headers {
            builder = builder.header(name, value);
        }
        builder
            .body(body.into())
            .expect("test request must be valid")
    }

    async fn send(app: &Router, request: Request<Body>) -> StatusCode {
        app.clone()
            .oneshot(request)
            .await
            .expect("test router must produce a response")
            .status()
    }

    #[allow(clippy::unused_async)]
    async fn parser_failure() -> Result<StatusCode, ApiError> {
        Err(ApiError(ApplicationError::ParserUnavailable(
            PROVIDER_SENTINEL.to_owned(),
        )))
    }

    #[allow(clippy::too_many_lines)]
    #[tokio::test(flavor = "current_thread")]
    async fn runtime_http_logs_exclude_sensitive_sentinels_across_representative_paths() {
        let capture = LogCapture::default();
        let subscriber = tracing_subscriber::fmt()
            .json()
            .with_ansi(false)
            .with_current_span(true)
            .with_span_list(false)
            .with_max_level(tracing::Level::TRACE)
            .with_writer(capture.clone())
            .finish();
        let default_guard = tracing::subscriber::set_default(subscriber);
        let app = build_router(test_state());

        assert_eq!(
            send(
                &app,
                request(
                    Method::GET,
                    "/health/live",
                    &[("x-request-id", CORRELATION_REQUEST_ID)],
                    Body::empty(),
                ),
            )
            .await,
            StatusCode::OK
        );
        assert_eq!(
            send(
                &app,
                request(
                    Method::GET,
                    "/v1/nutrition/analyses",
                    &[("authorization", INVALID_AUTHORIZATION)],
                    Body::empty(),
                ),
            )
            .await,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            send(
                &app,
                request(
                    Method::POST,
                    "/v1/nutrition/analyses",
                    &[
                        ("authorization", VALID_AUTHORIZATION),
                        ("content-type", "application/json"),
                    ],
                    format!(r#"{{"text":"{MEAL_SENTINEL}"}}"#),
                ),
            )
            .await,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            send(
                &app,
                request(
                    Method::POST,
                    "/v1/nutrition/analyses",
                    &[
                        ("authorization", VALID_AUTHORIZATION),
                        ("content-type", "application/json"),
                        ("idempotency-key", "runtime-log-capture-db-failure"),
                        ("x-provider-secret", SECRET_SENTINEL),
                    ],
                    format!(r#"{{"text":"{MEAL_SENTINEL}","locale":"vi-VN","mode":"balanced"}}"#),
                ),
            )
            .await,
            StatusCode::INTERNAL_SERVER_ERROR
        );

        let parser_app = apply_http_observability(
            Router::new().route("/mock-parser-failure", post(parser_failure)),
        );
        assert_eq!(
            send(
                &parser_app,
                request(
                    Method::POST,
                    "/mock-parser-failure",
                    &[("authorization", INVALID_AUTHORIZATION)],
                    format!(r#"{{"text":"{MEAL_SENTINEL}"}}"#),
                ),
            )
            .await,
            StatusCode::SERVICE_UNAVAILABLE
        );
        drop(default_guard);

        let logs = capture.text();
        assert!(
            logs.contains("http_request"),
            "safe request spans must be captured"
        );
        assert!(
            logs.contains(CORRELATION_REQUEST_ID),
            "safe request correlation must remain observable"
        );
        for sentinel in [
            AUTH_SENTINEL,
            MEAL_SENTINEL,
            PROVIDER_SENTINEL,
            SECRET_SENTINEL,
        ] {
            assert!(
                !logs.contains(sentinel),
                "structured logs leaked sensitive sentinel {sentinel}: {logs}"
            );
        }
    }

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
