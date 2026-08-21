use crate::{
    config::{self, AppEnvironment, WorkerMode},
    loop_runner,
};
use persistence_postgres::{
    FdcFoundationImportRequest, import_fdc_foundation_json, run_privacy_retention,
};
use sqlx::PgPool;
use std::{env, fs, time::Instant};
use tracing::info;
use tracing_subscriber::EnvFilter;

pub(crate) async fn run() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .json()
        .init();

    let environment = AppEnvironment::from_env();
    let worker_config = config::WorkerConfig::from_env(environment);
    config::initialize_metrics(worker_config.metrics_bind_addr);
    let run_fdc_import = env_bool("RUN_FDC_FOUNDATION_IMPORT", false);
    let run_privacy_cleanup = env_bool("RUN_PRIVACY_RETENTION", false);
    assert!(
        !run_fdc_import || environment.allows_source_import(),
        "RUN_FDC_FOUNDATION_IMPORT=true is forbidden when APP_ENV is production"
    );

    let pool = persistence_postgres::connect(
        &worker_config.database_url,
        worker_config.database_pool_size,
    )
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
        let started = Instant::now();
        let result = run_fdc_foundation_import(&pool).await;
        metrics::counter!(
            "nutrition_catalog_release_operations_total",
            "operation" => "foundation_import",
            "outcome" => if result.is_ok() { "success" } else { "failure" }
        )
        .increment(1);
        metrics::histogram!(
            "nutrition_catalog_release_operation_duration_seconds",
            "operation" => "foundation_import"
        )
        .record(started.elapsed().as_secs_f64());
        result.expect("FDC Foundation import failed");
    }
    if run_privacy_cleanup {
        let started = Instant::now();
        let result = run_privacy_retention(&pool).await;
        metrics::counter!(
            "nutrition_privacy_retention_runs_total",
            "operation" => "retention",
            "outcome" => if result.is_ok() { "success" } else { "failure" }
        )
        .increment(1);
        metrics::histogram!("nutrition_privacy_retention_duration_seconds")
            .record(started.elapsed().as_secs_f64());
        let report = result.expect("privacy retention job failed");
        info!(
            deleted_parser_telemetry = report.deleted_parser_telemetry,
            deleted_audit_events = report.deleted_audit_events,
            purged_analysis_aggregates = report.purged_analysis_aggregates,
            "privacy retention job completed"
        );
    }
    sqlx_healthcheck(&pool).await;
    match worker_config.mode {
        WorkerMode::Idle => info!("worker healthcheck completed"),
        WorkerMode::RunOnce => {
            let processed = loop_runner::process_once(&pool, &worker_config).await;
            info!(processed, "worker run-once completed");
        }
        WorkerMode::Loop => loop_runner::run_loop(&pool, &worker_config).await,
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
    let started = Instant::now();
    let result = sqlx::query("SELECT 1").execute(pool).await;
    metrics::counter!(
        "nutrition_db_readiness_total",
        "outcome" => if result.is_ok() { "success" } else { "failure" }
    )
    .increment(1);
    metrics::histogram!("nutrition_db_readiness_duration_seconds")
        .record(started.elapsed().as_secs_f64());
    metrics::gauge!("nutrition_db_pool_size").set(f64::from(pool.size()));
    let idle_connections = u32::try_from(pool.num_idle()).unwrap_or(u32::MAX);
    metrics::gauge!("nutrition_db_pool_idle").set(f64::from(idle_connections));
    result.expect("database health check failed");
}

#[cfg(test)]
mod tests {
    use super::parse_fdc_ids;
    use crate::config::AppEnvironment;

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
