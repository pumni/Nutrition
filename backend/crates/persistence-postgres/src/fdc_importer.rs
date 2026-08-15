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
const FDC_FOUNDATION_2026_04_SOURCE_RECORD_COUNT: usize = 395;
const FDC_FOUNDATION_2026_04_VALID_RECORD_COUNT: usize = 363;
const FDC_DATASET_CODE: &str = "usda_fdc";
const FDC_SOURCE_DOWNLOAD_URL: &str =
    "https://fdc.nal.usda.gov/fdc-datasets/FoodData_Central_foundation_food_json_2026-04-30.zip";
const FDC_SCHEMA_CONTRACT: &str = "FoundationFoods[].{fdcId:uint,dataType:Foundation,description:string,foodNutrients:[{amount:number,nutrient:{id:uint,unitName:string},foodNutrientDerivation?:{code?:string}}]}";

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
struct RawFood {
    fdc_id: u64,
    description: String,
    payload: Value,
}

struct StagedNutrient {
    internal_code: &'static str,
    source_nutrient_id: u64,
    source_method: Option<&'static str>,
    amount: Decimal,
    minimum: Option<Decimal>,
    maximum: Option<Decimal>,
    method_code: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct EnergySummary {
    atwater_specific: usize,
    atwater_general: usize,
    missing_energy: usize,
    unexpected_legacy: usize,
}

struct EnergyExtraction {
    selected: Option<StagedNutrient>,
    unexpected_legacy_count: usize,
}

struct PreparedImport {
    created_by: Uuid,
    foods: Vec<RawFood>,
    selected_ids: BTreeSet<u64>,
    source_sha256: String,
    schema_fingerprint: String,
    selection_fingerprint: String,
    catalog_release_version: String,
    energy_summary: EnergySummary,
    preprocessing_policy_version: Option<String>,
    normalized_payload_sha256: Option<String>,
}

/// Imports a pinned USDA FDC Foundation Foods JSON artifact into raw provenance storage and
/// creates a non-published staged catalog selection.
///
/// This function deliberately does not activate a dataset or catalog release and does not publish
/// composition profiles. Energy is also deliberately omitted until the protected policy decision
/// tracked by repository issue #22 is resolved.
///
/// # Errors
///
/// Returns an error before commit when the checksum, source structure, reviewed selection,
/// unambiguous macronutrient mapping, or database invariants are invalid.
pub async fn import_fdc_foundation_json(
    pool: &PgPool,
    source_bytes: &[u8],
    request: &FdcFoundationImportRequest,
) -> Result<FdcFoundationImportReport, FdcFoundationImportError> {
    let prepared = prepare_import(source_bytes, request)?;
    let mut tx = pool.begin().await?;
    let report = stage_import(&mut tx, request, &prepared).await?;
    tx.commit().await?;
    Ok(report)
}

/// Validates a pinned FDC artifact without writing to `PostgreSQL` or activating any release.
///
/// The report is deliberately an evidence artifact rather than an activation decision. It keeps
/// source-wide coverage separate from the reviewed selection, records structural anomalies, and
/// always reports `production_eligible: false` when rendered.
#[must_use]
pub fn validate_fdc_foundation_json(
    source_bytes: &[u8],
    request: &FdcFoundationValidationRequest,
) -> FdcFoundationValidationReport {
    let source_sha256 = sha256_hex(source_bytes);
    let expected_sha256 = request.expected_sha256.trim().to_ascii_lowercase();
    let (checksum_valid, artifact_errors) =
        validate_checksum(&source_sha256, &expected_sha256, request);
    let mut source_schema_errors = Vec::new();
    let source_foods = validate_source_records(source_bytes, &mut source_schema_errors);
    source_schema_errors.sort();
    let source_schema_conformant = source_schema_errors.is_empty();
    let preprocessing = apply_requested_preprocessing(source_bytes, &source_sha256, request);
    let effective = effective_validation_foods(
        &source_foods,
        &preprocessing,
        request.preprocessing_policy_version.is_some(),
        source_schema_conformant,
    );
    let foods = effective.foods;
    let normalized_payload_valid = effective.normalized_payload_valid;
    let normalized_record_count = effective.normalized_record_count;
    let mut energy_errors = Vec::new();
    let source_energy = source_energy_summary(&foods, &mut energy_errors);
    let selection = validate_reviewed_selection(&foods, request);
    let source_integrity_valid =
        checksum_valid && artifact_errors.is_empty() && preprocessing.source_integrity_valid;
    let mut errors = artifact_errors;
    if !preprocessing.applied {
        errors.extend(source_schema_errors.iter().cloned());
    }
    errors.extend(preprocessing.errors.iter().cloned());
    errors.extend(effective.errors);
    errors.extend(energy_errors);
    errors.extend(selection.errors.iter().cloned());
    let artifact_valid = source_integrity_valid && normalized_payload_valid;
    let validation_passed = artifact_valid
        && selection.selection_valid
        && selection.errors.is_empty()
        && errors.is_empty();
    errors.sort();
    FdcFoundationValidationReport {
        source_sha256,
        expected_sha256,
        schema_fingerprint: schema_fingerprint(),
        raw_record_count: source_foods.raw_record_count,
        valid_record_count: foods.len(),
        selected_record_count: selection.selected_record_count,
        null_record_count: source_foods.null_record_count,
        invalid_record_count: source_foods.invalid_record_count,
        selection_fingerprint: selection.selection_fingerprint,
        selection_status: selection.status,
        source_energy_atwater_specific_count: source_energy.atwater_specific,
        source_energy_atwater_general_count: source_energy.atwater_general,
        source_energy_missing_count: source_energy.missing_energy,
        source_unexpected_legacy_energy_count: source_energy.unexpected_legacy,
        selected_energy_atwater_specific_count: selection.energy.atwater_specific,
        selected_energy_atwater_general_count: selection.energy.atwater_general,
        selected_energy_missing_count: selection.energy.missing_energy,
        selected_unexpected_legacy_energy_count: selection.energy.unexpected_legacy,
        source_integrity_valid: if source_integrity_valid {
            FdcValidationState::Valid
        } else {
            FdcValidationState::Invalid
        },
        source_schema_conformant: if source_schema_conformant {
            FdcValidationState::Valid
        } else {
            FdcValidationState::Invalid
        },
        preprocessing_applied: if request.preprocessing_policy_version.is_none() {
            FdcPreprocessingState::NotRequested
        } else if preprocessing.applied {
            FdcPreprocessingState::Applied
        } else {
            FdcPreprocessingState::Rejected
        },
        preprocessing_policy_version: preprocessing.policy_version,
        normalized_payload_sha256: preprocessing.normalized_payload_sha256,
        normalized_record_count,
        normalized_payload_valid: if normalized_payload_valid {
            FdcValidationState::Valid
        } else {
            FdcValidationState::Invalid
        },
        source_schema_errors,
        checksum_status: if checksum_valid { "valid" } else { "invalid" }.to_owned(),
        artifact_status: if artifact_valid { "valid" } else { "invalid" }.to_owned(),
        validation_status: if validation_passed {
            "passed"
        } else {
            "blocked"
        }
        .to_owned(),
        errors,
    }
}

struct ValidatedFoods {
    foods: Vec<RawFood>,
    raw_record_count: usize,
    null_record_count: usize,
    invalid_record_count: usize,
}

struct ValidatedSelection {
    energy: EnergySummary,
    errors: Vec<String>,
    selection_fingerprint: Option<String>,
    selected_record_count: usize,
    selection_valid: bool,
    status: String,
}

struct PreprocessingResult {
    applied: bool,
    policy_version: Option<String>,
    normalized_payload_sha256: Option<String>,
    normalized_payload: Option<Vec<u8>>,
    source_integrity_valid: bool,
    errors: Vec<String>,
}

struct EffectiveValidationFoods {
    foods: Vec<RawFood>,
    normalized_payload_valid: bool,
    normalized_record_count: Option<usize>,
    errors: Vec<String>,
}

fn effective_validation_foods(
    source_foods: &ValidatedFoods,
    preprocessing: &PreprocessingResult,
    preprocessing_requested: bool,
    source_schema_conformant: bool,
) -> EffectiveValidationFoods {
    let Some(normalized_bytes) = preprocessing.normalized_payload.as_deref() else {
        return EffectiveValidationFoods {
            foods: source_foods.foods.clone(),
            normalized_payload_valid: !preprocessing_requested && source_schema_conformant,
            normalized_record_count: None,
            errors: Vec::new(),
        };
    };
    let mut errors = Vec::new();
    let normalized_foods = validate_source_records(normalized_bytes, &mut errors);
    let valid = errors.is_empty()
        && normalized_foods.raw_record_count == FDC_FOUNDATION_2026_04_VALID_RECORD_COUNT
        && normalized_foods.null_record_count == 0
        && normalized_foods.invalid_record_count == 0;
    EffectiveValidationFoods {
        foods: normalized_foods.foods,
        normalized_payload_valid: valid,
        normalized_record_count: Some(normalized_foods.raw_record_count),
        errors,
    }
}

fn apply_requested_preprocessing(
    source_bytes: &[u8],
    source_sha256: &str,
    request: &FdcFoundationValidationRequest,
) -> PreprocessingResult {
    let Some(policy_version) = request.preprocessing_policy_version.as_deref() else {
        return PreprocessingResult {
            applied: false,
            policy_version: None,
            normalized_payload_sha256: None,
            normalized_payload: None,
            source_integrity_valid: true,
            errors: Vec::new(),
        };
    };

    let errors = preprocessing_contract_errors(source_sha256, policy_version, request);
    if !errors.is_empty() {
        return PreprocessingResult {
            applied: false,
            policy_version: Some(policy_version.to_owned()),
            normalized_payload_sha256: None,
            normalized_payload: None,
            source_integrity_valid: false,
            errors,
        };
    }

    let normalized_payload = match build_normalized_fdc_payload(source_bytes) {
        Ok(payload) => payload,
        Err(error) => {
            return PreprocessingResult {
                applied: false,
                policy_version: Some(policy_version.to_owned()),
                normalized_payload_sha256: None,
                normalized_payload: None,
                source_integrity_valid: true,
                errors: vec![error],
            };
        }
    };
    let normalized_payload_sha256 = sha256_hex(&normalized_payload);
    let mut normalized_errors = Vec::new();
    let normalized_foods = validate_source_records(&normalized_payload, &mut normalized_errors);
    if !normalized_errors.is_empty()
        || normalized_foods.raw_record_count != FDC_FOUNDATION_2026_04_VALID_RECORD_COUNT
        || normalized_foods.null_record_count != 0
        || normalized_foods.invalid_record_count != 0
    {
        normalized_errors.sort();
        return PreprocessingResult {
            applied: false,
            policy_version: Some(policy_version.to_owned()),
            normalized_payload_sha256: Some(normalized_payload_sha256),
            normalized_payload: None,
            source_integrity_valid: true,
            errors: normalized_errors,
        };
    }
    PreprocessingResult {
        applied: true,
        policy_version: Some(policy_version.to_owned()),
        normalized_payload_sha256: Some(normalized_payload_sha256),
        normalized_payload: Some(normalized_payload),
        source_integrity_valid: true,
        errors: Vec::new(),
    }
}

fn preprocessing_contract_errors(
    source_sha256: &str,
    policy_version: &str,
    request: &FdcFoundationValidationRequest,
) -> Vec<String> {
    let mut errors = Vec::new();
    if policy_version != FDC_FOUNDATION_2026_04_NULL_TAIL_POLICY_VERSION {
        errors.push(format!(
            "unsupported FDC preprocessing policy version: {policy_version}"
        ));
    }
    if request.release_version != FDC_FOUNDATION_2026_04_RELEASE_VERSION {
        errors.push(format!(
            "{FDC_FOUNDATION_2026_04_NULL_TAIL_POLICY_VERSION} requires release_version {FDC_FOUNDATION_2026_04_RELEASE_VERSION}"
        ));
    }
    if source_sha256 != FDC_FOUNDATION_2026_04_EXTRACTED_JSON_SHA256 {
        errors.push(format!(
            "{FDC_FOUNDATION_2026_04_NULL_TAIL_POLICY_VERSION} requires extracted JSON SHA-256 {FDC_FOUNDATION_2026_04_EXTRACTED_JSON_SHA256}, got {source_sha256}"
        ));
    }
    if normalize_sha256(&request.expected_sha256).ok().as_deref()
        != Some(FDC_FOUNDATION_2026_04_EXTRACTED_JSON_SHA256)
    {
        errors.push(format!(
            "{FDC_FOUNDATION_2026_04_NULL_TAIL_POLICY_VERSION} requires expected_sha256 {FDC_FOUNDATION_2026_04_EXTRACTED_JSON_SHA256}"
        ));
    }
    match request.source_archive_sha256.as_deref() {
        Some(archive_sha256)
            if normalize_sha256(archive_sha256).ok().as_deref()
                == Some(FDC_FOUNDATION_2026_04_ARCHIVE_SHA256) => {}
        Some(archive_sha256) => errors.push(format!(
            "{FDC_FOUNDATION_2026_04_NULL_TAIL_POLICY_VERSION} requires archive SHA-256 {FDC_FOUNDATION_2026_04_ARCHIVE_SHA256}, got {archive_sha256}"
        )),
        None => errors.push(format!(
            "{FDC_FOUNDATION_2026_04_NULL_TAIL_POLICY_VERSION} requires source_archive_sha256"
        )),
    }
    errors
}

fn build_normalized_fdc_payload(source_bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut root = serde_json::from_slice::<Value>(source_bytes)
        .map_err(|error| format!("preprocessing source JSON parsing failed: {error}"))?;
    transform_fdc_foundation_2026_04_null_tail(&mut root)
}

fn transform_fdc_foundation_2026_04_null_tail(root: &mut Value) -> Result<Vec<u8>, String> {
    let Some(food_values) = root
        .get_mut("FoundationFoods")
        .and_then(Value::as_array_mut)
    else {
        return Err(
            "preprocessing source JSON root must contain a FoundationFoods array".to_owned(),
        );
    };
    if food_values.len() != FDC_FOUNDATION_2026_04_SOURCE_RECORD_COUNT {
        return Err(format!(
            "{FDC_FOUNDATION_2026_04_NULL_TAIL_POLICY_VERSION} requires exactly {FDC_FOUNDATION_2026_04_SOURCE_RECORD_COUNT} FoundationFoods entries, got {}",
            food_values.len()
        ));
    }
    if let Some(index) = food_values
        .iter()
        .take(FDC_FOUNDATION_2026_04_VALID_RECORD_COUNT)
        .position(Value::is_null)
    {
        return Err(format!(
            "{FDC_FOUNDATION_2026_04_NULL_TAIL_POLICY_VERSION} rejects an interior null at FoundationFoods[{index}]"
        ));
    }
    if let Some(index) = food_values
        .iter()
        .take(FDC_FOUNDATION_2026_04_VALID_RECORD_COUNT)
        .position(|value| !value.is_object())
    {
        return Err(format!(
            "{FDC_FOUNDATION_2026_04_NULL_TAIL_POLICY_VERSION} requires object entries at FoundationFoods[0..362]; invalid entry at index {index}"
        ));
    }
    let trailing_null_count = food_values
        .iter()
        .skip(FDC_FOUNDATION_2026_04_VALID_RECORD_COUNT)
        .filter(|value| value.is_null())
        .count();
    if trailing_null_count
        != FDC_FOUNDATION_2026_04_SOURCE_RECORD_COUNT - FDC_FOUNDATION_2026_04_VALID_RECORD_COUNT
        || food_values
            .iter()
            .skip(FDC_FOUNDATION_2026_04_VALID_RECORD_COUNT)
            .any(|value| !value.is_null())
    {
        return Err(format!(
            "{FDC_FOUNDATION_2026_04_NULL_TAIL_POLICY_VERSION} requires exactly 32 null entries at FoundationFoods[363..394]"
        ));
    }
    food_values.truncate(FDC_FOUNDATION_2026_04_VALID_RECORD_COUNT);
    serde_json::to_vec(root)
        .map_err(|error| format!("normalized FDC payload serialization failed: {error}"))
}

fn validate_checksum(
    source_sha256: &str,
    expected_sha256: &str,
    request: &FdcFoundationValidationRequest,
) -> (bool, Vec<String>) {
    let mut errors = Vec::new();
    let checksum_valid = normalize_sha256(expected_sha256).map_or_else(
        |_| {
            errors.push("expected_sha256 must be exactly 64 hexadecimal characters".to_owned());
            false
        },
        |expected| expected == source_sha256,
    );
    if !checksum_valid && errors.is_empty() {
        errors.push(format!(
            "artifact checksum mismatch: expected {expected_sha256}, actual {source_sha256}"
        ));
    }
    for (field, value) in [
        ("release_version", request.release_version.as_str()),
        (
            "source_published_date",
            request.source_published_date.as_str(),
        ),
        ("object_uri", request.object_uri.as_str()),
    ] {
        if value.trim().is_empty() {
            errors.push(format!("{field} must not be empty"));
        }
    }
    if let Some(archive_sha256) = request.source_archive_sha256.as_deref()
        && normalize_sha256(archive_sha256).is_err()
    {
        errors.push(
            "source_archive_sha256 must be exactly 64 hexadecimal characters when present"
                .to_owned(),
        );
    }
    (checksum_valid, errors)
}

fn validate_source_records(source_bytes: &[u8], errors: &mut Vec<String>) -> ValidatedFoods {
    let mut result = ValidatedFoods {
        foods: Vec::new(),
        raw_record_count: 0,
        null_record_count: 0,
        invalid_record_count: 0,
    };
    let root = match serde_json::from_slice::<Value>(source_bytes) {
        Ok(root) => root,
        Err(error) => {
            errors.push(format!("FDC JSON parsing failed: {error}"));
            return result;
        }
    };
    let Some(food_values) = root.get("FoundationFoods").and_then(Value::as_array) else {
        errors.push("FDC JSON root must contain a FoundationFoods array".to_owned());
        return result;
    };
    result.raw_record_count = food_values.len();
    let mut seen_ids = BTreeSet::new();
    for (index, payload) in food_values.iter().enumerate() {
        if payload.is_null() {
            result.null_record_count += 1;
            result.invalid_record_count += 1;
            errors.push(format!(
                "FoundationFoods[{index}] is null; expected a Foundation food object"
            ));
            continue;
        }
        let food = match parse_food_record(payload.clone()) {
            Ok(food) => food,
            Err(error) => {
                result.invalid_record_count += 1;
                errors.push(format!(
                    "FoundationFoods[{index}] failed structural validation: {error}"
                ));
                continue;
            }
        };
        if !seen_ids.insert(food.fdc_id) {
            result.invalid_record_count += 1;
            errors.push(format!(
                "FoundationFoods[{index}] duplicates FDC ID {}",
                food.fdc_id
            ));
            continue;
        }
        let shape_errors = validate_food_schema(&food);
        if !shape_errors.is_empty() {
            result.invalid_record_count += 1;
            errors.extend(shape_errors.into_iter().map(|error| {
                format!("FoundationFoods[{index}] schema validation failed: {error}")
            }));
            continue;
        }
        result.foods.push(food);
    }
    result
}

fn source_energy_summary(foods: &[RawFood], errors: &mut Vec<String>) -> EnergySummary {
    let mut summary = EnergySummary::default();
    for food in foods {
        match extract_energy(food) {
            Ok(energy) => add_energy_summary(&mut summary, &energy),
            Err(error) => errors.push(format!(
                "FDC ID {} energy validation failed: {error}",
                food.fdc_id
            )),
        }
    }
    summary
}

fn validate_reviewed_selection(
    foods: &[RawFood],
    request: &FdcFoundationValidationRequest,
) -> ValidatedSelection {
    let selected_ids = request
        .reviewed_fdc_ids
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let mut errors = Vec::new();
    if request.reviewed_fdc_ids.is_empty() {
        errors.push("reviewed FDC selection is not approved".to_owned());
    }
    if selected_ids.len() != request.reviewed_fdc_ids.len() {
        errors.push("reviewed FDC selection contains duplicate IDs".to_owned());
    }
    let available_ids = foods
        .iter()
        .map(|food| food.fdc_id)
        .collect::<BTreeSet<_>>();
    let missing_ids = selected_ids
        .difference(&available_ids)
        .copied()
        .collect::<Vec<_>>();
    if !missing_ids.is_empty() {
        errors.push(format!(
            "reviewed FDC IDs are missing from the structurally valid artifact: {missing_ids:?}"
        ));
    }
    let selection_valid = !request.reviewed_fdc_ids.is_empty()
        && selected_ids.len() == request.reviewed_fdc_ids.len()
        && missing_ids.is_empty();
    let status = if request.reviewed_fdc_ids.is_empty() {
        "not_approved"
    } else if selection_valid {
        "validated"
    } else {
        "invalid"
    }
    .to_owned();
    let selection_fingerprint = if selected_ids.is_empty() {
        None
    } else {
        Some(selection_fingerprint(&selected_ids))
    };
    let mut energy = EnergySummary::default();
    let mut selected_record_count = 0;
    for food in foods
        .iter()
        .filter(|food| selected_ids.contains(&food.fdc_id))
    {
        selected_record_count += 1;
        if let Err(error) = extract_unambiguous_macronutrients(food) {
            errors.push(format!(
                "FDC ID {} selected composition validation failed: {error}",
                food.fdc_id
            ));
        }
        match extract_energy(food) {
            Ok(extracted) => add_energy_summary(&mut energy, &extracted),
            Err(error) => errors.push(format!(
                "FDC ID {} selected energy validation failed: {error}",
                food.fdc_id
            )),
        }
    }
    errors.sort();
    ValidatedSelection {
        energy,
        errors,
        selection_fingerprint,
        selected_record_count,
        selection_valid,
        status,
    }
}

impl FdcFoundationValidationReport {
    #[must_use]
    pub fn to_json(&self, request: &FdcFoundationValidationRequest) -> Value {
        json!({
            "source": FDC_DATASET_CODE,
            "source_release": request.release_version,
            "source_published_date": request.source_published_date,
            "object_uri": request.object_uri,
            "source_payload_filename": request.source_payload_filename,
            "source_archive_sha256": request.source_archive_sha256,
            "artifact_sha256": self.source_sha256,
            "expected_artifact_sha256": self.expected_sha256,
            "checksum_valid": self.checksum_status == "valid",
            "source_integrity_valid": self.source_integrity_valid.is_valid(),
            "source_schema_conformant": self.source_schema_conformant.is_valid(),
            "preprocessing_applied": self.preprocessing_applied.is_applied(),
            "preprocessing_policy_version": self.preprocessing_policy_version,
            "normalized_payload_sha256": self.normalized_payload_sha256,
            "normalized_record_count": self.normalized_record_count,
            "normalized_payload_valid": self.normalized_payload_valid.is_valid(),
            "schema_fingerprint": self.schema_fingerprint,
            "schema_contract": FDC_SCHEMA_CONTRACT,
            "importer_version": FDC_FOUNDATION_IMPORTER_VERSION,
            "energy_policy": FDC_ENERGY_MAPPING_POLICY_VERSION,
            "raw_records": self.raw_record_count,
            "valid_records": self.valid_record_count,
            "selected_records": self.selected_record_count,
            "null_records": self.null_record_count,
            "invalid_records": self.invalid_record_count,
            "source_schema_errors": self.source_schema_errors,
            "selection_fingerprint": self.selection_fingerprint,
            "selection_status": self.selection_status,
            "energy": {
                "scope": "valid_source_records",
                "atwater_specific_2048": self.source_energy_atwater_specific_count,
                "atwater_general_2047": self.source_energy_atwater_general_count,
                "missing": self.source_energy_missing_count,
                "unexpected_legacy_1008": self.source_unexpected_legacy_energy_count
            },
            "selected_energy": {
                "scope": "reviewed_selection",
                "atwater_specific_2048": self.selected_energy_atwater_specific_count,
                "atwater_general_2047": self.selected_energy_atwater_general_count,
                "missing": self.selected_energy_missing_count,
                "unexpected_legacy_1008": self.selected_unexpected_legacy_energy_count
            },
            "errors": self.errors,
            "artifact_valid": self.artifact_status == "valid",
            "selection_valid": self.selection_status == "validated",
            "validation_passed": self.validation_status == "passed",
            "production_eligible": false,
            "release": {
                "status": "staged_only",
                "reviewer_approved": false,
                "activation_attempted": false
            }
        })
    }

