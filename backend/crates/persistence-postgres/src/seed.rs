use sqlx::PgPool;

/// Loads the idempotent, test-only foundation dataset and catalog release.
///
/// # Errors
///
/// Returns [`crate::PersistenceError::Query`] when `PostgreSQL` rejects the seed transaction.
pub async fn seed_foundation_fixture(pool: &PgPool) -> Result<(), crate::PersistenceError> {
    sqlx::raw_sql(include_str!("../../../seeds/0001_foundation_fixture.sql"))
        .execute(pool)
        .await
        .map_err(crate::PersistenceError::Query)?;
    Ok(())
}
