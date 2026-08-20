#![allow(clippy::wildcard_imports)]

use super::*;

pub(crate) fn effective_validation_foods(
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

pub(crate) fn apply_requested_preprocessing(
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

pub(crate) fn preprocessing_contract_errors(
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

pub(crate) fn build_normalized_fdc_payload(source_bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut root = serde_json::from_slice::<Value>(source_bytes)
        .map_err(|error| format!("preprocessing source JSON parsing failed: {error}"))?;
    transform_fdc_foundation_2026_04_null_tail(&mut root)
}

pub(crate) fn transform_fdc_foundation_2026_04_null_tail(
    root: &mut Value,
) -> Result<Vec<u8>, String> {
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
