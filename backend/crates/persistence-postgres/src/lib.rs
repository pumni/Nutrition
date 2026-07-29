mod analysis_repository;
mod catalog_repository;
mod seed;

pub use analysis_repository::PostgresAnalysisRepository;
pub use catalog_repository::{PostgresCatalogEvidenceProvider, active_catalog_release_id};
pub use seed::seed_foundation_fixture;

use sqlx::{PgPool, postgres::PgPoolOptions};
use std::time::Duration;
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
    PgPoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(Duration::from_secs(5))
        .connect(database_url)
        .await
        .map_err(PersistenceError::Connect)
}

/// Applies all embedded forward-only migrations.
///
/// # Errors
///
/// Returns [`PersistenceError::Migrate`] when `PostgreSQL` rejects a migration or the migration
/// history is inconsistent.
pub async fn migrate(pool: &PgPool) -> Result<(), PersistenceError> {
    MIGRATOR.run(pool).await.map_err(PersistenceError::Migrate)
}