    /// Renders the report as deterministic pretty-printed JSON.
    ///
    /// # Errors
    ///
    /// Returns a serialization error if the report cannot be represented as JSON.
    pub fn to_pretty_json(
        &self,
        request: &FdcFoundationValidationRequest,
    ) -> Result<String, serde_json::Error> {
        // `Value` contains only JSON-native values, so this serialization is deterministic.
        serde_json::to_string_pretty(&self.to_json(request))
    }
}

fn prepare_import(
    source_bytes: &[u8],
    request: &FdcFoundationImportRequest,
) -> Result<PreparedImport, FdcFoundationImportError> {
    validate_request_metadata(request)?;
    let expected_sha256 = normalize_sha256(&request.expected_sha256)?;
    let source_sha256 = sha256_hex(source_bytes);
    if expected_sha256 != source_sha256 {
        return Err(FdcFoundationImportError::ChecksumMismatch {
            expected: expected_sha256,
            actual: source_sha256,
        });
    }

    let (import_bytes, normalized_payload_sha256) =
        prepare_import_payload(source_bytes, &source_sha256, request)?;

    let created_by = request.created_by.parse::<Uuid>().map_err(|_| {
        FdcFoundationImportError::InvalidInput("created_by must be a UUID".to_owned())
    })?;
    let selected_ids = reviewed_selection(&request.include_fdc_ids)?;
    let foods = parse_source_foods(&import_bytes)?;
    validate_selection_exists(&foods, &selected_ids)?;
    let energy_summary = validate_selected_import_foods(&foods, &selected_ids)?;

    let schema_fingerprint = schema_fingerprint();
    let selection_fingerprint = selection_fingerprint(&selected_ids);
    let release_fingerprint = sha256_hex(
        format!(
            "{}:{selection_fingerprint}:{FDC_FOUNDATION_IMPORTER_VERSION}:{}",
            request.release_version,
            request
                .preprocessing_policy_version
                .as_deref()
                .unwrap_or("none")
        )
        .as_bytes(),
    );
    let catalog_release_version = format!(
        "usda-fdc-foundation-{}-{}",
        request.release_version,
        &release_fingerprint[..12]
    );

    Ok(PreparedImport {
        created_by,
        foods,
        selected_ids,
        source_sha256,
        schema_fingerprint,
        selection_fingerprint,
        catalog_release_version,
        energy_summary,
        preprocessing_policy_version: request.preprocessing_policy_version.clone(),
        normalized_payload_sha256,
    })
}

fn prepare_import_payload(
    source_bytes: &[u8],
    source_sha256: &str,
    request: &FdcFoundationImportRequest,
) -> Result<(Vec<u8>, Option<String>), FdcFoundationImportError> {
    if request.preprocessing_policy_version.is_none() {
        return Ok((source_bytes.to_vec(), None));
    }
    let validation_request = FdcFoundationValidationRequest {
        release_version: request.release_version.clone(),
        source_published_date: request.source_published_date.clone(),
        object_uri: request.object_uri.clone(),
        source_payload_filename: None,
        source_archive_sha256: request.source_archive_sha256.clone(),
        expected_sha256: request.expected_sha256.clone(),
        reviewed_fdc_ids: request.include_fdc_ids.clone(),
        preprocessing_policy_version: request.preprocessing_policy_version.clone(),
    };
    let preprocessing =
        apply_requested_preprocessing(source_bytes, source_sha256, &validation_request);
    let normalized_payload_sha256 = preprocessing.normalized_payload_sha256.clone();
    let normalized_bytes = preprocessing.normalized_payload.ok_or_else(|| {
        FdcFoundationImportError::InvalidInput(format!(
            "FDC preprocessing was not applied: {}",
            preprocessing.errors.join("; ")
        ))
    })?;
    if !preprocessing.source_integrity_valid || !preprocessing.applied {
        return Err(FdcFoundationImportError::InvalidInput(
            "FDC normalized payload failed its source/policy verification".to_owned(),
        ));
    }
    Ok((normalized_bytes, normalized_payload_sha256))
}

fn validate_selected_import_foods(
    foods: &[RawFood],
    selected_ids: &BTreeSet<u64>,
) -> Result<EnergySummary, FdcFoundationImportError> {
    let mut energy_summary = EnergySummary::default();
    for food in foods
        .iter()
        .filter(|food| selected_ids.contains(&food.fdc_id))
    {
        extract_unambiguous_macronutrients(food)?;
        let energy = extract_energy(food)?;
        match energy
            .selected
            .as_ref()
            .map(|value| value.source_nutrient_id)
        {
            Some(2048) => energy_summary.atwater_specific += 1,
            Some(2047) => energy_summary.atwater_general += 1,
            None => energy_summary.missing_energy += 1,
            Some(source_nutrient_id) => {
                return Err(FdcFoundationImportError::InvalidInput(format!(
                    "FDC ID {} selected unsupported energy nutrient {}",
                    food.fdc_id, source_nutrient_id
                )));
            }
        }
        energy_summary.unexpected_legacy += energy.unexpected_legacy_count;
    }
    Ok(energy_summary)
}

fn validate_request_metadata(
    request: &FdcFoundationImportRequest,
) -> Result<(), FdcFoundationImportError> {
    for (field, value) in [
        ("release_version", request.release_version.as_str()),
        (
            "source_published_date",
            request.source_published_date.as_str(),
        ),
        ("object_uri", request.object_uri.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(FdcFoundationImportError::InvalidInput(format!(
                "{field} must not be empty"
            )));
        }
    }
    Ok(())
}

async fn stage_import(
    tx: &mut Transaction<'_, Postgres>,
    request: &FdcFoundationImportRequest,
    prepared: &PreparedImport,
) -> Result<FdcFoundationImportReport, FdcFoundationImportError> {
    let dataset_id = ensure_fdc_dataset(tx).await?;
    let dataset_release_id = ensure_dataset_release(
        tx,
        dataset_id,
        &prepared.foods,
        request,
        &prepared.source_sha256,
        &prepared.schema_fingerprint,
    )
    .await?;
    store_raw_records(tx, dataset_release_id, &prepared.foods).await?;
    mark_dataset_release_imported(tx, dataset_release_id).await?;

    if let Some(existing_catalog_release_id) =
        existing_catalog_release(tx, &prepared.catalog_release_version).await?
    {
        return Ok(import_report(
            prepared,
            dataset_release_id,
            existing_catalog_release_id,
            true,
        ));
    }

    ensure_core_nutrients(tx).await?;
    let catalog_release_id =
        create_staged_catalog_release(tx, prepared, dataset_release_id).await?;
    stage_reviewed_selection(
        tx,
        prepared,
        request,
        dataset_release_id,
        catalog_release_id,
    )
    .await?;
    Ok(import_report(
        prepared,
        dataset_release_id,
        catalog_release_id,
        false,
    ))
}

async fn mark_dataset_release_imported(
    tx: &mut Transaction<'_, Postgres>,
    dataset_release_id: Uuid,
) -> Result<(), FdcFoundationImportError> {
    sqlx::query(
        "UPDATE raw.dataset_release
            SET status = 'imported', imported_at = COALESCE(imported_at, now())
          WHERE id = $1 AND status IN ('discovered', 'validated', 'imported')",
    )
    .bind(dataset_release_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn existing_catalog_release(
    tx: &mut Transaction<'_, Postgres>,
    version: &str,
) -> Result<Option<Uuid>, FdcFoundationImportError> {
    sqlx::query_scalar::<_, Uuid>("SELECT id FROM catalog.catalog_release WHERE version = $1")
        .bind(version)
        .fetch_optional(&mut **tx)
        .await
        .map_err(FdcFoundationImportError::Query)
}

async fn create_staged_catalog_release(
    tx: &mut Transaction<'_, Postgres>,
    prepared: &PreparedImport,
    dataset_release_id: Uuid,
) -> Result<Uuid, FdcFoundationImportError> {
    let catalog_release_id = Uuid::now_v7();
    let manifest = json!({
        "source": FDC_DATASET_CODE,
        "source_dataset_release_id": dataset_release_id,
        "importer_version": FDC_FOUNDATION_IMPORTER_VERSION,
        "preprocessing_policy_version": prepared.preprocessing_policy_version,
        "normalized_payload_sha256": prepared.normalized_payload_sha256,
        "selection_sha256": prepared.selection_fingerprint,
        "selected_fdc_ids": prepared.selected_ids.iter().copied().collect::<Vec<_>>(),
        "selected_count": prepared.selected_ids.len(),
        "raw_record_count": prepared.foods.len(),
        "energy_policy": FDC_ENERGY_MAPPING_POLICY_VERSION,
        "energy_mapping": {
            "atwater_specific_2048_count": prepared.energy_summary.atwater_specific,
            "atwater_general_2047_count": prepared.energy_summary.atwater_general,
            "missing_energy_count": prepared.energy_summary.missing_energy,
            "unexpected_legacy_1008_count": prepared.energy_summary.unexpected_legacy
        },
        "production_eligible": false
    });
    let catalog_checksum = sha256_hex(&serde_json::to_vec(&manifest)?);
    sqlx::query(
        "INSERT INTO catalog.catalog_release
            (id, version, status, manifest, checksum_sha256, created_by)
         VALUES ($1, $2, 'staged', $3, $4, $5)",
    )
    .bind(catalog_release_id)
    .bind(&prepared.catalog_release_version)
    .bind(&manifest)
    .bind(catalog_checksum)
    .bind(prepared.created_by)
    .execute(&mut **tx)
    .await?;
    Ok(catalog_release_id)
}

async fn stage_reviewed_selection(
    tx: &mut Transaction<'_, Postgres>,
    prepared: &PreparedImport,
    request: &FdcFoundationImportRequest,
    dataset_release_id: Uuid,
    catalog_release_id: Uuid,
) -> Result<(), FdcFoundationImportError> {
    for food in prepared
        .foods
        .iter()
        .filter(|food| prepared.selected_ids.contains(&food.fdc_id))
    {
        stage_selected_food(tx, request, dataset_release_id, catalog_release_id, food).await?;
    }
    Ok(())
}

fn import_report(
    prepared: &PreparedImport,
    dataset_release_id: Uuid,
    catalog_release_id: Uuid,
    replayed: bool,
) -> FdcFoundationImportReport {
    FdcFoundationImportReport {
        dataset_release_id,
        catalog_release_id,
        catalog_release_version: prepared.catalog_release_version.clone(),
        raw_record_count: prepared.foods.len(),
        selected_record_count: prepared.selected_ids.len(),
        source_sha256: prepared.source_sha256.clone(),
        schema_fingerprint: prepared.schema_fingerprint.clone(),
        energy_atwater_specific_count: prepared.energy_summary.atwater_specific,
        energy_atwater_general_count: prepared.energy_summary.atwater_general,
        energy_missing_count: prepared.energy_summary.missing_energy,
        unexpected_legacy_energy_count: prepared.energy_summary.unexpected_legacy,
        replayed,
    }
}

fn normalize_sha256(value: &str) -> Result<String, FdcFoundationImportError> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.len() != 64 || !normalized.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(FdcFoundationImportError::InvalidInput(
            "expected_sha256 must be exactly 64 hexadecimal characters".to_owned(),
        ));
    }
    Ok(normalized)
}

fn reviewed_selection(values: &[u64]) -> Result<BTreeSet<u64>, FdcFoundationImportError> {
    if values.is_empty() {
        return Err(FdcFoundationImportError::InvalidInput(
            "include_fdc_ids must contain at least one reviewed FDC ID".to_owned(),
        ));
    }
    let selected = values.iter().copied().collect::<BTreeSet<_>>();
    if selected.len() != values.len() {
        return Err(FdcFoundationImportError::InvalidInput(
            "include_fdc_ids must not contain duplicates".to_owned(),
        ));
    }
    Ok(selected)
}

fn parse_source_foods(source_bytes: &[u8]) -> Result<Vec<RawFood>, FdcFoundationImportError> {
    let root: Value = serde_json::from_slice(source_bytes)?;
    let food_values = root
        .get("FoundationFoods")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            FdcFoundationImportError::InvalidInput(
                "FDC JSON root must contain a FoundationFoods array".to_owned(),
            )
        })?;
    if food_values.is_empty() {
        return Err(FdcFoundationImportError::InvalidInput(
            "FoundationFoods must not be empty".to_owned(),
        ));
    }

    let mut seen_ids = BTreeSet::new();
    let mut foods = Vec::with_capacity(food_values.len());
    for payload in food_values {
        let food = parse_food_record(payload.clone())?;
        let fdc_id = food.fdc_id;
        if !seen_ids.insert(fdc_id) {
            return Err(FdcFoundationImportError::InvalidInput(format!(
                "duplicate FDC ID {fdc_id} in source artifact"
            )));
        }
        foods.push(food);
    }
    Ok(foods)
}

