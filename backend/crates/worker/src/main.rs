use persistence_postgres::{
    FdcFoundationImportRequest, claim_jobs, complete_job, deliver_outbox_batch, fail_job,
    import_fdc_foundation_json, run_privacy_retention,
};
use sqlx::PgPool;
use std::{env, fs, time::Duration};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .json()
        .init();

    let environment = AppEnvironment::from_env();
    let config = WorkerConfig::from_env(environment);
    let run_fdc_import = env_bool("RUN_FDC_FOUNDATION_IMPORT", false);
    let run_privacy_cleanup = env_bool("RUN_PRIVACY_RETENTION", false);
    assert!(
        !run_fdc_import || environment.allows_source_import(),
        "RUN_FDC_FOUNDATION_IMPORT=true is forbidden when APP_ENV is production"
    );

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
        assert!(
            environment.allows_development_adapters(),
            "RUN_FOUNDATION_SEED=true is forbidden when APP_ENV is staging or production"
        );
        persistence_postgres::seed_foundation_fixture(&pool)
            .await
            .expect("foundation fixture seed failed");
        info!("test-only foundation fixture seed applied");
    }
    if run_fdc_import {
        run_fdc_foundation_import(&pool)
            .await
            .expect("FDC Foundation import failed");
    }
    if run_privacy_cleanup {
        let report = run_privacy_retention(&pool)
            .await
            .expect("privacy retention job failed");
        info!(
            deleted_parser_telemetry = report.deleted_parser_telemetry,
            deleted_audit_events = report.deleted_audit_events,
            purged_analysis_aggregates = report.purged_analysis_aggregates,
            "privacy retention job completed"
        );
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

async fn run_fdc_foundation_import(
    pool: &PgPool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let source_path = required_env("FDC_IMPORT_PATH");
    let source_bytes = fs::read(&source_path)?;
    let request = FdcFoundationImportRequest {
        release_version: required_env("FDC_IMPORT_RELEASE_VERSION"),
        source_published_date: required_env("FDC_IMPORT_SOURCE_PUBLISHED_DATE"),
        object_uri: required_env("FDC_IMPORT_OBJECT_URI"),
        expected_sha256: required_env("FDC_IMPORT_EXPECTED_SHA256"),
        source_archive_sha256: optional_env("FDC_IMPORT_SOURCE_ARCHIVE_SHA256"),
        preprocessing_policy_version: optional_env("FDC_IMPORT_PREPROCESSING_POLICY"),
        include_fdc_ids: parse_fdc_ids(&required_env("FDC_IMPORT_INCLUDE_IDS"))?,
        created_by: required_env("FDC_IMPORT_CREATED_BY"),
    };
    let report = import_fdc_foundation_json(pool, &source_bytes, &request).await?;
    info!(
        dataset_release_id = %report.dataset_release_id,
        catalog_release_id = %report.catalog_release_id,
        catalog_release_version = %report.catalog_release_version,
        raw_record_count = report.raw_record_count,
        selected_record_count = report.selected_record_count,
        replayed = report.replayed,
        "staged FDC Foundation import completed"
    );
    Ok(())
}

fn parse_fdc_ids(value: &str) -> Result<Vec<u64>, String> {
    let values = value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|_| format!("FDC_IMPORT_INCLUDE_IDS contains invalid FDC ID: {value}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if values.is_empty() {
        return Err("FDC_IMPORT_INCLUDE_IDS must contain at least one FDC ID".to_owned());
    }
    Ok(values)
}

fn required_env(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("{name} is required"))
}

fn optional_env(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.trim().is_empty())
}

fn env_bool(name: &str, default: bool) -> bool {
    match env::var(name).as_deref() {
        Ok("true") => true,
        Ok("false") => false,
        Err(env::VarError::NotPresent) => default,
        Err(env::VarError::NotUnicode(_)) => panic!("{name} must be valid Unicode"),
        Ok(value) => panic!("{name} must be true or false, got {value}"),
    }
}

