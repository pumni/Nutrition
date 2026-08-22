use super::{
    load::sha256_hex,
    model::{
        CATALOG_HANDOFF_CONTRACT_VERSION, CATALOG_HANDOFF_PACKAGE_KIND, CatalogHandoffImportError,
        FDC_HANDOFF_RELEASE, FDC_HANDOFF_SELECTED_IDS, FDC_HANDOFF_SELECTION_SHA256, LoadedPackage,
        RawSourceRecord,
    },
};
use std::{
    collections::{BTreeMap, BTreeSet},
    str::FromStr,
};

pub(crate) fn validate_package(package: &LoadedPackage) -> Result<(), CatalogHandoffImportError> {
    let manifest = &package.manifest;
    if manifest.package_kind != CATALOG_HANDOFF_PACKAGE_KIND {
        return Err(CatalogHandoffImportError::Semantic(format!(
            "package_kind must be {CATALOG_HANDOFF_PACKAGE_KIND}"
        )));
    }
    if manifest.backend_baseline.trim().is_empty()
        || manifest.policy_versions.get("handoff").map(String::as_str)
            != Some(CATALOG_HANDOFF_CONTRACT_VERSION)
        || manifest.selection.selection_version != "backend-fdc-selection-0.1.0"
        || manifest.source.published_date != FDC_HANDOFF_RELEASE
        || manifest.source.object_uri.trim().is_empty()
    {
        return Err(CatalogHandoffImportError::Semantic(
            "manifest provenance or policy versions are incomplete".to_owned(),
        ));
    }
    if manifest.import_mode != "staged_only"
        || manifest.production_eligible
        || manifest.activation_authorized
    {
        return Err(CatalogHandoffImportError::Semantic(
            "catalog handoff packages must be staged_only with both production flags false"
                .to_owned(),
        ));
    }
    if manifest.dataset_release.dataset_code != "usda_fdc"
        || manifest.dataset_release.version != FDC_HANDOFF_RELEASE
        || manifest.source.source_code != "usda_fdc"
        || manifest.source.release != FDC_HANDOFF_RELEASE
    {
        return Err(CatalogHandoffImportError::Semantic(
            "manifest source or dataset release is not the supported FDC Foundation release"
                .to_owned(),
        ));
    }
    if manifest.selection.selection_sha256 != FDC_HANDOFF_SELECTION_SHA256
        || manifest.selection.record_count != FDC_HANDOFF_SELECTED_IDS.len()
        || manifest.selection.auto_add_records
    {
        return Err(CatalogHandoffImportError::Semantic(
            "selection does not match the reviewed 20-record allowlist".to_owned(),
        ));
    }
    if package.dataset_release.dataset_code != manifest.dataset_release.dataset_code
        || package.dataset_release.version != manifest.dataset_release.version
        || package.dataset_release.record_count != manifest.selection.record_count
        || package.dataset_release.status != "staged_candidate"
        || package.dataset_release.production_eligible
    {
        return Err(CatalogHandoffImportError::Semantic(
            "dataset release metadata disagrees with the manifest".to_owned(),
        ));
    }
    if package.source_releases.sources.len() != 1 {
        return Err(CatalogHandoffImportError::ReferenceIntegrity(
            "exactly one source release is required".to_owned(),
        ));
    }
    let source = &package.source_releases.sources[0];
    if source.source_code != manifest.source.source_code
        || source.release != manifest.source.release
        || source.production_eligible
        || source.extracted_artifact.sha256 != manifest.source.artifact_sha256
        || source.archive_artifact.sha256 != manifest.source.archive_sha256
        || source.purpose.trim().is_empty()
        || source.rights_state.trim().is_empty()
        || source.extracted_artifact.size == 0
        || source.archive_artifact.size == 0
        || source.extracted_artifact.content_type.trim().is_empty()
        || source.archive_artifact.content_type.trim().is_empty()
        || source.extracted_artifact.relative_path.trim().is_empty()
        || source.archive_artifact.relative_path.trim().is_empty()
        || source
            .extracted_artifact
            .acquisition
            .as_ref()
            .is_some_and(|value| !value.is_object())
        || source
            .archive_artifact
            .acquisition
            .as_ref()
            .is_some_and(|value| !value.is_object())
    {
        return Err(CatalogHandoffImportError::Semantic(
            "source release provenance disagrees with the manifest".to_owned(),
        ));
    }
    validate_records(package)?;
    Ok(())
}