fn parse_food_record(payload: Value) -> Result<RawFood, FdcFoundationImportError> {
    let fdc_id = payload
        .get("fdcId")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            FdcFoundationImportError::InvalidInput(
                "every Foundation food must contain an unsigned integer fdcId".to_owned(),
            )
        })?;
    if payload.get("dataType").and_then(Value::as_str) != Some("Foundation") {
        return Err(FdcFoundationImportError::InvalidInput(format!(
            "FDC ID {fdc_id} is not a Foundation food"
        )));
    }
    let description = payload
        .get("description")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|description| !description.is_empty())
        .ok_or_else(|| {
            FdcFoundationImportError::InvalidInput(format!(
                "FDC ID {fdc_id} has no non-empty description"
            ))
        })?
        .to_owned();
    Ok(RawFood {
        fdc_id,
        description,
        payload,
    })
}

fn validate_food_schema(food: &RawFood) -> Vec<String> {
    let Some(food_nutrients) = food.payload.get("foodNutrients").and_then(Value::as_array) else {
        return vec![format!("FDC ID {} has no foodNutrients array", food.fdc_id)];
    };
    let mut errors = Vec::new();
    for (index, item) in food_nutrients.iter().enumerate() {
        let Some(nutrient) = item.get("nutrient") else {
            errors.push(format!(
                "FDC ID {} foodNutrients[{index}] has no nutrient object",
                food.fdc_id
            ));
            continue;
        };
        if nutrient.get("id").and_then(Value::as_u64).is_none() {
            errors.push(format!(
                "FDC ID {} foodNutrients[{index}].nutrient.id is not an unsigned integer",
                food.fdc_id
            ));
        }
        if nutrient.get("unitName").and_then(Value::as_str).is_none() {
            errors.push(format!(
                "FDC ID {} foodNutrients[{index}].nutrient.unitName is not a string",
                food.fdc_id
            ));
        }
    }
    errors
}

