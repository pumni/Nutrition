#![allow(clippy::wildcard_imports)]

use super::*;

pub(crate) fn validate_checksum(
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

pub(crate) fn validate_source_records(
    source_bytes: &[u8],
    errors: &mut Vec<String>,
) -> ValidatedFoods {
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

pub(crate) fn normalize_sha256(value: &str) -> Result<String, FdcFoundationImportError> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.len() != 64 || !normalized.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(FdcFoundationImportError::InvalidInput(
            "expected_sha256 must be exactly 64 hexadecimal characters".to_owned(),
        ));
    }
    Ok(normalized)
}

pub(crate) fn reviewed_selection(
    values: &[u64],
) -> Result<BTreeSet<u64>, FdcFoundationImportError> {
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

pub(crate) fn parse_source_foods(
    source_bytes: &[u8],
) -> Result<Vec<RawFood>, FdcFoundationImportError> {
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

pub(crate) fn parse_food_record(payload: Value) -> Result<RawFood, FdcFoundationImportError> {
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

pub(crate) fn validate_food_schema(food: &RawFood) -> Vec<String> {
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

pub(crate) fn validate_selection_exists(
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

pub(crate) fn selection_fingerprint(selected_ids: &BTreeSet<u64>) -> String {
    let joined = selected_ids
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(",");
    sha256_hex(joined.as_bytes())
}

pub(crate) fn schema_fingerprint() -> String {
    sha256_hex(FDC_SCHEMA_CONTRACT.as_bytes())
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
