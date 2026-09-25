use metrics_exporter_prometheus::PrometheusBuilder;
use std::process::ExitCode;
use tokio::net::TcpListener;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> ExitCode {
    initialize_tracing();
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            error!(error = %error, "API startup failed");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), api_http::config::StartupError> {
    let metrics_address =
        api_http::config::metrics_bind_addr().map_err(api_http::config::StartupError::Config)?;
    initialize_metrics(metrics_address)?;

    let (address, state) = api_http::config::build().await?;
    let app = api_http::router::build_router(state);
    let listener = TcpListener::bind(address)
        .await
        .map_err(|_| api_http::config::StartupError::HttpListener)?;
    info!(%address, "nutrition API listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|_| api_http::config::StartupError::HttpServer)
}

fn initialize_metrics(address: std::net::SocketAddr) -> Result<(), api_http::config::StartupError> {
    let builder = PrometheusBuilder::new()
        .set_buckets(&[
            0.005, 0.01, 0.025, 0.05, 0.1, 0.2, 0.3, 0.5, 0.75, 1.0, 2.0, 4.0, 6.0, 8.0, 10.0,
            15.0, 30.0,
        ])
        .map_err(|_| api_http::config::StartupError::Metrics)?;
    builder
        .with_http_listener(address)
        .install()
        .map_err(|_| api_http::config::StartupError::Metrics)
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

#[cfg(any(unix, test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ShutdownSignal {
    CtrlC,
    Sigterm,
}

#[cfg(any(unix, test))]
async fn wait_for_shutdown_signal<C, T>(ctrl_c: C, sigterm: T) -> ShutdownSignal
where
    C: std::future::Future<Output = ()>,
    T: std::future::Future<Output = ()>,
{
    tokio::select! {
        () = ctrl_c => ShutdownSignal::CtrlC,
        () = sigterm => ShutdownSignal::Sigterm,
    }
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut terminate =
            match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
                Ok(terminate) => terminate,
                Err(error) => {
                    error!(%error, "SIGTERM listener unavailable; waiting for Ctrl-C only");
                    let _ = tokio::signal::ctrl_c().await;
                    return;
                }
            };
        let signal = wait_for_shutdown_signal(
            async {
                let _ = tokio::signal::ctrl_c().await;
            },
            async {
                let _ = terminate.recv().await;
            },
        )
        .await;
        match signal {
            ShutdownSignal::CtrlC => info!("Ctrl-C received; shutting down"),
            ShutdownSignal::Sigterm => info!("SIGTERM received; shutting down"),
        }
    }

    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

#[cfg(test)]
mod tests {
    use super::{ShutdownSignal, wait_for_shutdown_signal};

    #[tokio::test]
    async fn ctrl_c_can_trigger_graceful_shutdown() {
        assert_eq!(
            wait_for_shutdown_signal(async {}, std::future::pending()).await,
            ShutdownSignal::CtrlC
        );
    }

    #[tokio::test]
    async fn sigterm_can_trigger_graceful_shutdown() {
        assert_eq!(
            wait_for_shutdown_signal(std::future::pending(), async {}).await,
            ShutdownSignal::Sigterm
        );
    }
}
