use application::ApplicationError;
use metrics::{counter, histogram};
use sqlx::PgPool;
use std::future::Future;
use std::time::Instant;

pub(crate) fn record_db_operation(
    operation: &'static str,
    started: Instant,
    outcome: &'static str,
) {
    counter!("nutrition_db_operations_total", "operation" => operation, "outcome" => outcome)
        .increment(1);
    histogram!("nutrition_db_operation_duration_seconds", "operation" => operation)
        .record(started.elapsed().as_secs_f64());
}

pub(crate) fn record_pool_metrics(pool: &PgPool) {
    metrics::gauge!("nutrition_db_pool_size").set(f64::from(pool.size()));
    let idle_connections = u32::try_from(pool.num_idle()).unwrap_or(u32::MAX);
    metrics::gauge!("nutrition_db_pool_idle").set(f64::from(idle_connections));
}

pub(crate) async fn observe_db_future<T, F>(
    operation: &'static str,
    future: F,
) -> Result<T, ApplicationError>
where
    F: Future<Output = Result<T, ApplicationError>>,
{
    let started = Instant::now();
    let result = future.await;
    record_db_operation(
        operation,
        started,
        if result.is_ok() { "success" } else { "failure" },
    );
    result
}
