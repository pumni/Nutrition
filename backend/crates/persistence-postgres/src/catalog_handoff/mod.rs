mod load;
mod model;
mod staging;
mod validate;

use load::load_package;
use sqlx::PgPool;
use staging::stage_package;
use validate::validate_package;

pub use model::{
    CatalogHandoffImportCapability, CatalogHandoffImportError, CatalogHandoffImportReport,
    CatalogHandoffImportRequest, CatalogHandoffPackageValidationReport,
};

/// Validates a handoff package without opening a database connection or transaction.
///
/// # Errors
///
/// Returns a typed error when the package is missing, unsafe, tampered with, unsupported, or
/// semantically inconsistent.
pub fn validate_catalog_handoff_v1_package(
    package_path: &std::path::Path,
) -> Result<CatalogHandoffPackageValidationReport, CatalogHandoffImportError> {
    let request = CatalogHandoffImportRequest {
        package_path: package_path.to_owned(),
        created_by: String::new(),
        capability: model::CatalogHandoffImportCapability::Production,
    };
    let package = load_package(&request)?;
    validate_package(&package)?;
    Ok(CatalogHandoffPackageValidationReport {
        contract_version: package.manifest.contract_version,
        package_id: package.manifest.package_id,
        package_sha256: package.package_sha256,
        selected_record_count: package.raw_records.len(),
        composition_value_count: package.compositions.len(),
    })
}

/// Validates and stages a canonical catalog-handoff v1 package.
///
/// All filesystem, schema, checksum, semantic, and reference checks complete before the database
/// transaction is opened. The transaction creates only a staged catalog release; activation is
/// intentionally owned by the separate catalog activation API.
///
/// # Errors
///
/// Returns a typed validation, release-conflict, or database error. No transaction is committed
/// when any validation or staging operation fails.
pub async fn import_catalog_handoff_v1(
    pool: &PgPool,
    request: &CatalogHandoffImportRequest,
) -> Result<CatalogHandoffImportReport, CatalogHandoffImportError> {
    let package = load_package(request)?;
    validate_package(&package)?;
    if package.manifest.handoff_profile == model::CATALOG_HANDOFF_TEST_FIXTURE_PROFILE
        && request.capability != model::CatalogHandoffImportCapability::TestFixture
    {
        return Err(CatalogHandoffImportError::Policy(
            "test fixture profile requires an explicit test-fixture import capability".to_owned(),
        ));
    }
    let mut tx = pool.begin().await?;
    let report = stage_package(&mut tx, request, &package).await?;
    tx.commit().await?;
    Ok(report)
}