fn validate_selection_exists(
    foods: &[RawFood],
    selected_ids: &BTreeSet<u64>,
) -> Result<(), FdcFoundationImportError> {
    let available = foods
        .iter()
        .map(|food| food.fdc_id)
        .collect::<BTreeSet<_>>();
    let missing = selected_ids
        .difference(&available)
        .copied()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(FdcFoundationImportError::InvalidInput(format!(
            "reviewed FDC IDs are missing from the pinned artifact: {missing:?}"
        )));
    }
    Ok(())
}

fn selection_fingerprint(selected_ids: &BTreeSet<u64>) -> String {
    let joined = selected_ids
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(",");
    sha256_hex(joined.as_bytes())
}

fn add_energy_summary(summary: &mut EnergySummary, energy: &EnergyExtraction) {
    match energy
        .selected
        .as_ref()
        .map(|value| value.source_nutrient_id)
    {
        Some(2048) => summary.atwater_specific += 1,
        Some(2047) => summary.atwater_general += 1,
        None => summary.missing_energy += 1,
        Some(_) => unreachable!("energy extraction only selects 2048 or 2047"),
    }
    summary.unexpected_legacy += energy.unexpected_legacy_count;
}

fn schema_fingerprint() -> String {
    sha256_hex(FDC_SCHEMA_CONTRACT.as_bytes())
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

async fn ensure_fdc_dataset(
    tx: &mut Transaction<'_, Postgres>,
) -> Result<Uuid, FdcFoundationImportError> {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO raw.dataset
            (id, code, name, publisher, license_code, license_url, homepage,
             ingestion_policy_version)
         VALUES
            ($1, $2, 'USDA FoodData Central',
             'U.S. Department of Agriculture, Agricultural Research Service',
             'CC0-1.0', 'https://creativecommons.org/publicdomain/zero/1.0/',
             'https://fdc.nal.usda.gov/', $3)
         ON CONFLICT (code) DO NOTHING",
    )
    .bind(id)
    .bind(FDC_DATASET_CODE)
    .bind(FDC_FOUNDATION_IMPORTER_VERSION)
    .execute(&mut **tx)
    .await?;

    sqlx::query_scalar("SELECT id FROM raw.dataset WHERE code = $1")
        .bind(FDC_DATASET_CODE)
        .fetch_one(&mut **tx)
        .await
        .map_err(FdcFoundationImportError::Query)
}

