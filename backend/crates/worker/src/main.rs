mod config;
mod loop_runner;
mod runtime;

#[tokio::main]
async fn main() {
    runtime::run().await;
}
