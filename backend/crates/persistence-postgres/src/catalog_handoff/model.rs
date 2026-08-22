use serde::Deserialize;
use serde_json::Value;
use std::{collections::BTreeMap, path::PathBuf};
use thiserror::Error;
use uuid::Uuid;

pub const CATALOG_HANDOFF_CONTRACT_VERSION: &str = "catalog-handoff-1.0.0";
pub const CATALOG_HANDOFF_PROFILE: &str = "fdc-foundation-reviewed-selection-v1";
pub const CATALOG_HANDOFF_TEST_FIXTURE_PROFILE: &str = "catalog-handoff-test-fixture-v1";
pub const CATALOG_HANDOFF_PACKAGE_KIND: &str = "nutrition-catalog-handoff";
pub const FDC_HANDOFF_SOURCE_CODE: &str = "usda_fdc_foundation";
pub const FDC_HANDOFF_RELEASE: &str = "2026-04-30";
pub const TEST_FIXTURE_SOURCE_CODE: &str = "synthetic_fixture";
pub const TEST_FIXTURE_RELEASE: &str = "0.1.0";
pub const TEST_FIXTURE_SELECTION_VERSION: &str = "catalog-handoff-test-fixture-0.1.0";
pub const FDC_HANDOFF_SELECTION_SHA256: &str =
    "ad867dbbb6a9387c4cb3e3837fb337353097d7ebd99f774eded25cf56dd9ffc2";
pub const FDC_HANDOFF_SELECTED_IDS: [u64; 20] = [
    1_750_339, 1_750_340, 1_750_341, 1_750_342, 1_750_343, 1_999_626, 1_999_627, 1_999_628,
    1_999_629, 1_999_630, 1_999_631, 1_999_632, 1_999_633, 1_999_634, 2_003_586, 2_003_587,
    2_003_588, 2_003_589, 2_003_590, 2_003_591,
];

