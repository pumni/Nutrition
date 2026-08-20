use crate::config::WorkerConfig;
use persistence_postgres::{claim_jobs, complete_job, deliver_outbox_batch, fail_job};
use sqlx::PgPool;
use std::time::Instant;
use tracing::{info, warn};

pub(crate) async fn process_once(pool: &PgPool, config: &WorkerConfig) -> u64 {
    let outbox_started = Instant::now();
    let delivered = deliver_outbox_batch(pool, &config.worker_id, config.batch_size)
        .await
        .expect("outbox delivery failed");
    metrics::counter!(
        "nutrition_worker_outbox_events_total",
        "outcome" => if delivered == 0 { "empty" } else { "delivered" }
    )
    .increment(if delivered == 0 { 1 } else { delivered });
    metrics::histogram!("nutrition_worker_outbox_duration_seconds")
        .record(outbox_started.elapsed().as_secs_f64());
    let claim_started = Instant::now();
    let jobs = claim_jobs(pool, &config.worker_id, config.batch_size)
        .await
        .expect("job claim failed");
    metrics::counter!("nutrition_worker_jobs_claimed_total").increment(jobs.len() as u64);
    metrics::histogram!("nutrition_worker_claim_duration_seconds")
        .record(claim_started.elapsed().as_secs_f64());
    for job in &jobs {
        if job.job_type == "foundation_noop" {
            complete_job(pool, job.id)
                .await
                .expect("noop job completion failed");
            metrics::counter!(
                "nutrition_worker_jobs_total",
                "job_class" => "foundation_noop",
                "outcome" => "completed"
            )
            .increment(1);
        } else {
            warn!(job_class = "unsupported", "unsupported worker job type");
            fail_job(pool, job, "unsupported_job_type")
                .await
                .expect("job failure transition failed");
            let outcome = if job.attempts >= job.max_attempts {
                "dead"
            } else {
                "retry"
            };
            metrics::counter!(
                "nutrition_worker_jobs_total",
                "job_class" => "unsupported",
                "outcome" => outcome
            )
            .increment(1);
            if outcome == "dead" {
                metrics::counter!(
                    "nutrition_worker_dead_jobs_total",
                    "job_class" => "unsupported"
                )
                .increment(1);
            }
        }
    }
    let processed = delivered + u64::try_from(jobs.len()).expect("job batch length fits u64");
    metrics::counter!("nutrition_worker_batches_total", "outcome" => "success").increment(1);
    processed
}

pub(crate) async fn run_loop(pool: &PgPool, config: &WorkerConfig) {
    info!(worker_id = %config.worker_id, "worker claim loop started");
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("worker graceful shutdown requested");
                break;
            }
            () = tokio::time::sleep(config.poll_interval) => {
                let processed = process_once(pool, config).await;
                if processed > 0 {
                    info!(processed, "worker batch completed");
                }
            }
        }
    }
}
