use crate::{
    config::{self, AppEnvironment, WorkerMode},
    loop_runner,
};
use persistence_postgres::{
    FdcFoundationImportRequest, import_fdc_foundation_json, run_privacy_retention,
};
use sqlx::PgPool;
use std::{env, fs, time::Instant};
use thiserror::Error;
use tracing::info;
#[derive(Debug, Error)]
pub(crate) enum StartupError {
    #[error("startup configuration is invalid: {0}")]
    Config(#[from] config::ConfigError),
    #[error("startup policy rejected")]
    Policy,
    #[error("metrics initialization failed")]
    Metrics,
    #[error("database connection failed")]
    DatabaseConnection,
    #[error("database migration failed")]
    Migration,
    #[error("foundation seed failed")]
    FoundationSeed,
    #[error("FDC Foundation import failed")]
    FdcImport,
    #[error("privacy retention job failed")]
    PrivacyRetention,
    #[error("database health check failed")]
    DatabaseHealthcheck,
}

use tracing_subscriber::EnvFilter;

pub(crate) async fn run() -> Result<(), StartupError> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .json()
        .init();

    let environment = AppEnvironment::from_env()?;
    let worker_config = config::WorkerConfig::from_env(environment)?;
    config::initialize_metrics(worker_config.metrics_bind_addr)
        .map_err(|()| StartupError::Metrics)?;
    let run_migrations = env_bool("RUN_MIGRATIONS", false)?;
    let run_foundation_seed = env_bool("RUN_FOUNDATION_SEED", false)?;
    let run_fdc_import = env_bool("RUN_FDC_FOUNDATION_IMPORT", false)?;
    let run_privacy_cleanup = env_bool("RUN_PRIVACY_RETENTION", false)?;
    if run_fdc_import && !environment.allows_source_import() {
        return Err(StartupError::Policy);
    }

    let pool = persistence_postgres::connect(
        &worker_config.database_url,
        worker_config.database_pool_size,
    )
    .await
    .map_err(|_| StartupError::DatabaseConnection)?;
    if run_migrations {
        persistence_postgres::migrate(&pool)
            .await
            .map_err(|_| StartupError::Migration)?;
        info!("database migrations applied");
    }
    if run_foundation_seed {
        if !environment.allows_development_adapters() {
            return Err(StartupError::Policy);
        }
        persistence_postgres::seed_foundation_fixture(&pool)
            .await
            .map_err(|_| StartupError::FoundationSeed)?;
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
        result?;
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
        let report = result.map_err(|_| StartupError::PrivacyRetention)?;
        info!(
            deleted_parser_telemetry = report.deleted_parser_telemetry,
            deleted_audit_events = report.deleted_audit_events,
            purged_analysis_aggregates = report.purged_analysis_aggregates,
            "privacy retention job completed"
        );
    }
    sqlx_healthcheck(&pool).await?;
    match worker_config.mode {
        WorkerMode::Idle => info!("worker healthcheck completed"),
        WorkerMode::RunOnce => {
            let processed = loop_runner::process_once(&pool, &worker_config).await;
            info!(processed, "worker run-once completed");
        }
        WorkerMode::Loop => loop_runner::run_loop(&pool, &worker_config).await,
    }
    Ok(())
}
async fn run_fdc_foundation_import(pool: &PgPool) -> Result<(), StartupError> {
    let source_path = required_env("FDC_IMPORT_PATH")?;
    let source_bytes = fs::read(&source_path).map_err(|_| StartupError::FdcImport)?;
    let request = FdcFoundationImportRequest {
        release_version: required_env("FDC_IMPORT_RELEASE_VERSION")?,
        source_published_date: required_env("FDC_IMPORT_SOURCE_PUBLISHED_DATE")?,
        object_uri: required_env("FDC_IMPORT_OBJECT_URI")?,
        expected_sha256: required_env("FDC_IMPORT_EXPECTED_SHA256")?,
        source_archive_sha256: optional_env("FDC_IMPORT_SOURCE_ARCHIVE_SHA256")?,
        preprocessing_policy_version: optional_env("FDC_IMPORT_PREPROCESSING_POLICY")?,
        include_fdc_ids: parse_fdc_ids(&required_env("FDC_IMPORT_INCLUDE_IDS")?).map_err(|_| {
            config::ConfigError::InvalidConfiguration {
                name: "FDC_IMPORT_INCLUDE_IDS",
            }
        })?,
        created_by: required_env("FDC_IMPORT_CREATED_BY")?,
    };
    let report = import_fdc_foundation_json(pool, &source_bytes, &request)
        .await
        .map_err(|_| StartupError::FdcImport)?;
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

fn required_env(name: &'static str) -> Result<String, StartupError> {
    match env::var(name) {
        Ok(value) if !value.trim().is_empty() => Ok(value),
        Ok(_) | Err(env::VarError::NotPresent) => {
            Err(config::ConfigError::MissingEnvironment { name }.into())
        }
        Err(env::VarError::NotUnicode(_)) => {
            Err(config::ConfigError::InvalidUnicode { name }.into())
        }
    }
}

fn optional_env(name: &'static str) -> Result<Option<String>, StartupError> {
    match env::var(name) {
        Ok(value) if !value.trim().is_empty() => Ok(Some(value)),
        Ok(_) | Err(env::VarError::NotPresent) => Ok(None),
        Err(env::VarError::NotUnicode(_)) => {
            Err(config::ConfigError::InvalidUnicode { name }.into())
        }
    }
}

fn env_bool(name: &'static str, default: bool) -> Result<bool, config::ConfigError> {
    match env::var(name) {
        Ok(value) => parse_bool(name, Some(&value), default),
        Err(env::VarError::NotPresent) => parse_bool(name, None, default),
        Err(env::VarError::NotUnicode(_)) => Err(config::ConfigError::InvalidUnicode { name }),
    }
}

fn parse_bool(
    name: &'static str,
    value: Option<&str>,
    default: bool,
) -> Result<bool, config::ConfigError> {
    match value {
        Some("true") => Ok(true),
        Some("false") => Ok(false),
        None => Ok(default),
        Some(_) => Err(config::ConfigError::InvalidConfiguration { name }),
    }
}

async fn sqlx_healthcheck(pool: &sqlx::PgPool) -> Result<(), StartupError> {
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
    result
        .map(|_| ())
        .map_err(|_| StartupError::DatabaseHealthcheck)
}

#[cfg(test)]
mod tests {
    use super::{parse_bool, parse_fdc_ids};
    use crate::config::{AppEnvironment, ConfigError, validate_u32};

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
        assert_eq!(
            AppEnvironment::parse("prod"),
            Err(ConfigError::InvalidEnvironment)
        );
    }

    #[test]
    fn numeric_bounds_are_typed_errors() {
        assert_eq!(
            validate_u32("WORKER_BATCH_SIZE", 0, 1, 100),
            Err(ConfigError::InvalidNumericBounds {
                name: "WORKER_BATCH_SIZE",
                minimum: 1,
                maximum: 100,
            })
        );
    }
    #[test]
    fn invalid_boolean_is_a_typed_error() {
        assert_eq!(
            parse_bool("RUN_MIGRATIONS", Some("tru"), false),
            Err(ConfigError::InvalidConfiguration {
                name: "RUN_MIGRATIONS",
            })
        );
    }
    #[test]
    fn fdc_id_selection_requires_valid_ids() {
        assert_eq!(parse_fdc_ids("2, 1").expect("valid IDs"), vec![2, 1]);
        assert!(parse_fdc_ids("").is_err());
        assert!(parse_fdc_ids("1,nope").is_err());
    }
}
