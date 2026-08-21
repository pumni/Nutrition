#![allow(clippy::wildcard_imports)]

use super::*;

pub(crate) fn selection_candidate_manifest(
    request: &FdcFoundationValidationRequest,
    source_sha256: &str,
    normalized_payload_sha256: &str,
    eligible_population_count: usize,
    candidate_cap: usize,
    selected_ids: &BTreeSet<u64>,
    candidates: &[Value],
) -> Value {
    json!({
        "manifest_version": "fdc-selection-candidate-v1",
        "source": FDC_DATASET_CODE,
        "source_release": request.release_version,
        "source_published_date": request.source_published_date,
        "object_uri": request.object_uri,
        "source_payload_filename": request.source_payload_filename,
        "source_archive_sha256": request.source_archive_sha256,
        "source_payload_sha256": source_sha256,
        "normalized_payload_sha256": normalized_payload_sha256,
        "source_schema_fingerprint": schema_fingerprint(),
        "preprocessing_policy": request.preprocessing_policy_version,
        "importer_version": FDC_FOUNDATION_IMPORTER_VERSION,
        "energy_policy": FDC_ENERGY_MAPPING_POLICY_VERSION,
        "rights_status": "approved_for_v1",
        "license_basis": "public_domain + CC0_1_0",
        "candidate_cap": candidate_cap,
        "eligible_population_count": eligible_population_count,
        "candidate_count": candidates.len(),
        "candidate_selection_sha256": selection_fingerprint(selected_ids),
        "reviewer": FDC_FOUNDATION_V1_SELECTION_REVIEWER,
        "review_scope": "fdc-v1-selection",
        "review_status": "pending_human_review",
        "approval_reference": Value::Null,
        "reviewed_at": Value::Null,
        "production_eligible": false,
        "activation_attempted": false,
        "candidates": candidates
    })
}

pub(crate) struct SelectionCandidate {
    pub(crate) fdc_id: u64,
    pub(crate) description: String,
    pub(crate) energy_source_nutrient_id: u64,
    pub(crate) energy_method: &'static str,
}

pub(crate) fn candidate_from_food(food: &RawFood) -> Option<SelectionCandidate> {
    extract_unambiguous_macronutrients(food).ok()?;
    let energy = extract_energy(food).ok()?;
    if energy.unexpected_legacy_count != 0 {
        return None;
    }
    let selected = energy.selected?;
    let energy_method = selected.source_method?;
    if !matches!(selected.source_nutrient_id, 2048 | 2047) {
        return None;
    }
    Some(SelectionCandidate {
        fdc_id: food.fdc_id,
        description: food.description.clone(),
        energy_source_nutrient_id: selected.source_nutrient_id,
        energy_method,
    })
}

pub(crate) fn validate_reviewed_selection(
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
