use rust_decimal::Decimal;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use std::{
    collections::{BTreeMap, BTreeSet},
    str::FromStr,
};
use thiserror::Error;
use uuid::Uuid;

pub const FDC_FOUNDATION_IMPORTER_VERSION: &str = "fdc-foundation-json-0.2.0";
pub const FDC_ENERGY_MAPPING_POLICY_VERSION: &str = "fdc_energy_v1";
pub const FDC_FOUNDATION_2026_04_NULL_TAIL_POLICY_VERSION: &str =
    "fdc_foundation_2026_04_null_tail_v1";
pub const FDC_FOUNDATION_2026_04_RELEASE_VERSION: &str = "2026-04-30";
pub const FDC_FOUNDATION_2026_04_ARCHIVE_SHA256: &str =
    "186e988ec542e913f51ef62b86a47758e8cdd0d1dc3889e7b055581f3c09c77a";
pub const FDC_FOUNDATION_2026_04_EXTRACTED_JSON_SHA256: &str =
    "27d1fe3fd89edfbe528ed915da5619320e1d004d4594603a1b19bdb1511590cc";
pub const FDC_FOUNDATION_V1_SELECTION_REVIEWER: &str = "pumni";
pub const FDC_FOUNDATION_V1_SELECTION_CAP: usize = 20;
pub(crate) const FDC_FOUNDATION_2026_04_SOURCE_RECORD_COUNT: usize = 395;
pub(crate) const FDC_FOUNDATION_2026_04_VALID_RECORD_COUNT: usize = 363;
pub(crate) const FDC_DATASET_CODE: &str = "usda_fdc";
pub(crate) const FDC_SOURCE_DOWNLOAD_URL: &str =
    "https://fdc.nal.usda.gov/fdc-datasets/FoodData_Central_foundation_food_json_2026-04-30.zip";
pub(crate) const FDC_SCHEMA_CONTRACT: &str = "FoundationFoods[].{fdcId:uint,dataType:Foundation,description:string,foodNutrients:[{amount:number,nutrient:{id:uint,unitName:string},foodNutrientDerivation?:{code?:string}}]}";

mod artifact;
mod energy;
mod model;
mod normalize;
mod nutrients;
mod parse;
mod provenance;
mod report;
mod selection;
mod staging;

pub(crate) use energy::*;
pub(crate) use model::{
    EffectiveValidationFoods, EnergyExtraction, EnergySummary, PreparedImport, PreprocessingResult,
    RawFood, StagedNutrient, ValidatedFoods, ValidatedSelection,
};
pub use model::{
    FdcFoundationImportError, FdcFoundationImportReport, FdcFoundationImportRequest,
    FdcFoundationValidationReport, FdcFoundationValidationRequest, FdcPreprocessingState,
    FdcValidationState,
};
pub(crate) use normalize::*;
pub(crate) use nutrients::*;
pub(crate) use parse::*;
pub(crate) use provenance::*;
pub(crate) use report::*;
pub(crate) use selection::*;
pub(crate) use staging::*;

/// Imports a validated, release-pinned FDC Foundation payload into `PostgreSQL`.
///
/// # Errors
///
/// Returns an import error when validation, provenance, or transactional persistence fails.
pub async fn import_fdc_foundation_json(
    pool: &PgPool,
    source_bytes: &[u8],
    request: &FdcFoundationImportRequest,
) -> Result<FdcFoundationImportReport, FdcFoundationImportError> {
    artifact::import_fdc_foundation_json_impl(pool, source_bytes, request).await
}

#[must_use]
pub fn validate_fdc_foundation_json(
    source_bytes: &[u8],
    request: &FdcFoundationValidationRequest,
) -> FdcFoundationValidationReport {
    artifact::validate_fdc_foundation_json_impl(source_bytes, request)
}

/// Builds the deterministic reviewed-candidate manifest for an FDC Foundation payload.
///
/// # Errors
///
/// Returns an import error when the payload or its requested selection cannot be validated.
pub fn build_fdc_selection_candidate_manifest(
    source_bytes: &[u8],
    request: &FdcFoundationValidationRequest,
    candidate_cap: usize,
) -> Result<Value, FdcFoundationImportError> {
    artifact::build_fdc_selection_candidate_manifest_impl(source_bytes, request, candidate_cap)
}

#[cfg(test)]
mod tests;
