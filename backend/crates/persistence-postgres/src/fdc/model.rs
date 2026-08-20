#![allow(clippy::wildcard_imports)]

use super::*;

#[derive(Clone, Debug)]
pub struct FdcFoundationImportRequest {
    pub release_version: String,
    pub source_published_date: String,
    pub object_uri: String,
    pub expected_sha256: String,
    pub source_archive_sha256: Option<String>,
    pub preprocessing_policy_version: Option<String>,
    pub include_fdc_ids: Vec<u64>,
    pub created_by: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FdcFoundationImportReport {
    pub dataset_release_id: Uuid,
    pub catalog_release_id: Uuid,
    pub catalog_release_version: String,
    pub raw_record_count: usize,
    pub selected_record_count: usize,
    pub source_sha256: String,
    pub schema_fingerprint: String,
    pub energy_atwater_specific_count: usize,
    pub energy_atwater_general_count: usize,
    pub energy_missing_count: usize,
    pub unexpected_legacy_energy_count: usize,
    pub replayed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FdcFoundationValidationRequest {
    pub release_version: String,
    pub source_published_date: String,
    pub object_uri: String,
    pub source_payload_filename: Option<String>,
    pub source_archive_sha256: Option<String>,
    pub expected_sha256: String,
    pub reviewed_fdc_ids: Vec<u64>,
    pub preprocessing_policy_version: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FdcValidationState {
    Valid,
    Invalid,
}

impl FdcValidationState {
    #[must_use]
    pub const fn is_valid(&self) -> bool {
        matches!(self, Self::Valid)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FdcPreprocessingState {
    NotRequested,
    Applied,
    Rejected,
}

impl FdcPreprocessingState {
    #[must_use]
    pub const fn is_applied(&self) -> bool {
        matches!(self, Self::Applied)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FdcFoundationValidationReport {
    pub source_sha256: String,
    pub expected_sha256: String,
    pub checksum_status: String,
    pub schema_fingerprint: String,
    pub raw_record_count: usize,
    pub valid_record_count: usize,
    pub selected_record_count: usize,
    pub null_record_count: usize,
    pub invalid_record_count: usize,
    pub selection_fingerprint: Option<String>,
    pub selection_status: String,
    pub source_energy_atwater_specific_count: usize,
    pub source_energy_atwater_general_count: usize,
    pub source_energy_missing_count: usize,
    pub source_unexpected_legacy_energy_count: usize,
    pub selected_energy_atwater_specific_count: usize,
    pub selected_energy_atwater_general_count: usize,
    pub selected_energy_missing_count: usize,
    pub selected_unexpected_legacy_energy_count: usize,
    pub source_integrity_valid: FdcValidationState,
    pub source_schema_conformant: FdcValidationState,
    pub preprocessing_applied: FdcPreprocessingState,
    pub preprocessing_policy_version: Option<String>,
    pub normalized_payload_sha256: Option<String>,
    pub normalized_record_count: Option<usize>,
    pub normalized_payload_valid: FdcValidationState,
    pub source_schema_errors: Vec<String>,
    pub artifact_status: String,
    pub validation_status: String,
    pub errors: Vec<String>,
}

#[derive(Debug, Error)]
pub enum FdcFoundationImportError {
    #[error("invalid FDC import input: {0}")]
    InvalidInput(String),
    #[error("FDC import checksum mismatch: expected {expected}, actual {actual}")]
    ChecksumMismatch { expected: String, actual: String },
    #[error("FDC source release conflicts with an existing imported release: {0}")]
    ReleaseConflict(String),
    #[error("FDC JSON parsing failed")]
    Json(#[from] serde_json::Error),
    #[error("FDC import database query failed")]
    Query(#[from] sqlx::Error),
}

#[derive(Clone)]
pub(crate) struct RawFood {
    pub(crate) fdc_id: u64,
    pub(crate) description: String,
    pub(crate) payload: Value,
}

pub(crate) struct StagedNutrient {
    pub(crate) internal_code: &'static str,
    pub(crate) source_nutrient_id: u64,
    pub(crate) source_method: Option<&'static str>,
    pub(crate) amount: Decimal,
    pub(crate) minimum: Option<Decimal>,
    pub(crate) maximum: Option<Decimal>,
    pub(crate) method_code: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct EnergySummary {
    pub(crate) atwater_specific: usize,
    pub(crate) atwater_general: usize,
    pub(crate) missing_energy: usize,
    pub(crate) unexpected_legacy: usize,
}

pub(crate) struct EnergyExtraction {
    pub(crate) selected: Option<StagedNutrient>,
    pub(crate) unexpected_legacy_count: usize,
}

pub(crate) struct PreparedImport {
    pub(crate) created_by: Uuid,
    pub(crate) foods: Vec<RawFood>,
    pub(crate) selected_ids: BTreeSet<u64>,
    pub(crate) source_sha256: String,
    pub(crate) schema_fingerprint: String,
    pub(crate) selection_fingerprint: String,
    pub(crate) catalog_release_version: String,
    pub(crate) energy_summary: EnergySummary,
    pub(crate) preprocessing_policy_version: Option<String>,
    pub(crate) normalized_payload_sha256: Option<String>,
}

pub(crate) struct ValidatedFoods {
    pub(crate) foods: Vec<RawFood>,
    pub(crate) raw_record_count: usize,
    pub(crate) null_record_count: usize,
    pub(crate) invalid_record_count: usize,
}

pub(crate) struct ValidatedSelection {
    pub(crate) energy: EnergySummary,
    pub(crate) errors: Vec<String>,
    pub(crate) selection_fingerprint: Option<String>,
    pub(crate) selected_record_count: usize,
    pub(crate) selection_valid: bool,
    pub(crate) status: String,
}

pub(crate) struct PreprocessingResult {
    pub(crate) applied: bool,
    pub(crate) policy_version: Option<String>,
    pub(crate) normalized_payload_sha256: Option<String>,
    pub(crate) normalized_payload: Option<Vec<u8>>,
    pub(crate) source_integrity_valid: bool,
    pub(crate) errors: Vec<String>,
}

pub(crate) struct EffectiveValidationFoods {
    pub(crate) foods: Vec<RawFood>,
    pub(crate) normalized_payload_valid: bool,
    pub(crate) normalized_record_count: Option<usize>,
    pub(crate) errors: Vec<String>,
}
