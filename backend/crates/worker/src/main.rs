use persistence_postgres::{claim_jobs, complete_job, deliver_outbox_batch, fail_job};
use sqlx::PgPool;
use std::{env, time::Duration};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .json()
        .init();

    let config = WorkerConfig::from_env();
    let pool = persistence_postgres::connect(&config.database_url, config.database_pool_size)
        .await
        .expect("worker could not connect to PostgreSQL");
    if env::var("RUN_MIGRATIONS").as_deref() == Ok("true") {
        persistence_postgres::migrate(&pool)
            .await
            .expect("database migration failed");
        info!("database migrations applied");
    }
    if env::var("RUN_FOUNDATION_SEED").as_deref() == Ok("true") {
        persistence_postgres::seed_foundation_fixture(&pool)
            .await
            .expect("foundation fixture seed failed");
        info!("test-only foundation fixture seed applied");
    }
    sqlx_healthcheck(&pool).await;
    match config.mode {
        WorkerMode::Idle => info!("worker healthcheck completed"),
        WorkerMode::RunOnce => {
            let processed = process_once(&pool, &config).await;
            info!(processed, "worker run-once completed");
        }
        WorkerMode::Loop => run_loop(&pool, &config).await,
    }
}

async fn sqlx_healthcheck(pool: &sqlx::PgPool) {
    sqlx::query("SELECT 1")
        .execute(pool)
        .await
        .expect("database health check failed");
}

#[derive(Clone, Copy)]
enum WorkerMode {
    Idle,
    RunOnce,
    Loop,
}

struct WorkerConfig {
    database_url: String,
    database_pool_size: u32,
    worker_id: String,
    batch_size: i64,
    poll_interval: Duration,
    mode: WorkerMode,
}

impl WorkerConfig {
    fn from_env() -> Self {
        let mode = match env::var("WORKER_MODE").as_deref() {
            Ok("run-once") => WorkerMode::RunOnce,
            Ok("loop") => WorkerMode::Loop,
            Ok("idle") | Err(_) => WorkerMode::Idle,
            Ok(value) => panic!("unsupported WORKER_MODE: {value}"),
        };
        Self {
            database_url: env::var("DATABASE_URL")
                .expect("DATABASE_URL is required for the worker process"),
            database_pool_size: env_u32("WORKER_DATABASE_POOL_SIZE", 4, 1, 32),
            worker_id: env::var("WORKER_ID").unwrap_or_else(|_| "worker-local".to_owned()),
            batch_size: i64::from(env_u32("WORKER_BATCH_SIZE", 20, 1, 100)),
            poll_interval: Duration::from_millis(u64::from(env_u32(
                "WORKER_POLL_INTERVAL_MS",
                500,
                50,
                60_000,
            ))),
            mode,
        }
    }
}

fn env_u32(name: &str, default: u32, minimum: u32, maximum: u32) -> u32 {
    let value = env::var(name).map_or(default, |raw| {
        raw.parse()
            .unwrap_or_else(|_| panic!("{name} must be an integer"))
    });
    assert!(
        (minimum..=maximum).contains(&value),
        "{name} must be between {minimum} and {maximum}"
    );
    value
}

async fn process_once(pool: &PgPool, config: &WorkerConfig) -> u64 {
    let delivered = deliver_outbox_batch(pool, &config.worker_id, config.batch_size)
        .await
        .expect("outbox delivery failed");
    let jobs = claim_jobs(pool, &config.worker_id, config.batch_size)
        .await
        .expect("job claim failed");
    for job in &jobs {
        if job.job_type == "foundation_noop" {
            complete_job(pool, job.id)
                .await
                .expect("noop job completion failed");
        } else {
            warn!(job_id = %job.id, job_type = %job.job_type, "unsupported job type");
            fail_job(pool, job, "unsupported_job_type")
                .await
                .expect("job failure transition failed");
        }
    }
    delivered + u64::try_from(jobs.len()).expect("job batch length fits u64")
}

async fn run_loop(pool: &PgPool, config: &WorkerConfig) {
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