async fn ensure_dataset_release(
    tx: &mut Transaction<'_, Postgres>,
    dataset_id: Uuid,
    foods: &[RawFood],
    request: &FdcFoundationImportRequest,
    source_sha256: &str,
    schema_fingerprint: &str,
) -> Result<Uuid, FdcFoundationImportError> {
    if let Some((id, existing_checksum, existing_schema, existing_count)) =
        sqlx::query_as::<_, (Uuid, String, String, i64)>(
            "SELECT id, checksum_sha256, schema_fingerprint, record_count
               FROM raw.dataset_release
              WHERE dataset_id = $1 AND version = $2",
        )
        .bind(dataset_id)
        .bind(&request.release_version)
        .fetch_optional(&mut **tx)
        .await?
    {
        validate_existing_release(
            &request.release_version,
            source_sha256,
            schema_fingerprint,
            foods.len(),
            &existing_checksum,
            &existing_schema,
            existing_count,
        )?;
        return Ok(id);
    }

    create_dataset_release(
        tx,
        dataset_id,
        foods.len(),
        request,
        source_sha256,
        schema_fingerprint,
    )
    .await
}

fn validate_existing_release(
    release_version: &str,
    source_sha256: &str,
    schema_fingerprint: &str,
    food_count: usize,
    existing_checksum: &str,
    existing_schema: &str,
    existing_count: i64,
) -> Result<(), FdcFoundationImportError> {
    if existing_checksum != source_sha256 {
        return Err(FdcFoundationImportError::ReleaseConflict(format!(
            "release {release_version} already has checksum {existing_checksum}, not {source_sha256}"
        )));
    }
    if existing_schema != schema_fingerprint {
        return Err(FdcFoundationImportError::ReleaseConflict(format!(
            "release {release_version} already has schema fingerprint {existing_schema}, not {schema_fingerprint}"
        )));
    }
    let current_count = i64::try_from(food_count).map_err(|_| {
        FdcFoundationImportError::InvalidInput("FDC record count exceeds i64".to_owned())
    })?;
    if existing_count != current_count {
        return Err(FdcFoundationImportError::ReleaseConflict(format!(
            "release {release_version} already has record count {existing_count}, not {current_count}"
        )));
    }
    Ok(())
}

async fn create_dataset_release(
    tx: &mut Transaction<'_, Postgres>,
    dataset_id: Uuid,
    food_count: usize,
    request: &FdcFoundationImportRequest,
    source_sha256: &str,
    schema_fingerprint: &str,
) -> Result<Uuid, FdcFoundationImportError> {
    let id = Uuid::now_v7();
    let record_count = i64::try_from(food_count).map_err(|_| {
        FdcFoundationImportError::InvalidInput("FDC record count exceeds i64".to_owned())
    })?;
    let metadata = json!({
        "data_type": "Foundation",
        "importer_version": FDC_FOUNDATION_IMPORTER_VERSION,
        "source_download_url": FDC_SOURCE_DOWNLOAD_URL,
        "schema_fingerprint_kind": "validated_import_contract"
    });
    sqlx::query(
        "INSERT INTO raw.dataset_release
            (id, dataset_id, version, published_at, object_uri, checksum_sha256,
             schema_fingerprint, record_count, metadata, status)
         VALUES ($1, $2, $3, ($4 || 'T00:00:00Z')::timestamptz, $5, $6, $7, $8, $9, 'validated')",
    )
    .bind(id)
    .bind(dataset_id)
    .bind(&request.release_version)
    .bind(&request.source_published_date)
    .bind(&request.object_uri)
    .bind(source_sha256)
    .bind(schema_fingerprint)
    .bind(record_count)
    .bind(metadata)
    .execute(&mut **tx)
    .await?;
    Ok(id)
}

