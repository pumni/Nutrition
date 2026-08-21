use std::process::ExitCode;
use tracing::error;

mod config;
mod loop_runner;
mod runtime;

#[tokio::main]
async fn main() -> ExitCode {
    match runtime::run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            error!(error = %error, "worker startup failed");
            ExitCode::FAILURE
        }
    }
}