fn validate_records(package: &LoadedPackage) -> Result<(), CatalogHandoffImportError> {
    if package.raw_records.len() != FDC_HANDOFF_SELECTED_IDS.len() {
        return Err(CatalogHandoffImportError::Semantic(format!(
            "expected {} selected records, found {}",
            FDC_HANDOFF_SELECTED_IDS.len(),
            package.raw_records.len()
        )));
    }
    let approved: BTreeSet<String> = FDC_HANDOFF_SELECTED_IDS
        .iter()
        .map(ToString::to_string)
        .collect();
    let mut raw_keys = BTreeSet::new();
    let mut raw_by_id = BTreeMap::new();
    for record in &package.raw_records {
        let id = parse_source_id(&record.source_id)?;
        if !approved.contains(&record.source_id) {
            return Err(CatalogHandoffImportError::ReferenceIntegrity(format!(
                "source ID {} is outside the reviewed selection",
                record.source_id
            )));
        }
        let key = (
            record.source_code.clone(),
            record.release.clone(),
            record.source_id.clone(),
        );
        if !raw_keys.insert(key) {
            return Err(CatalogHandoffImportError::ReferenceIntegrity(format!(
                "duplicate source record {}",
                record.source_id
            )));
        }
        let payload_hash = serde_json::to_vec(&record.payload)
            .map(|bytes| sha256_hex(&bytes))
            .map_err(|error| CatalogHandoffImportError::Schema(error.to_string()))?;
        if record
            .payload
            .get("fdcId")
            .and_then(serde_json::Value::as_u64)
            != Some(id)
            || record
                .payload
                .get("dataType")
                .and_then(serde_json::Value::as_str)
                != Some("Foundation")
            || !record.payload.is_object()
            || record.payload_sha256.len() != 64
            || record.payload_sha256 != record.payload_sha256.to_ascii_lowercase()
            || record.payload_sha256 != payload_hash
        {
            return Err(CatalogHandoffImportError::Semantic(format!(
                "invalid source payload provenance for {}",
                record.source_id
            )));
        }
        raw_by_id.insert(id, record);
    }
    let selected: Vec<u64> = raw_by_id.keys().copied().collect();
    if selected != FDC_HANDOFF_SELECTED_IDS {
        return Err(CatalogHandoffImportError::ReferenceIntegrity(
            "selected source IDs do not match the reviewed selection".to_owned(),
        ));
    }
    if selection_fingerprint(&selected) != FDC_HANDOFF_SELECTION_SHA256 {
        return Err(CatalogHandoffImportError::Semantic(
            "selected source IDs have the wrong fingerprint".to_owned(),
        ));
    }
    validate_concepts(package, &raw_by_id)?;
    validate_names(package, &raw_by_id)?;
    validate_mappings(package, &raw_by_id)?;
    validate_compositions(package, &raw_by_id)?;
    Ok(())
}

fn validate_concepts(
    package: &LoadedPackage,
    raw: &BTreeMap<u64, &RawSourceRecord>,
) -> Result<(), CatalogHandoffImportError> {
    let mut ids = BTreeSet::new();
    if package.food_concepts.len() != raw.len() {
        return Err(CatalogHandoffImportError::ReferenceIntegrity(
            "every selected source record must have exactly one food concept".to_owned(),
        ));
    }
    for concept in &package.food_concepts {
        let id = parse_source_id(&concept.source_id)?;
        let source = raw.get(&id).ok_or_else(|| {
            CatalogHandoffImportError::ReferenceIntegrity(format!(
                "food concept references unknown source {}",
                concept.source_id
            ))
        })?;
        if !ids.insert(concept.concept_id.clone())
            || concept.concept_id != format!("source-food:usda_fdc:{id}")
            || concept.semantic_key != format!("usda-fdc:{id}")
            || concept.entity_kind != "basic_food"
            || concept.lifecycle_status != "candidate"
            || concept.source_code != "usda_fdc"
            || concept.source_payload_sha256 != source.payload_sha256
            || concept.review_status != "proposal"
        {
            return Err(CatalogHandoffImportError::ReferenceIntegrity(format!(
                "invalid or duplicate food concept {}",
                concept.concept_id
            )));
        }
    }
    Ok(())
}

fn validate_names(
    package: &LoadedPackage,
    raw: &BTreeMap<u64, &RawSourceRecord>,
) -> Result<(), CatalogHandoffImportError> {
    let concepts: BTreeSet<&str> = package
        .food_concepts
        .iter()
        .map(|item| item.concept_id.as_str())
        .collect();
    let mut ids = BTreeSet::new();
    if package.food_names.len() != raw.len() {
        return Err(CatalogHandoffImportError::ReferenceIntegrity(
            "every selected source record must have exactly one source-backed name".to_owned(),
        ));
    }
    for name in &package.food_names {
        let id = parse_source_id(&name.source_id)?;
        let source = raw.get(&id).ok_or_else(|| {
            CatalogHandoffImportError::ReferenceIntegrity(format!(
                "food name references unknown source {}",
                name.source_id
            ))
        })?;
        if !ids.insert(name.name_id.clone())
            || !concepts.contains(name.concept_id.as_str())
            || name.concept_id != format!("source-food:usda_fdc:{id}")
            || name.source_code != "usda_fdc"
            || name.locale != "en-US"
            || name.name_type != "preferred"
            || name.status != "source_observed"
            || name.source_payload_sha256 != source.payload_sha256
            || name.name.trim().is_empty()
            || name.normalized_name.trim().is_empty()
        {
            return Err(CatalogHandoffImportError::ReferenceIntegrity(format!(
                "invalid or duplicate food name {}",
                name.name_id
            )));
        }
    }
    Ok(())
}