async fn store_raw_records(
    tx: &mut Transaction<'_, Postgres>,
    dataset_release_id: Uuid,
    foods: &[RawFood],
) -> Result<(), FdcFoundationImportError> {
    for food in foods {
        let payload_hash = sha256_hex(&serde_json::to_vec(&food.payload)?);
        sqlx::query(
            "INSERT INTO raw.source_food_record
                (id, dataset_release_id, external_id, source_data_type, source_description,
                 normalized_search_text, raw_payload, payload_hash)
             VALUES ($1, $2, $3, 'Foundation', $4, $5, $6, $7)
             ON CONFLICT (dataset_release_id, external_id) DO NOTHING",
        )
        .bind(Uuid::now_v7())
        .bind(dataset_release_id)
        .bind(food.fdc_id.to_string())
        .bind(&food.description)
        .bind(food.description.to_lowercase())
        .bind(&food.payload)
        .bind(payload_hash)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn ensure_core_nutrients(
    tx: &mut Transaction<'_, Postgres>,
) -> Result<(), FdcFoundationImportError> {
    for (code, name, unit, group_code, is_energy_component) in [
        ("energy_kcal", "Energy", "kcal", "energy", false),
        ("protein_g", "Protein", "g", "macronutrient/protein", true),
        ("fat_g", "Fat", "g", "macronutrient/fat", true),
        (
            "carbohydrate_g",
            "Carbohydrate",
            "g",
            "macronutrient/carbohydrate",
            true,
        ),
    ] {
        sqlx::query(
            "INSERT INTO composition.nutrient
                (id, code, preferred_name, canonical_unit, nutrient_group,
                 external_identifiers, is_energy_component)
             VALUES ($1, $2, $3, $4, $5, '{}'::jsonb, $6)
             ON CONFLICT (code) DO NOTHING",
        )
        .bind(Uuid::now_v7())
        .bind(code)
        .bind(name)
        .bind(unit)
        .bind(group_code)
        .bind(is_energy_component)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn stage_selected_food(
    tx: &mut Transaction<'_, Postgres>,
    request: &FdcFoundationImportRequest,
    dataset_release_id: Uuid,
    catalog_release_id: Uuid,
    food: &RawFood,
) -> Result<(), FdcFoundationImportError> {
    let source_record_id = source_record_id(tx, dataset_release_id, food.fdc_id).await?;
    let food_id = ensure_food_entity(tx, food.fdc_id).await?;
    ensure_food_mapping(tx, source_record_id, food_id).await?;
    let food_name_id = ensure_food_name(tx, source_record_id, food_id, food).await?;
    add_name_to_release(tx, catalog_release_id, food_name_id).await?;
    let profile_id = ensure_staged_profile(tx, source_record_id, food_id, request, food).await?;
    add_profile_to_release(tx, catalog_release_id, profile_id).await
}

async fn source_record_id(
    tx: &mut Transaction<'_, Postgres>,
    dataset_release_id: Uuid,
    fdc_id: u64,
) -> Result<Uuid, FdcFoundationImportError> {
    sqlx::query_scalar(
        "SELECT id FROM raw.source_food_record WHERE dataset_release_id = $1 AND external_id = $2",
    )
    .bind(dataset_release_id)
    .bind(fdc_id.to_string())
    .fetch_one(&mut **tx)
    .await
    .map_err(FdcFoundationImportError::Query)
}

async fn ensure_food_entity(
    tx: &mut Transaction<'_, Postgres>,
    fdc_id: u64,
) -> Result<Uuid, FdcFoundationImportError> {
    let semantic_key = format!("usda-fdc:{fdc_id}");
    sqlx::query(
        "INSERT INTO catalog.food_entity (id, semantic_key, entity_kind, lifecycle_status)
         VALUES ($1, $2, 'basic_food', 'draft')
         ON CONFLICT (semantic_key) DO NOTHING",
    )
    .bind(Uuid::now_v7())
    .bind(&semantic_key)
    .execute(&mut **tx)
    .await?;
    sqlx::query_scalar("SELECT id FROM catalog.food_entity WHERE semantic_key = $1")
        .bind(semantic_key)
        .fetch_one(&mut **tx)
        .await
        .map_err(FdcFoundationImportError::Query)
}

async fn ensure_food_mapping(
    tx: &mut Transaction<'_, Postgres>,
    source_record_id: Uuid,
    food_id: Uuid,
) -> Result<(), FdcFoundationImportError> {
    sqlx::query(
        "INSERT INTO catalog.food_mapping
            (id, source_food_record_id, food_id, mapping_type, mapping_method, score,
             policy_version, review_status, rationale)
         SELECT $1, $2, $3, 'exact', 'fdc_exact_external_id', 1.0, $4, 'proposed',
                'Deterministic mapping from the pinned FDC external ID; requires catalog review before publication'
          WHERE NOT EXISTS (
              SELECT 1 FROM catalog.food_mapping
               WHERE source_food_record_id = $2 AND food_id = $3
          )",
    )
    .bind(Uuid::now_v7())
    .bind(source_record_id)
    .bind(food_id)
    .bind(FDC_FOUNDATION_IMPORTER_VERSION)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn ensure_food_name(
    tx: &mut Transaction<'_, Postgres>,
    source_record_id: Uuid,
    food_id: Uuid,
    food: &RawFood,
) -> Result<Uuid, FdcFoundationImportError> {
    if let Some(id) = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM catalog.food_name
          WHERE food_id = $1 AND source_record_id = $2 AND locale = 'en-US' AND name = $3
          ORDER BY valid_from
          LIMIT 1",
    )
    .bind(food_id)
    .bind(source_record_id)
    .bind(&food.description)
    .fetch_optional(&mut **tx)
    .await?
    {
        return Ok(id);
    }

    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO catalog.food_name
            (id, food_id, locale, name, normalized_name, name_type, source_record_id,
             is_curated, search_weight)
         VALUES ($1, $2, 'en-US', $3, $4, 'preferred', $5, false, 0)",
    )
    .bind(id)
    .bind(food_id)
    .bind(&food.description)
    .bind(food.description.to_lowercase())
    .bind(source_record_id)
    .execute(&mut **tx)
    .await?;
    Ok(id)
}

async fn add_name_to_release(
    tx: &mut Transaction<'_, Postgres>,
    catalog_release_id: Uuid,
    food_name_id: Uuid,
) -> Result<(), FdcFoundationImportError> {
    sqlx::query(
        "INSERT INTO catalog.catalog_release_food_name (catalog_release_id, food_name_id)
         VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(catalog_release_id)
    .bind(food_name_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn ensure_staged_profile(
    tx: &mut Transaction<'_, Postgres>,
    source_record_id: Uuid,
    food_id: Uuid,
    request: &FdcFoundationImportRequest,
    food: &RawFood,
) -> Result<Uuid, FdcFoundationImportError> {
    if let Some(id) = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM composition.composition_profile
          WHERE food_id = $1 AND source_record_id = $2
            AND basis_amount = 100 AND basis_unit = 'g' AND edible_basis
            AND method_metadata->>'importer_version' = $3
          ORDER BY created_at
          LIMIT 1",
    )
    .bind(food_id)
    .bind(source_record_id)
    .bind(FDC_FOUNDATION_IMPORTER_VERSION)
    .fetch_optional(&mut **tx)
    .await?
    {
        return Ok(id);
    }
    create_staged_profile(tx, food_id, source_record_id, request, food).await
}

async fn add_profile_to_release(
    tx: &mut Transaction<'_, Postgres>,
    catalog_release_id: Uuid,
    profile_id: Uuid,
) -> Result<(), FdcFoundationImportError> {
    sqlx::query(
        "INSERT INTO catalog.catalog_release_profile (catalog_release_id, profile_id)
         VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(catalog_release_id)
    .bind(profile_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn create_staged_profile(
    tx: &mut Transaction<'_, Postgres>,
    food_id: Uuid,
    source_record_id: Uuid,
    request: &FdcFoundationImportRequest,
    food: &RawFood,
) -> Result<Uuid, FdcFoundationImportError> {
    let energy = extract_energy(food)?;
    let energy_mapping = json!({
        "policy_version": FDC_ENERGY_MAPPING_POLICY_VERSION,
        "status": if energy.selected.is_some() { "complete" } else { "incomplete" },
        "source_nutrient_id": energy
            .selected
            .as_ref()
            .map(|value| value.source_nutrient_id),
        "source_method": energy
            .selected
            .as_ref()
            .and_then(|value| value.source_method),
        "unexpected_legacy_1008_count": energy.unexpected_legacy_count
    });
    let mut nutrients = extract_unambiguous_macronutrients(food)?;
    if let Some(energy) = energy.selected {
        nutrients.push(energy);
    }
    let profile_id = Uuid::now_v7();
    let method_metadata = json!({
        "source": FDC_DATASET_CODE,
        "source_release": request.release_version,
        "source_published_date": request.source_published_date,
        "fdc_id": food.fdc_id,
        "importer_version": FDC_FOUNDATION_IMPORTER_VERSION,
        "energy_mapping": energy_mapping,
        "production_eligible": false
    });
    sqlx::query(
        "INSERT INTO composition.composition_profile
            (id, food_id, source_record_id, profile_type, basis_amount, basis_unit, edible_basis,
             quality_grade, method_metadata, status)
         VALUES ($1, $2, $3, 'laboratory', 100, 'g', true, 'U', $4, 'in_review')",
    )
    .bind(profile_id)
    .bind(food_id)
    .bind(source_record_id)
    .bind(method_metadata)
    .execute(&mut **tx)
    .await?;

    insert_staged_values(tx, profile_id, request, food, nutrients).await?;
    Ok(profile_id)
}

async fn insert_staged_values(
    tx: &mut Transaction<'_, Postgres>,
    profile_id: Uuid,
    request: &FdcFoundationImportRequest,
    food: &RawFood,
    nutrients: Vec<StagedNutrient>,
) -> Result<(), FdcFoundationImportError> {
    let nutrient_ids = sqlx::query_as::<_, (String, Uuid)>(
        "SELECT code, id FROM composition.nutrient
          WHERE code IN ('energy_kcal', 'protein_g', 'fat_g', 'carbohydrate_g')",
    )
    .fetch_all(&mut **tx)
    .await?
    .into_iter()
    .collect::<BTreeMap<_, _>>();

    for nutrient in nutrients {
        let nutrient_id = nutrient_ids.get(nutrient.internal_code).ok_or_else(|| {
            FdcFoundationImportError::InvalidInput(format!(
                "internal nutrient {} is unavailable",
                nutrient.internal_code
            ))
        })?;
        let source_nutrient_id = i64::try_from(nutrient.source_nutrient_id).map_err(|_| {
            FdcFoundationImportError::InvalidInput(format!(
                "source nutrient ID {} exceeds PostgreSQL bigint",
                nutrient.source_nutrient_id
            ))
        })?;
        let unit = if nutrient.internal_code == "energy_kcal" {
            "kcal"
        } else {
            "g"
        };
        let source_metadata = json!({
            "source": FDC_DATASET_CODE,
            "source_release": request.release_version,
            "source_food_id": food.fdc_id,
            "source_nutrient_id": nutrient.source_nutrient_id,
            "source_method": nutrient.source_method,
            "importer_version": FDC_FOUNDATION_IMPORTER_VERSION,
            "energy_mapping_policy": if nutrient.internal_code == "energy_kcal" {
                Some(FDC_ENERGY_MAPPING_POLICY_VERSION)
            } else {
                None
            }
        });
        sqlx::query(
            "INSERT INTO composition.composition_value
                (profile_id, nutrient_id, amount, unit, minimum_amount, maximum_amount,
                 value_status, method_code, source_nutrient_id, source_method, source_metadata)
             VALUES ($1, $2, $3, $4, $5, $6, 'compiled', $7, $8, $9, $10)",
        )
        .bind(profile_id)
        .bind(nutrient_id)
        .bind(nutrient.amount)
        .bind(unit)
        .bind(nutrient.minimum)
        .bind(nutrient.maximum)
        .bind(nutrient.method_code)
        .bind(source_nutrient_id)
        .bind(nutrient.source_method)
        .bind(source_metadata)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

fn extract_unambiguous_macronutrients(
    food: &RawFood,
) -> Result<Vec<StagedNutrient>, FdcFoundationImportError> {
    let food_nutrients = food
        .payload
        .get("foodNutrients")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            FdcFoundationImportError::InvalidInput(format!(
                "FDC ID {} has no foodNutrients array",
                food.fdc_id
            ))
        })?;

    let definitions = [
        (1003_u64, "protein_g", "g"),
        (1004_u64, "fat_g", "g"),
        (1005_u64, "carbohydrate_g", "g"),
    ];
    let mut values = Vec::with_capacity(definitions.len());
    for (source_nutrient_id, internal_code, expected_unit) in definitions {
        values.push(extract_macronutrient(
            food,
            food_nutrients,
            source_nutrient_id,
            internal_code,
            expected_unit,
        )?);
    }
    Ok(values)
}

fn extract_macronutrient(
    food: &RawFood,
    food_nutrients: &[Value],
    source_nutrient_id: u64,
    internal_code: &'static str,
    expected_unit: &str,
) -> Result<StagedNutrient, FdcFoundationImportError> {
    let matches = food_nutrients
        .iter()
        .filter(|item| {
            item.get("nutrient")
                .and_then(|nutrient| nutrient.get("id"))
                .and_then(Value::as_u64)
                == Some(source_nutrient_id)
        })
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(FdcFoundationImportError::InvalidInput(format!(
            "FDC ID {} must contain exactly one source nutrient {} for {}",
            food.fdc_id, source_nutrient_id, internal_code
        )));
    }
    let item = matches[0];
    staged_nutrient_from_item(
        food,
        item,
        source_nutrient_id,
        internal_code,
        expected_unit,
        None,
    )
}

fn extract_energy(food: &RawFood) -> Result<EnergyExtraction, FdcFoundationImportError> {
    let food_nutrients = food
        .payload
        .get("foodNutrients")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            FdcFoundationImportError::InvalidInput(format!(
                "FDC ID {} has no foodNutrients array",
                food.fdc_id
            ))
        })?;

    let specific = extract_energy_candidate(food, food_nutrients, 2048, "atwater_specific")?;
    let general = extract_energy_candidate(food, food_nutrients, 2047, "atwater_general")?;
    let unexpected_legacy_count = food_nutrients
        .iter()
        .filter(|item| {
            item.get("nutrient")
                .and_then(|nutrient| nutrient.get("id"))
                .and_then(Value::as_u64)
                == Some(1008)
        })
        .count();

    Ok(EnergyExtraction {
        selected: specific.or(general),
        unexpected_legacy_count,
    })
}

fn extract_energy_candidate(
    food: &RawFood,
    food_nutrients: &[Value],
    source_nutrient_id: u64,
    source_method: &'static str,
) -> Result<Option<StagedNutrient>, FdcFoundationImportError> {
    let matches = food_nutrients
        .iter()
        .filter(|item| {
            item.get("nutrient")
                .and_then(|nutrient| nutrient.get("id"))
                .and_then(Value::as_u64)
                == Some(source_nutrient_id)
        })
        .collect::<Vec<_>>();
    if matches.len() > 1 {
        return Err(FdcFoundationImportError::InvalidInput(format!(
            "FDC ID {} contains duplicate energy nutrient {}",
            food.fdc_id, source_nutrient_id
        )));
    }
    let Some(item) = matches.first().copied() else {
        return Ok(None);
    };
    staged_nutrient_from_item(
        food,
        item,
        source_nutrient_id,
        "energy_kcal",
        "kcal",
        Some(source_method),
    )
    .map(Some)
}

fn staged_nutrient_from_item(
    food: &RawFood,
    item: &Value,
    source_nutrient_id: u64,
    internal_code: &'static str,
    expected_unit: &str,
    source_method: Option<&'static str>,
) -> Result<StagedNutrient, FdcFoundationImportError> {
    validate_nutrient_unit(food.fdc_id, source_nutrient_id, item, expected_unit)?;
    let amount = required_nonnegative_amount(food.fdc_id, source_nutrient_id, item)?;
    let minimum = decimal_field(item, "min")?;
    let maximum = decimal_field(item, "max")?;
    if let (Some(minimum), Some(maximum)) = (minimum, maximum)
        && minimum > maximum
    {
        return Err(FdcFoundationImportError::InvalidInput(format!(
            "FDC ID {} nutrient {} has min greater than max",
            food.fdc_id, source_nutrient_id
        )));
    }
    let method_code = item
        .get("foodNutrientDerivation")
        .and_then(|derivation| derivation.get("code"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    Ok(StagedNutrient {
        internal_code,
        source_nutrient_id,
        source_method,
        amount,
        minimum,
        maximum,
        method_code,
    })
}

fn validate_nutrient_unit(
    fdc_id: u64,
    source_nutrient_id: u64,
    item: &Value,
    expected_unit: &str,
) -> Result<(), FdcFoundationImportError> {
    let unit = item
        .get("nutrient")
        .and_then(|nutrient| nutrient.get("unitName"))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            FdcFoundationImportError::InvalidInput(format!(
                "FDC ID {fdc_id} nutrient {source_nutrient_id} has no unitName"
            ))
        })?;
    if !unit.eq_ignore_ascii_case(expected_unit) {
        return Err(FdcFoundationImportError::InvalidInput(format!(
            "FDC ID {fdc_id} nutrient {source_nutrient_id} uses unit {unit}, expected {expected_unit}"
        )));
    }
    Ok(())
}

fn required_nonnegative_amount(
    fdc_id: u64,
    source_nutrient_id: u64,
    item: &Value,
) -> Result<Decimal, FdcFoundationImportError> {
    let amount = decimal_field(item, "amount")?.ok_or_else(|| {
        FdcFoundationImportError::InvalidInput(format!(
            "FDC ID {fdc_id} nutrient {source_nutrient_id} has no amount"
        ))
    })?;
    if amount.is_sign_negative() {
        return Err(FdcFoundationImportError::InvalidInput(format!(
            "FDC ID {fdc_id} nutrient {source_nutrient_id} has a negative amount"
        )));
    }
    Ok(amount)
}

fn decimal_field(value: &Value, field: &str) -> Result<Option<Decimal>, FdcFoundationImportError> {
    let Some(raw) = value.get(field) else {
        return Ok(None);
    };
    if raw.is_null() {
        return Ok(None);
    }
    let text = match raw {
        Value::Number(number) => number.to_string(),
        Value::String(text) => text.clone(),
        _ => {
            return Err(FdcFoundationImportError::InvalidInput(format!(
                "{field} must be numeric"
            )));
        }
    };
    Decimal::from_str(&text)
        .map(Some)
        .map_err(|_| FdcFoundationImportError::InvalidInput(format!("{field} is not a decimal")))
}

#[cfg(test)]
mod tests {
    use super::{
        FdcFoundationValidationRequest, extract_energy, extract_unambiguous_macronutrients,
        parse_source_foods, reviewed_selection, transform_fdc_foundation_2026_04_null_tail,
        validate_fdc_foundation_json,
    };
    use serde_json::{Value, json};
    use std::collections::BTreeSet;

    const MINIMAL: &str = r#"{
      "FoundationFoods": [{
        "fdcId": 900000001,
        "dataType": "Foundation",
        "description": "Synthetic foundation food",
        "foodNutrients": [
          {"amount": 2.5, "nutrient": {"id": 1003, "unitName": "G"}},
          {"amount": 3.5, "nutrient": {"id": 1004, "unitName": "G"}},
          {"amount": 4.5, "nutrient": {"id": 1005, "unitName": "G"}},
          {"amount": 99, "nutrient": {"id": 2048, "unitName": "KCAL"}}
        ]
      }]
    }"#;

    #[test]
    fn parses_foundation_shape_and_preserves_specific_energy_provenance() {
        let foods = parse_source_foods(MINIMAL.as_bytes()).expect("fixture must parse");
        let nutrients = extract_unambiguous_macronutrients(&foods[0])
            .expect("unambiguous macro nutrients must map");
        let codes = nutrients
            .iter()
            .map(|nutrient| nutrient.internal_code)
            .collect::<BTreeSet<_>>();
        assert_eq!(
            codes,
            BTreeSet::from(["carbohydrate_g", "fat_g", "protein_g"])
        );
        let energy = extract_energy(&foods[0])
            .expect("specific energy must parse")
            .selected
            .expect("specific energy must be selected");
        assert_eq!(energy.internal_code, "energy_kcal");
        assert_eq!(energy.source_nutrient_id, 2048);
        assert_eq!(energy.source_method, Some("atwater_specific"));
        assert_eq!(energy.amount.to_string(), "99");
    }

    #[test]
    fn energy_policy_prefers_specific_and_falls_back_to_general() {
        let both = energy_food(&json!([
            nutrient(2048, "KCAL", &json!(52)),
            nutrient(2047, "KCAL", &json!(53))
        ]));
        let specific = extract_energy(&both)
            .expect("valid energy candidates must parse")
            .selected
            .expect("specific energy must win");
        assert_eq!(specific.source_nutrient_id, 2048);
        assert_eq!(specific.source_method, Some("atwater_specific"));

        let general = energy_food(&json!([nutrient(2047, "KCAL", &json!(53))]));
        let general = extract_energy(&general)
            .expect("general energy must parse")
            .selected
            .expect("general energy must be selected");
        assert_eq!(general.source_nutrient_id, 2047);
        assert_eq!(general.source_method, Some("atwater_general"));
    }

    #[test]
    fn energy_policy_marks_missing_and_legacy_values_incomplete() {
        let legacy = extract_energy(&energy_food(&json!([nutrient(1008, "KCAL", &json!(50))])))
            .expect("legacy energy is reportable");
        assert!(legacy.selected.is_none());
        assert_eq!(legacy.unexpected_legacy_count, 1);

        let missing =
            extract_energy(&energy_food(&json!([]))).expect("missing energy is reportable");
        assert!(missing.selected.is_none());
        assert_eq!(missing.unexpected_legacy_count, 0);
    }

    #[test]
    fn energy_policy_fails_closed_on_malformed_or_duplicate_candidates() {
        let malformed_specific = energy_food(&json!([
            nutrient(2048, "KCAL", &json!("malformed")),
            nutrient(2047, "KCAL", &json!(53))
        ]));
        assert!(extract_energy(&malformed_specific).is_err());

        let duplicate_specific = energy_food(&json!([
            nutrient(2048, "KCAL", &json!(52)),
            nutrient(2048, "KCAL", &json!(53))
        ]));
        assert!(extract_energy(&duplicate_specific).is_err());

        let invalid_unit = energy_food(&json!([nutrient(2048, "G", &json!(52))]));
        assert!(extract_energy(&invalid_unit).is_err());
    }

    fn nutrient(id: u64, unit: &str, amount: &Value) -> Value {
        json!({
            "amount": amount,
            "nutrient": {"id": id, "unitName": unit}
        })
    }

    fn energy_food(food_nutrients: &Value) -> super::RawFood {
        let payload = json!({
            "FoundationFoods": [{
                "fdcId": 900_000_099,
                "dataType": "Foundation",
                "description": "Synthetic energy policy food",
                "foodNutrients": food_nutrients
            }]
        });
        let bytes = serde_json::to_vec(&payload).expect("synthetic energy JSON must serialize");
        parse_source_foods(&bytes)
            .expect("synthetic energy food must parse")
            .remove(0)
    }

    #[test]
    fn reviewed_selection_rejects_empty_and_duplicate_ids() {
        assert!(reviewed_selection(&[]).is_err());
        assert!(reviewed_selection(&[1, 1]).is_err());
        assert_eq!(
            reviewed_selection(&[2, 1]).expect("unique selection").len(),
            2
        );
    }

    #[test]
    fn validation_report_is_deterministic_and_preserves_selection_energy_counts() {
        let request = validation_request(MINIMAL, vec![900_000_001]);
        let first = validate_fdc_foundation_json(MINIMAL.as_bytes(), &request);
        let second = validate_fdc_foundation_json(MINIMAL.as_bytes(), &request);

        assert_eq!(first, second);
        assert_eq!(first.artifact_status, "valid");
        assert_eq!(first.selection_status, "validated");
        assert_eq!(first.validation_status, "passed");
        assert_eq!(first.raw_record_count, 1);
        assert_eq!(first.selected_record_count, 1);
        assert_eq!(first.source_energy_atwater_specific_count, 1);
        assert_eq!(first.selected_energy_atwater_specific_count, 1);
        assert_eq!(
            first
                .to_pretty_json(&request)
                .expect("validation report must serialize"),
            second
                .to_pretty_json(&request)
                .expect("validation report must serialize")
        );
        assert_eq!(
            first
                .to_json(&request)
                .get("production_eligible")
                .and_then(Value::as_bool),
            Some(false)
        );
    }

    #[test]
    fn validation_report_fails_closed_on_null_records_and_unapproved_selection() {
        let source = r#"{
          "FoundationFoods": [
            {
              "fdcId": 900000001,
              "dataType": "Foundation",
              "description": "Synthetic foundation food",
              "foodNutrients": [
                {"amount": 2.5, "nutrient": {"id": 1003, "unitName": "G"}},
                {"amount": 3.5, "nutrient": {"id": 1004, "unitName": "G"}},
                {"amount": 4.5, "nutrient": {"id": 1005, "unitName": "G"}},
                {"amount": 99, "nutrient": {"id": 2048, "unitName": "KCAL"}}
              ]
            },
            null
          ]
        }"#;
        let request = validation_request(source, Vec::new());
        let report = validate_fdc_foundation_json(source.as_bytes(), &request);

        assert_eq!(report.raw_record_count, 2);
        assert_eq!(report.valid_record_count, 1);
        assert_eq!(report.null_record_count, 1);
        assert_eq!(report.invalid_record_count, 1);
        assert_eq!(report.artifact_status, "invalid");
        assert_eq!(report.selection_status, "not_approved");
        assert_eq!(report.validation_status, "blocked");
        assert!(report.errors.iter().any(|error| error.contains("is null")));
        assert!(
            report
                .errors
                .iter()
                .any(|error| error.contains("not approved"))
        );
    }

    #[test]
    fn null_tail_preprocessing_removes_only_the_approved_tail() {
        let mut records = (0..363)
            .map(|index| json!({"fdcId": index, "marker": format!("record-{index}")}))
            .collect::<Vec<_>>();
        records.extend(std::iter::repeat_n(Value::Null, 32));
        let mut root = json!({"FoundationFoods": records});

        let normalized = transform_fdc_foundation_2026_04_null_tail(&mut root)
            .expect("the exact 32-null tail must be transformable");
        let normalized_root: Value =
            serde_json::from_slice(&normalized).expect("normalized payload must be JSON");
        let foods = normalized_root
            .get("FoundationFoods")
            .and_then(Value::as_array)
            .expect("normalized payload must retain FoundationFoods");
        assert_eq!(foods.len(), 363);
        assert_eq!(foods[0]["marker"], "record-0");
        assert_eq!(foods[362]["marker"], "record-362");
    }

    #[test]
    fn null_tail_preprocessing_rejects_interior_or_non_null_tail_entries() {
        let mut interior_null_records = (0..363)
            .map(|index| json!({"fdcId": index}))
            .collect::<Vec<_>>();
        interior_null_records[12] = Value::Null;
        interior_null_records.extend(std::iter::repeat_n(Value::Null, 32));
        let mut interior_null_root = json!({"FoundationFoods": interior_null_records});
        let interior_error = transform_fdc_foundation_2026_04_null_tail(&mut interior_null_root)
            .expect_err("interior nulls must fail closed");
        assert!(interior_error.contains("interior null"));

        let mut non_null_tail_records = (0..363)
            .map(|index| json!({"fdcId": index}))
            .collect::<Vec<_>>();
        non_null_tail_records.extend(std::iter::repeat_n(Value::Null, 31));
        non_null_tail_records.push(json!({"fdcId": 999}));
        let mut non_null_tail_root = json!({"FoundationFoods": non_null_tail_records});
        let tail_error = transform_fdc_foundation_2026_04_null_tail(&mut non_null_tail_root)
            .expect_err("a non-null tail entry must fail closed");
        assert!(tail_error.contains("exactly 32 null entries"));
    }

    #[test]
    fn requested_preprocessing_rejects_unpinned_source_hashes() {
        let source = MINIMAL;
        let mut request = validation_request(source, Vec::new());
        request.preprocessing_policy_version =
            Some(super::FDC_FOUNDATION_2026_04_NULL_TAIL_POLICY_VERSION.to_owned());
        let report = validate_fdc_foundation_json(source.as_bytes(), &request);

        assert!(!report.source_integrity_valid.is_valid());
        assert!(!report.preprocessing_applied.is_applied());
        assert!(!report.normalized_payload_valid.is_valid());
        assert!(
            report
                .errors
                .iter()
                .any(|error| { error.contains("requires extracted JSON SHA-256") })
        );
    }

    fn validation_request(
        source: &str,
        reviewed_fdc_ids: Vec<u64>,
    ) -> FdcFoundationValidationRequest {
        FdcFoundationValidationRequest {
            release_version: "2026-04-30".to_owned(),
            source_published_date: "2026-04-30".to_owned(),
            object_uri: "fixture://fdc/validation.json".to_owned(),
            source_payload_filename: None,
            source_archive_sha256: None,
            expected_sha256: super::sha256_hex(source.as_bytes()),
            reviewed_fdc_ids,
            preprocessing_policy_version: None,
        }
    }
}
