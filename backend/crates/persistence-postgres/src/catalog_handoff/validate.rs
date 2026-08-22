use super::{
    load::sha256_hex,
    model::{
        CATALOG_HANDOFF_CONTRACT_VERSION, CATALOG_HANDOFF_PACKAGE_KIND, CATALOG_HANDOFF_PROFILE,
        CATALOG_HANDOFF_TEST_FIXTURE_PROFILE, CatalogHandoffImportError, FDC_HANDOFF_RELEASE,
        FDC_HANDOFF_SELECTED_IDS, FDC_HANDOFF_SELECTION_SHA256, FDC_HANDOFF_SOURCE_CODE,
        LoadedPackage, RawSourceRecord, TEST_FIXTURE_RELEASE, TEST_FIXTURE_SELECTION_VERSION,
        TEST_FIXTURE_SOURCE_CODE,
    },
};

struct ProfileSpec {
    source_code: &'static str,
    release: &'static str,
    published_date: &'static str,
    selection_version: &'static str,
    selection_sha256: &'static str,
    rights_state: &'static str,
    semantic_prefix: &'static str,
    selected_ids: &'static [u64],
}

fn profile_spec(profile: &str) -> Result<ProfileSpec, CatalogHandoffImportError> {
    match profile {
        CATALOG_HANDOFF_PROFILE => Ok(ProfileSpec {
            source_code: FDC_HANDOFF_SOURCE_CODE,
            release: FDC_HANDOFF_RELEASE,
            published_date: FDC_HANDOFF_RELEASE,
            selection_version: "backend-fdc-selection-0.1.0",
            selection_sha256: FDC_HANDOFF_SELECTION_SHA256,
            rights_state: "approved",
            semantic_prefix: "usda-fdc",
            selected_ids: &FDC_HANDOFF_SELECTED_IDS,
        }),
        CATALOG_HANDOFF_TEST_FIXTURE_PROFILE => Ok(ProfileSpec {
            source_code: TEST_FIXTURE_SOURCE_CODE,
            release: TEST_FIXTURE_RELEASE,
            published_date: "2000-01-01",
            selection_version: TEST_FIXTURE_SELECTION_VERSION,
            selection_sha256: FDC_HANDOFF_SELECTION_SHA256,
            rights_state: "test_only",
            semantic_prefix: "usda-fdc",
            selected_ids: &FDC_HANDOFF_SELECTED_IDS,
        }),
        other => Err(CatalogHandoffImportError::UnsupportedProfile(
            other.to_owned(),
        )),
    }
}
use std::{
    collections::{BTreeMap, BTreeSet},
    str::FromStr,
};

#[allow(clippy::too_many_lines)]
pub(crate) fn validate_package(package: &LoadedPackage) -> Result<(), CatalogHandoffImportError> {
    let manifest = &package.manifest;
    let profile = profile_spec(&manifest.handoff_profile)?;
    if manifest.package_kind != CATALOG_HANDOFF_PACKAGE_KIND {
        return Err(CatalogHandoffImportError::Semantic(format!(
            "package_kind must be {CATALOG_HANDOFF_PACKAGE_KIND}"
        )));
    }
    if manifest.backend_baseline.trim().is_empty()
        || manifest.policy_versions.get("handoff").map(String::as_str)
            != Some(CATALOG_HANDOFF_CONTRACT_VERSION)
        || manifest.selection.selection_version != profile.selection_version
        || manifest
            .policy_versions
            .get("nutrient_crosswalk")
            .map(String::as_str)
            != Some("fdc-nutrient-crosswalk-0.2.0")
        || manifest.source.published_date != profile.published_date
        || manifest.source.rights_state != profile.rights_state
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
    if manifest.dataset_release.dataset_code != profile.source_code
        || manifest.dataset_release.version != profile.release
        || manifest.source.source_code != profile.source_code
        || manifest.source.release != profile.release
    {
        return Err(CatalogHandoffImportError::Semantic(
            "manifest source or dataset release is not the supported FDC Foundation release"
                .to_owned(),
        ));
    }
    if manifest.selection.selection_sha256 != profile.selection_sha256
        || manifest.selection.record_count != profile.selected_ids.len()
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
        || package.dataset_release.artifact_sha256 != manifest.source.artifact_sha256
        || package.dataset_release.archive_sha256 != manifest.source.archive_sha256
        || package.dataset_release.object_uri.as_deref()
            != Some(manifest.source.object_uri.as_str())
        || package.dataset_release.source_rights_state != profile.rights_state
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
        || source.locator != manifest.source.object_uri
        || source.rights_state != package.dataset_release.source_rights_state
        || source.rights_state != profile.rights_state
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
    validate_artifact_acquisition(
        &source.extracted_artifact,
        &source.source_code,
        &source.release,
        &source.publisher,
        &source.rights_state,
    )?;
    validate_artifact_acquisition(
        &source.archive_artifact,
        &source.source_code,
        &source.release,
        &source.publisher,
        &source.rights_state,
    )?;
    validate_records(package, &profile)?;
    Ok(())
}