fn validate_mappings(
    package: &LoadedPackage,
    raw: &BTreeMap<u64, &RawSourceRecord>,
) -> Result<(), CatalogHandoffImportError> {
    let concepts: BTreeSet<&str> = package
        .food_concepts
        .iter()
        .map(|item| item.concept_id.as_str())
        .collect();
    let mut source_ids = BTreeSet::new();
    if package.mappings.len() != raw.len() {
        return Err(CatalogHandoffImportError::ReferenceIntegrity(
            "every selected source record must have exactly one mapping".to_owned(),
        ));
    }
    for mapping in &package.mappings {
        let id = parse_source_id(&mapping.source_id)?;
        let source = raw.get(&id).ok_or_else(|| {
            CatalogHandoffImportError::ReferenceIntegrity(format!(
                "mapping references unknown source {}",
                mapping.source_id
            ))
        })?;
        if !source_ids.insert(mapping.source_id.clone())
            || !concepts.contains(mapping.concept_id.as_str())
            || mapping.concept_id != format!("source-food:usda_fdc:{id}")
            || mapping.source_code != "usda_fdc"
            || mapping.release != FDC_HANDOFF_RELEASE
            || mapping.source_payload_sha256 != source.payload_sha256
            || mapping.mapping_type != "exact"
            || mapping.mapping_method != "fdc_exact_external_id"
            || (mapping.score - 1.0).abs() > f64::EPSILON
            || mapping.review_status != "proposal"
        {
            return Err(CatalogHandoffImportError::ReferenceIntegrity(format!(
                "invalid or duplicate mapping {}",
                mapping.mapping_id
            )));
        }
    }
    Ok(())
}

fn validate_compositions(
    package: &LoadedPackage,
    raw: &BTreeMap<u64, &RawSourceRecord>,
) -> Result<(), CatalogHandoffImportError> {
    let mut identities = BTreeSet::new();
    for value in &package.compositions {
        let id = parse_source_id(&value.source_id)?;
        let source = raw.get(&id).ok_or_else(|| {
            CatalogHandoffImportError::ReferenceIntegrity(format!(
                "composition references unknown source {}",
                value.source_id
            ))
        })?;
        if !identities.insert((value.source_id.clone(), value.target_code.clone()))
            || value.source_code != "usda_fdc"
            || value.release != FDC_HANDOFF_RELEASE
            || value.source_label.trim().is_empty()
            || value.source_payload_sha256 != source.payload_sha256
            || !matches!(
                value.target_code.as_str(),
                "energy_kcal" | "protein_g" | "fat_g" | "carbohydrate_g"
            )
            || value.conversion != "identity"
            || value.review_status != "proposal"
            || value.reviewer_decision_status != "pending_human_review"
        {
            return Err(CatalogHandoffImportError::Semantic(format!(
                "invalid or duplicate composition value {}",
                value.value_id
            )));
        }
        let expected_unit = if value.target_code == "energy_kcal" {
            ("KCAL", "kcal")
        } else {
            ("G", "g")
        };
        if value.source_unit != expected_unit.0
            || value.canonical_unit != expected_unit.1
            || value
                .value
                .is_some_and(|amount| !amount.is_finite() || amount < 0.0)
            || value.source_nutrient_id == 0
        {
            return Err(CatalogHandoffImportError::Semantic(format!(
                "unsupported unit or amount for {}",
                value.value_id
            )));
        }
        if value.value_status == "missing" && value.value.is_some() {
            return Err(CatalogHandoffImportError::Semantic(format!(
                "missing composition value {} must have a null amount",
                value.value_id
            )));
        }
    }
    Ok(())
}

fn parse_source_id(value: &str) -> Result<u64, CatalogHandoffImportError> {
    let parsed = u64::from_str(value).map_err(|_| {
        CatalogHandoffImportError::ReferenceIntegrity(format!(
            "source ID {value} is not a positive integer"
        ))
    })?;
    if parsed == 0 {
        return Err(CatalogHandoffImportError::ReferenceIntegrity(format!(
            "source ID {value} is zero"
        )));
    }
    Ok(parsed)
}

fn selection_fingerprint(ids: &[u64]) -> String {
    let joined = ids
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",");
    sha256_hex(joined.as_bytes())
}