async fn sqlx_healthcheck(pool: &sqlx::PgPool) {
    sqlx::query("SELECT 1")
        .execute(pool)
        .await
        .expect("database health check failed");
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AppEnvironment {
    Local,
    Ci,
    Staging,
    Production,
}

impl AppEnvironment {
    fn from_env() -> Self {
        let value = env::var("APP_ENV").expect("APP_ENV is required");
        Self::parse(&value).unwrap_or_else(|error| panic!("{error}"))
    }

    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "local" => Ok(Self::Local),
            "ci" => Ok(Self::Ci),
            "staging" => Ok(Self::Staging),
            "production" => Ok(Self::Production),
            _ => Err("APP_ENV must be local, ci, staging, or production".to_owned()),
        }
    }

    const fn allows_development_adapters(self) -> bool {
        matches!(self, Self::Local | Self::Ci)
    }

    const fn allows_source_import(self) -> bool {
        matches!(self, Self::Local | Self::Ci | Self::Staging)
    }
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
    fn from_env(environment: AppEnvironment) -> Self {
        let mode = match env::var("WORKER_MODE").as_deref() {
            Ok("run-once") => WorkerMode::RunOnce,
            Ok("loop") => WorkerMode::Loop,
            Ok("idle") => WorkerMode::Idle,
            Err(env::VarError::NotPresent) if environment.allows_development_adapters() => {
                WorkerMode::Idle
            }
            Err(env::VarError::NotPresent) => {
                panic!("WORKER_MODE is required when APP_ENV is staging or production")
            }
            Err(env::VarError::NotUnicode(_)) => panic!("WORKER_MODE must be valid Unicode"),
            Ok(value) => panic!("unsupported WORKER_MODE: {value}"),
        };
        let worker_id = match env::var("WORKER_ID") {
            Ok(value) if !value.trim().is_empty() => value,
            Ok(_) => panic!("WORKER_ID must not be empty"),
            Err(env::VarError::NotPresent) if environment.allows_development_adapters() => {
                "worker-local".to_owned()
            }
            Err(env::VarError::NotPresent) => {
                panic!("WORKER_ID is required when APP_ENV is staging or production")
            }
            Err(env::VarError::NotUnicode(_)) => panic!("WORKER_ID must be valid Unicode"),
        };
        Self {
            database_url: env::var("DATABASE_URL")
                .expect("DATABASE_URL is required for the worker process"),
            database_pool_size: env_u32("WORKER_DATABASE_POOL_SIZE", 4, 1, 32),
            worker_id,
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

#[cfg(test)]
mod tests {
    use super::{AppEnvironment, parse_fdc_ids};

    #[test]
    fn environment_policy_is_explicit() {
        assert_eq!(AppEnvironment::parse("local"), Ok(AppEnvironment::Local));
        assert_eq!(AppEnvironment::parse("ci"), Ok(AppEnvironment::Ci));
        assert_eq!(
            AppEnvironment::parse("staging"),
            Ok(AppEnvironment::Staging)
        );
        assert_eq!(
            AppEnvironment::parse("production"),
            Ok(AppEnvironment::Production)
        );
        assert!(AppEnvironment::Local.allows_development_adapters());
        assert!(AppEnvironment::Ci.allows_development_adapters());
        assert!(!AppEnvironment::Staging.allows_development_adapters());
        assert!(!AppEnvironment::Production.allows_development_adapters());
        assert!(AppEnvironment::Local.allows_source_import());
        assert!(AppEnvironment::Ci.allows_source_import());
        assert!(AppEnvironment::Staging.allows_source_import());
        assert!(!AppEnvironment::Production.allows_source_import());
        assert!(AppEnvironment::parse("prod").is_err());
    }

    #[test]
    fn fdc_id_selection_requires_valid_ids() {
        assert_eq!(parse_fdc_ids("2, 1").expect("valid IDs"), vec![2, 1]);
        assert!(parse_fdc_ids("").is_err());
        assert!(parse_fdc_ids("1,nope").is_err());
    }
}
