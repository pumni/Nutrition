mod analysis_repository;
mod catalog_activation;
mod catalog_repository;
mod fdc_importer;
mod ops_repository;
mod parser_telemetry;
mod portion_repository;
mod privacy;
mod seed;
mod telemetry;

pub use analysis_repository::PostgresAnalysisRepository;
pub use catalog_activation::{
    CatalogReleaseActivationError, CatalogReleaseActivationReport, CatalogReleaseActivationRequest,
    CatalogReleaseRollbackReport, CatalogReleaseRollbackRequest, activate_catalog_release,
    stage_catalog_rollback,
};
pub use catalog_repository::{PostgresCatalogEvidenceProvider, active_catalog_release_id};
pub use fdc_importer::{
    FDC_ENERGY_MAPPING_POLICY_VERSION, FDC_FOUNDATION_2026_04_ARCHIVE_SHA256,
    FDC_FOUNDATION_2026_04_EXTRACTED_JSON_SHA256, FDC_FOUNDATION_2026_04_NULL_TAIL_POLICY_VERSION,
    FDC_FOUNDATION_2026_04_RELEASE_VERSION, FDC_FOUNDATION_IMPORTER_VERSION,
    FDC_FOUNDATION_V1_SELECTION_CAP, FDC_FOUNDATION_V1_SELECTION_REVIEWER,
    FdcFoundationImportError, FdcFoundationImportReport, FdcFoundationImportRequest,
    FdcFoundationValidationReport, FdcFoundationValidationRequest,
    build_fdc_selection_candidate_manifest, import_fdc_foundation_json,
    validate_fdc_foundation_json,
};
pub use ops_repository::{ClaimedJob, claim_jobs, complete_job, deliver_outbox_batch, fail_job};
pub use parser_telemetry::PostgresParserTelemetrySink;
pub use portion_repository::PostgresPortionEvidenceProvider;
pub use privacy::{
    PrivacyDeletionReceipt, PrivacyError, PrivacyRetentionReport, USER_DATA_EXPORT_VERSION,
    delete_user_data, export_user_data, run_privacy_retention,
};
pub use seed::seed_foundation_fixture;

use sqlx::{PgPool, postgres::PgPoolOptions};
use std::time::{Duration, Instant};
use thiserror::Error;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");

#[derive(Debug, Error)]
pub enum PersistenceError {
    #[error("database connection failed")]
    Connect(#[source] sqlx::Error),
    #[error("database migration failed")]
    Migrate(#[source] sqlx::migrate::MigrateError),
    #[error("database query failed")]
    Query(#[source] sqlx::Error),
}

/// Opens a bounded `PostgreSQL` connection pool.
///
/// # Errors
///
/// Returns [`PersistenceError::Connect`] when a connection cannot be established within the
/// configured acquisition timeout.
pub async fn connect(database_url: &str, max_connections: u32) -> Result<PgPool, PersistenceError> {
    let started = Instant::now();
    let result = PgPoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(Duration::from_secs(5))
        .connect(database_url)
        .await;
    telemetry::record_db_operation(
        "connect",
        started,
        if result.is_ok() { "success" } else { "failure" },
    );
    result
        .inspect(telemetry::record_pool_metrics)
        .map_err(PersistenceError::Connect)
}

/// Applies all embedded forward-only migrations.
///
/// # Errors
///
/// Returns [`PersistenceError::Migrate`] when `PostgreSQL` rejects a migration or the migration
/// history is inconsistent.
pub async fn migrate(pool: &PgPool) -> Result<(), PersistenceError> {
    let started = Instant::now();
    let result = MIGRATOR.run(pool).await;
    telemetry::record_db_operation(
        "migrate",
        started,
        if result.is_ok() { "success" } else { "failure" },
    );
    result.map_err(PersistenceError::Migrate)
}