#[derive(Clone, Debug)]
pub struct CatalogHandoffImportRequest {
    pub package_path: PathBuf,
    pub created_by: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogHandoffImportReport {
    pub dataset_release_id: Uuid,
    pub catalog_release_id: Uuid,
    pub catalog_release_version: String,
    pub contract_version: String,
    pub package_sha256: String,
    pub selected_record_count: usize,
    pub composition_value_count: usize,
    pub replayed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogHandoffPackageValidationReport {
    pub contract_version: String,
    pub package_id: String,
    pub package_sha256: String,
    pub selected_record_count: usize,
    pub composition_value_count: usize,
}

#[derive(Debug, Error)]
pub enum CatalogHandoffImportError {
    #[error("invalid catalog handoff input: {0}")]
    InvalidInput(String),
    #[error("unsupported catalog handoff contract version: {0}")]
    UnsupportedContractVersion(String),
    #[error("unsupported catalog handoff profile: {0}")]
    UnsupportedProfile(String),
    #[error("unsafe catalog handoff package path: {0}")]
    UnsafePackagePath(String),
    #[error("catalog handoff file is missing: {0}")]
    MissingFile(String),
    #[error("catalog handoff file is unexpected: {0}")]
    UnexpectedFile(String),
    #[error("catalog handoff checksum mismatch for {path}: expected {expected}, actual {actual}")]
    ChecksumMismatch {
        path: String,
        expected: String,
        actual: String,
    },
    #[error("catalog handoff schema validation failed: {0}")]
    Schema(String),
    #[error("catalog handoff semantic validation failed: {0}")]
    Semantic(String),
    #[error("catalog handoff reference integrity failed: {0}")]
    ReferenceIntegrity(String),
    #[error("catalog handoff release conflict: {0}")]
    ReleaseConflict(String),
    #[error("catalog handoff filesystem operation failed: {0}")]
    Io(String),
    #[error("catalog handoff JSON parsing failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("catalog handoff database query failed")]
    Query(#[from] sqlx::Error),
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Manifest {
    pub contract_version: String,
    pub package_kind: String,
    pub package_id: String,
    pub producer: String,
    pub producer_version: String,
    pub handoff_profile: String,
    pub source: ManifestSource,
    pub selection: Selection,
    pub policy_versions: BTreeMap<String, String>,
    pub dataset_release: DatasetRef,
    pub catalog_release_version: String,
    pub files: Vec<FileEntry>,
    pub import_mode: String,
    pub production_eligible: bool,
    pub activation_authorized: bool,
    pub backend_baseline: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ManifestSource {
    pub source_code: String,
    pub release: String,
    pub published_date: String,
    pub object_uri: String,
    pub artifact_sha256: String,
    pub archive_sha256: String,
    pub rights_state: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(clippy::struct_field_names)]
pub(crate) struct Selection {
    pub selection_version: String,
    pub selection_sha256: String,
    pub record_count: usize,
    pub auto_add_records: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DatasetRef {
    pub dataset_code: String,
    pub version: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FileEntry {
    pub path: String,
    pub role: String,
    pub schema: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub record_count: usize,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DatasetRelease {
    pub dataset_code: String,
    pub version: String,
    pub status: String,
    pub artifact_sha256: String,
    pub archive_sha256: String,
    pub object_uri: Option<String>,
    pub schema_fingerprint: String,
    pub source_rights_state: String,
    pub record_count: usize,
    pub production_eligible: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceReleases {
    pub sources: Vec<SourceRelease>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceRelease {
    pub source_code: String,
    pub release: String,
    pub publisher: String,
    pub purpose: String,
    pub locator: String,
    pub rights_state: String,
    pub production_eligible: bool,
    pub archive_artifact: Artifact,
    pub extracted_artifact: Artifact,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Artifact {
    pub sha256: String,
    pub size: u64,
    pub content_type: String,
    pub relative_path: String,
    pub acquisition: Option<Value>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawSourceRecord {
    pub source_code: String,
    pub release: String,
    pub source_id: String,
    pub description: String,
    pub data_type: String,
    pub payload_sha256: String,
    pub payload: Value,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FoodConcept {
    pub concept_id: String,
    pub semantic_key: String,
    pub entity_kind: String,
    pub lifecycle_status: String,
    pub source_code: String,
    pub source_id: String,
    pub source_payload_sha256: String,
    pub review_status: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FoodName {
    pub name_id: String,
    pub concept_id: String,
    pub source_code: String,
    pub source_id: String,
    pub locale: String,
    pub name: String,
    pub normalized_name: String,
    pub name_type: String,
    pub source_payload_sha256: String,
    pub status: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceFoodMapping {
    pub mapping_id: String,
    pub source_code: String,
    pub release: String,
    pub source_id: String,
    pub source_payload_sha256: String,
    pub concept_id: String,
    pub mapping_type: String,
    pub mapping_method: String,
    pub score: f64,
    pub policy_version: String,
    pub review_status: String,
    pub rationale: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CompositionValue {
    pub value_id: String,
    pub source_code: String,
    pub release: String,
    pub source_id: String,
    pub source_payload_sha256: String,
    pub target_code: String,
    pub source_nutrient_id: u64,
    pub source_label: String,
    pub source_unit: String,
    pub source_method: String,
    pub value: Option<f64>,
    pub value_status: String,
    pub conversion: String,
    pub canonical_unit: String,
    pub policy_version: String,
    pub review_status: String,
    pub reviewer_decision_status: String,
}

#[derive(Clone, Debug)]
pub(crate) struct LoadedPackage {
    pub manifest: Manifest,
    pub manifest_value: Value,
    pub manifest_bytes: Vec<u8>,
    pub package_sha256: String,
    pub dataset_release: DatasetRelease,
    pub source_releases: SourceReleases,
    pub raw_records: Vec<RawSourceRecord>,
    pub food_concepts: Vec<FoodConcept>,
    pub food_names: Vec<FoodName>,
    pub mappings: Vec<SourceFoodMapping>,
    pub compositions: Vec<CompositionValue>,
}
