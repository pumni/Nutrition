#![allow(clippy::wildcard_imports)]

use super::*;

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
pub(crate) async fn import_fdc_foundation_json_impl(
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
pub(crate) fn validate_fdc_foundation_json_impl(
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

/// Builds a deterministic, human-reviewable candidate manifest from the exact normalized FDC
/// payload. This is evidence generation only: every candidate remains pending review and the
/// resulting manifest can never authorize catalog activation.
///
/// # Errors
///
/// Returns an error when the pinned source, preprocessing contract, normalized payload, or
/// candidate cap is invalid.
pub(crate) fn build_fdc_selection_candidate_manifest_impl(
    source_bytes: &[u8],
    request: &FdcFoundationValidationRequest,
    candidate_cap: usize,
) -> Result<Value, FdcFoundationImportError> {
    if candidate_cap == 0 || candidate_cap > FDC_FOUNDATION_V1_SELECTION_CAP {
        return Err(FdcFoundationImportError::InvalidInput(format!(
            "candidate_cap must be between 1 and {FDC_FOUNDATION_V1_SELECTION_CAP}"
        )));
    }

    let source_sha256 = sha256_hex(source_bytes);
    let (_, checksum_errors) = validate_checksum(
        &source_sha256,
        &request.expected_sha256.trim().to_ascii_lowercase(),
        request,
    );
    if !checksum_errors.is_empty() {
        return Err(FdcFoundationImportError::InvalidInput(
            checksum_errors.join("; "),
        ));
    }

    let preprocessing = apply_requested_preprocessing(source_bytes, &source_sha256, request);
    if !preprocessing.applied || !preprocessing.errors.is_empty() {
        return Err(FdcFoundationImportError::InvalidInput(format!(
            "pinned FDC preprocessing is not valid: {}",
            preprocessing.errors.join("; ")
        )));
    }
    let normalized_payload = preprocessing.normalized_payload.ok_or_else(|| {
        FdcFoundationImportError::InvalidInput(
            "pinned FDC preprocessing produced no normalized payload".to_owned(),
        )
    })?;
    let normalized_payload_sha256 = preprocessing.normalized_payload_sha256.ok_or_else(|| {
        FdcFoundationImportError::InvalidInput(
            "pinned FDC preprocessing produced no normalized payload hash".to_owned(),
        )
    })?;
    let mut schema_errors = Vec::new();
    let normalized = validate_source_records(&normalized_payload, &mut schema_errors);
    if !schema_errors.is_empty()
        || normalized.raw_record_count != FDC_FOUNDATION_2026_04_VALID_RECORD_COUNT
        || normalized.null_record_count != 0
        || normalized.invalid_record_count != 0
    {
        schema_errors.sort();
        return Err(FdcFoundationImportError::InvalidInput(format!(
            "normalized FDC payload is not structurally valid: {}",
            schema_errors.join("; ")
        )));
    }

    let mut eligible = normalized
        .foods
        .iter()
        .filter_map(candidate_from_food)
        .collect::<Vec<_>>();
    eligible.sort_by_key(|candidate| candidate.fdc_id);
    let eligible_population_count = eligible.len();
    eligible.truncate(candidate_cap);
    if eligible.is_empty() {
        return Err(FdcFoundationImportError::InvalidInput(
            "pinned normalized FDC payload contains no technically eligible candidate records"
                .to_owned(),
        ));
    }

    let selected_ids = eligible
        .iter()
        .map(|candidate| candidate.fdc_id)
        .collect::<BTreeSet<_>>();
    let candidates = eligible
        .into_iter()
        .map(|candidate| {
            json!({
                "fdc_id": candidate.fdc_id,
                "description": candidate.description,
                "source_record_status": "structurally_valid",
                "identity_status": "pending_human_review",
                "protein_status": "valid",
                "fat_status": "valid",
                "carbohydrate_status": "valid",
                "energy_source_nutrient_id": candidate.energy_source_nutrient_id,
                "energy_method": candidate.energy_method,
                "legacy_1008_present": false,
                "recipe_inference_used": false,
                "proposed_reason": "Foundation Food record with valid 1003/1004/1005 and fdc_energy_v1 evidence; selected by ascending FDC ID for a reviewer-sized initial allowlist.",
                "review_status": "pending_human_review"
            })
        })
        .collect::<Vec<_>>();

    Ok(selection_candidate_manifest(
        request,
        &source_sha256,
        &normalized_payload_sha256,
        eligible_population_count,
        candidate_cap,
        &selected_ids,
        &candidates,
    ))
}

pub(crate) fn prepare_import(
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

pub(crate) fn prepare_import_payload(
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

pub(crate) fn validate_selected_import_foods(
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

pub(crate) fn validate_request_metadata(
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