fn validate_records(
    package: &LoadedPackage,
    profile: &ProfileSpec,
) -> Result<(), CatalogHandoffImportError> {
    if package.raw_records.len() != profile.selected_ids.len() {
        return Err(CatalogHandoffImportError::Semantic(format!(
            "expected {} selected records, found {}",
            profile.selected_ids.len(),
            package.raw_records.len()
        )));
    }
    let approved: BTreeSet<String> = profile
        .selected_ids
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
        if record.source_code != profile.source_code || record.release != profile.release {
            return Err(CatalogHandoffImportError::ReferenceIntegrity(format!(
                "source record {} has unsupported source identity",
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
    if selected != profile.selected_ids {
        return Err(CatalogHandoffImportError::ReferenceIntegrity(
            "selected source IDs do not match the reviewed selection".to_owned(),
        ));
    }
    if selection_fingerprint(&selected) != profile.selection_sha256 {
        return Err(CatalogHandoffImportError::Semantic(
            "selected source IDs have the wrong fingerprint".to_owned(),
        ));
    }
    validate_concepts(package, &raw_by_id, profile)?;
    validate_names(package, &raw_by_id, profile)?;
    validate_mappings(package, &raw_by_id, profile)?;
    validate_compositions(package, &raw_by_id, profile)?;
    Ok(())
}

fn validate_concepts(
    package: &LoadedPackage,
    raw: &BTreeMap<u64, &RawSourceRecord>,
    profile: &ProfileSpec,
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
            || concept.concept_id != format!("source-food:{}:{id}", profile.source_code)
            || concept.semantic_key != format!("{}:{id}", profile.semantic_prefix)
            || concept.entity_kind != "basic_food"
            || concept.lifecycle_status != "candidate"
            || concept.source_code != profile.source_code
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
    profile: &ProfileSpec,
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
            || name.concept_id != format!("source-food:{}:{id}", profile.source_code)
            || name.name != source.description
            || name.normalized_name != name.name.to_lowercase()
            || name.source_code != profile.source_code
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
    profile: &ProfileSpec,
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
            || mapping.mapping_id != format!("source-mapping:{}:{id}", profile.source_code)
            || mapping.concept_id != format!("source-food:{}:{id}", profile.source_code)
            || mapping.source_code != profile.source_code
            || mapping.release != profile.release
            || mapping.source_payload_sha256 != source.payload_sha256
            || mapping.mapping_type != "exact"
            || mapping.mapping_method != "fdc_exact_external_id"
            || (mapping.score - 1.0).abs() > f64::EPSILON
            || mapping.policy_version != "catalog-handoff-exact-source-id-0.1.0"
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

#[allow(clippy::too_many_lines)]
fn validate_compositions(
    package: &LoadedPackage,
    raw: &BTreeMap<u64, &RawSourceRecord>,
    profile: &ProfileSpec,
) -> Result<(), CatalogHandoffImportError> {
    let mut identities = BTreeSet::new();
    let mut nutrients_by_source: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let expected_policy = "fdc-nutrient-crosswalk-0.2.0";
    let expected_targets = BTreeSet::from([
        "energy_kcal".to_owned(),
        "protein_g".to_owned(),
        "fat_g".to_owned(),
        "carbohydrate_g".to_owned(),
    ]);
    for value in &package.compositions {
        let id = parse_source_id(&value.source_id)?;
        let source = raw.get(&id).ok_or_else(|| {
            CatalogHandoffImportError::ReferenceIntegrity(format!(
                "composition references unknown source {}",
                value.source_id
            ))
        })?;
        if !identities.insert((value.source_id.clone(), value.target_code.clone()))
            || value.value_id
                != format!(
                    "composition:{}:{}:{}",
                    profile.source_code, value.source_id, value.target_code
                )
            || value.source_code != profile.source_code
            || value.release != profile.release
            || value.source_label.trim().is_empty()
            || value.source_payload_sha256 != source.payload_sha256
            || !matches!(
                value.target_code.as_str(),
                "energy_kcal" | "protein_g" | "fat_g" | "carbohydrate_g"
            )
            || value.conversion != "identity"
            || value.review_status != "proposal"
            || value.reviewer_decision_status != "pending_human_review"
            || value.policy_version != expected_policy
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
        let valid_value_state = match value.value_status.as_str() {
            "numeric" => value
                .value
                .is_some_and(|amount| amount.is_finite() && amount > 0.0),
            "zero" => value.value == Some(0.0),
            "missing" => value.value.is_none(),
            _ => false,
        };
        if value.source_unit != expected_unit.0
            || value.canonical_unit != expected_unit.1
            || !valid_value_state
            || value
                .value
                .is_some_and(|amount| !amount.is_finite() || amount < 0.0)
            || value.source_nutrient_id == 0
            || !expected_source_nutrients(&value.target_code).contains(&value.source_nutrient_id)
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
        let observation = raw_nutrient_observation(source, value.source_nutrient_id)?;
        let nutrient = observation.get("nutrient").ok_or_else(|| {
            CatalogHandoffImportError::ReferenceIntegrity(format!(
                "composition {} has no raw nutrient object",
                value.value_id
            ))
        })?;
        let raw_unit = nutrient.get("unitName").and_then(serde_json::Value::as_str);
        let raw_amount = observation
            .get("amount")
            .and_then(serde_json::Value::as_f64);
        if raw_unit != Some(value.source_unit.as_str()) || raw_amount != value.value {
            return Err(CatalogHandoffImportError::ReferenceIntegrity(format!(
                "composition {} is not grounded in raw nutrient evidence",
                value.value_id
            )));
        }
        let raw_label = nutrient.get("name").and_then(serde_json::Value::as_str);
        if raw_label.is_some() && raw_label != Some(value.source_label.as_str()) {
            return Err(CatalogHandoffImportError::ReferenceIntegrity(format!(
                "composition {} has a non-grounded nutrient label",
                value.value_id
            )));
        }
        if value.source_method != expected_source_method(observation, value.source_nutrient_id) {
            return Err(CatalogHandoffImportError::ReferenceIntegrity(format!(
                "composition {} has a non-grounded source method",
                value.value_id
            )));
        }
        nutrients_by_source
            .entry(value.source_id.clone())
            .or_default()
            .insert(value.target_code.clone());
    }
    if package.compositions.len() != package.raw_records.len() * expected_targets.len()
        || package
            .compositions
            .iter()
            .any(|value| value.value_status == "missing")
        || nutrients_by_source
            .iter()
            .any(|(_, targets)| targets != &expected_targets)
    {
        return Err(CatalogHandoffImportError::ReferenceIntegrity(
            "reviewed profile requires four grounded core nutrients for every selected record"
                .to_owned(),
        ));
    }
    Ok(())
}

fn expected_source_nutrients(target_code: &str) -> BTreeSet<u64> {
    match target_code {
        "protein_g" => BTreeSet::from([1003]),
        "fat_g" => BTreeSet::from([1004]),
        "carbohydrate_g" => BTreeSet::from([1005]),
        "energy_kcal" => BTreeSet::from([2047, 2048]),
        _ => BTreeSet::new(),
    }
}

fn expected_source_method(observation: &serde_json::Value, source_nutrient_id: u64) -> String {
    if let Some(derivation) = observation.get("foodNutrientDerivation")
        && let Some(method) = derivation
            .get("code")
            .and_then(serde_json::Value::as_str)
            .filter(|method| !method.trim().is_empty())
            .or_else(|| {
                derivation
                    .get("description")
                    .and_then(serde_json::Value::as_str)
                    .filter(|method| !method.trim().is_empty())
            })
    {
        return method.trim().to_owned();
    }
    match source_nutrient_id {
        2047 => "atwater_general".to_owned(),
        2048 => "atwater_specific".to_owned(),
        1003..=1005 => "declared_or_analytical".to_owned(),
        _ => String::new(),
    }
}

fn raw_nutrient_observation(
    source: &RawSourceRecord,
    source_nutrient_id: u64,
) -> Result<&serde_json::Value, CatalogHandoffImportError> {
    let observations = source
        .payload
        .get("foodNutrients")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            CatalogHandoffImportError::ReferenceIntegrity(format!(
                "source record {} has no foodNutrients evidence",
                source.source_id
            ))
        })?;
    let matches: Vec<&serde_json::Value> = observations
        .iter()
        .filter(|item| {
            item.get("nutrient")
                .and_then(|nutrient| nutrient.get("id"))
                .and_then(serde_json::Value::as_u64)
                == Some(source_nutrient_id)
        })
        .collect();
    if matches.len() != 1 {
        return Err(CatalogHandoffImportError::ReferenceIntegrity(format!(
            "source record {} has {} observations for nutrient {}",
            source.source_id,
            matches.len(),
            source_nutrient_id
        )));
    }
    Ok(matches[0])
}

fn validate_artifact_acquisition(
    artifact: &super::model::Artifact,
    source_code: &str,
    release: &str,
    publisher: &str,
    rights_state: &str,
) -> Result<(), CatalogHandoffImportError> {
    let Some(acquisition) = artifact.acquisition.as_ref() else {
        return Ok(());
    };
    let matches = acquisition
        .get("source_code")
        .and_then(serde_json::Value::as_str)
        == Some(source_code)
        && acquisition
            .get("publisher")
            .and_then(serde_json::Value::as_str)
            == Some(publisher)
        && acquisition
            .get("release")
            .and_then(serde_json::Value::as_str)
            == Some(release)
        && acquisition
            .get("rights_state")
            .and_then(serde_json::Value::as_str)
            == Some(rights_state)
        && acquisition
            .get("sha256")
            .and_then(serde_json::Value::as_str)
            == Some(artifact.sha256.as_str())
        && acquisition.get("size").and_then(serde_json::Value::as_u64) == Some(artifact.size)
        && acquisition
            .get("content_type")
            .and_then(serde_json::Value::as_str)
            == Some(artifact.content_type.as_str())
        && acquisition
            .get("filename")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| !value.is_empty())
        && acquisition
            .get("retrieved_at")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| !value.is_empty())
        && acquisition
            .get("acquisition_tool_version")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| !value.is_empty());
    if !matches {
        return Err(CatalogHandoffImportError::Semantic(format!(
            "artifact acquisition metadata disagrees with {}",
            artifact.relative_path
        )));
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
