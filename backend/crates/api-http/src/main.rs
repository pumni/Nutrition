use metrics_exporter_prometheus::PrometheusBuilder;
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    initialize_tracing();
    initialize_metrics(api_http::config::metrics_bind_addr());

    let (address, state) = api_http::config::build().await;
    let app = api_http::router::build_router(state);
    let listener = TcpListener::bind(address)
        .await
        .expect("failed to bind HTTP listener");
    info!(%address, "nutrition API listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("HTTP server failed");
}

fn initialize_metrics(address: std::net::SocketAddr) {
    let builder = PrometheusBuilder::new()
        .set_buckets(&[
            0.005, 0.01, 0.025, 0.05, 0.1, 0.2, 0.3, 0.5, 0.75, 1.0, 2.0, 4.0, 6.0, 8.0, 10.0,
            15.0, 30.0,
        ])
        .expect("default Prometheus histogram buckets are valid");
    builder
        .with_http_listener(address)
        .install()
        .expect("failed to install Prometheus metrics exporter");
}

fn initialize_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .json()
        .with_current_span(true)
        .with_span_list(false)
        .init();
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}
