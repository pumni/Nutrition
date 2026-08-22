use super::{
    load::sha256_hex,
    model::{
        CATALOG_HANDOFF_CONTRACT_VERSION, CatalogHandoffImportError, CatalogHandoffImportReport,
        CatalogHandoffImportRequest, FoodConcept, FoodName, LoadedPackage, RawSourceRecord,
        SourceFoodMapping,
    },
};
use serde_json::json;
use sqlx::{Postgres, Transaction};
use std::{collections::BTreeMap, str::FromStr};
use uuid::Uuid;

pub(crate) async fn stage_package(
    tx: &mut Transaction<'_, Postgres>,
    request: &CatalogHandoffImportRequest,
    package: &LoadedPackage,
) -> Result<CatalogHandoffImportReport, CatalogHandoffImportError> {
    let created_by = Uuid::from_str(&request.created_by).map_err(|_| {
        CatalogHandoffImportError::InvalidInput("created_by must be a UUID".to_owned())
    })?;
    let dataset_id = ensure_dataset(tx, package).await?;
    let dataset_release_id = ensure_dataset_release(tx, dataset_id, package).await?;
    store_raw_records(tx, dataset_release_id, package).await?;
    mark_dataset_release_imported(tx, dataset_release_id).await?;

    let manifest_checksum = sha256_hex(&package.manifest_bytes);
    if let Some((catalog_release_id, existing_checksum)) =
        existing_catalog_release(tx, &package.manifest.catalog_release_version).await?
    {
        if existing_checksum != manifest_checksum {
            return Err(CatalogHandoffImportError::ReleaseConflict(format!(
                "catalog release {} already has manifest checksum {}, not {}",
                package.manifest.catalog_release_version, existing_checksum, manifest_checksum
            )));
        }
        return Ok(report(
            package,
            dataset_release_id,
            catalog_release_id,
            true,
        ));
    }

    ensure_core_nutrients(tx).await?;
    let catalog_release_id =
        create_catalog_release(tx, created_by, package, manifest_checksum).await?;
    stage_selection(tx, dataset_release_id, catalog_release_id, package).await?;
    Ok(report(
        package,
        dataset_release_id,
        catalog_release_id,
        false,
    ))
}

fn report(
    package: &LoadedPackage,
    dataset_release_id: Uuid,
    catalog_release_id: Uuid,
    replayed: bool,
) -> CatalogHandoffImportReport {
    CatalogHandoffImportReport {
        dataset_release_id,
        catalog_release_id,
        catalog_release_version: package.manifest.catalog_release_version.clone(),
        contract_version: package.manifest.contract_version.clone(),
        package_sha256: package.package_sha256.clone(),
        selected_record_count: package.raw_records.len(),
        composition_value_count: package.compositions.len(),
        replayed,
    }
}

async fn ensure_dataset(
    tx: &mut Transaction<'_, Postgres>,
    package: &LoadedPackage,
) -> Result<Uuid, CatalogHandoffImportError> {
    let source = &package.source_releases.sources[0];
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO raw.dataset
            (id, code, name, publisher, license_code, homepage, ingestion_policy_version)
         VALUES ($1, $2, 'USDA FoodData Central', $3, NULL, $4, $5)
         ON CONFLICT (code) DO NOTHING",
    )
    .bind(id)
    .bind(&source.source_code)
    .bind(&source.publisher)
    .bind(&source.locator)
    .bind(CATALOG_HANDOFF_CONTRACT_VERSION)
    .execute(&mut **tx)
    .await?;
    sqlx::query_scalar("SELECT id FROM raw.dataset WHERE code = $1")
        .bind(&source.source_code)
        .fetch_one(&mut **tx)
        .await
        .map_err(CatalogHandoffImportError::Query)
}

async fn ensure_dataset_release(
    tx: &mut Transaction<'_, Postgres>,
    dataset_id: Uuid,
    package: &LoadedPackage,
) -> Result<Uuid, CatalogHandoffImportError> {
    let existing = sqlx::query_as::<_, (Uuid, String, String)>(
        "SELECT id, checksum_sha256, schema_fingerprint
           FROM raw.dataset_release
          WHERE dataset_id = $1 AND version = $2",
    )
    .bind(dataset_id)
    .bind(&package.dataset_release.version)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some((id, checksum, schema)) = existing {
        if checksum != package.dataset_release.artifact_sha256 {
            return Err(CatalogHandoffImportError::ReleaseConflict(format!(
                "dataset release {} already has artifact checksum {}, not {}",
                package.dataset_release.version, checksum, package.dataset_release.artifact_sha256
            )));
        }
        // A legacy raw importer may have recorded the same source artifact with its full-source
        // count or schema fingerprint. The content hash remains the authoritative source identity;
        // the handoff stages its reviewed subset into that immutable release.
        let _ = schema;
        return Ok(id);
    }
    let id = Uuid::now_v7();
    let source = &package.source_releases.sources[0];
    let published_at = format!("{}T00:00:00Z", package.dataset_release.version);
    let metadata = json!({
        "contract_version": package.manifest.contract_version,
        "package_id": package.manifest.package_id,
        "package_sha256": package.package_sha256,
        "handoff_profile": package.manifest.handoff_profile,
        "source_rights_state": package.dataset_release.source_rights_state,
        "archive_sha256": package.dataset_release.archive_sha256,
        "producer": package.manifest.producer,
        "producer_version": package.manifest.producer_version
    });
    let count = i64::try_from(package.dataset_release.record_count).map_err(|_| {
        CatalogHandoffImportError::InvalidInput("record count exceeds PostgreSQL bigint".to_owned())
    })?;
    sqlx::query(
        "INSERT INTO raw.dataset_release
            (id, dataset_id, version, published_at, object_uri, checksum_sha256,
             schema_fingerprint, record_count, metadata, status)
         VALUES ($1, $2, $3, $4::timestamptz, $5, $6, $7, $8, $9, 'validated')",
    )
    .bind(id)
    .bind(dataset_id)
    .bind(&package.dataset_release.version)
    .bind(published_at)
    .bind(
        package
            .dataset_release
            .object_uri
            .as_deref()
            .unwrap_or(&source.locator),
    )
    .bind(&package.dataset_release.artifact_sha256)
    .bind(&package.dataset_release.schema_fingerprint)
    .bind(count)
    .bind(metadata)
    .execute(&mut **tx)
    .await?;
    Ok(id)
}

async fn store_raw_records(
    tx: &mut Transaction<'_, Postgres>,
    dataset_release_id: Uuid,
    package: &LoadedPackage,
) -> Result<(), CatalogHandoffImportError> {
    for record in &package.raw_records {
        sqlx::query(
            "INSERT INTO raw.source_food_record
                (id, dataset_release_id, external_id, source_data_type, source_description,
                 normalized_search_text, raw_payload, payload_hash)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (dataset_release_id, external_id) DO NOTHING",
        )
        .bind(Uuid::now_v7())
        .bind(dataset_release_id)
        .bind(&record.source_id)
        .bind(&record.data_type)
        .bind(&record.description)
        .bind(record.description.to_lowercase())
        .bind(&record.payload)
        .bind(&record.payload_sha256)
        .execute(&mut **tx)
        .await?;
        let stored_hash: String = sqlx::query_scalar(
            "SELECT payload_hash FROM raw.source_food_record WHERE dataset_release_id = $1 AND external_id = $2",
        )
        .bind(dataset_release_id)
        .bind(&record.source_id)
        .fetch_one(&mut **tx)
        .await?;
        if stored_hash != record.payload_sha256 {
            return Err(CatalogHandoffImportError::ReleaseConflict(format!(
                "source record {} has a different payload hash",
                record.source_id
            )));
        }
    }
    Ok(())
}

async fn mark_dataset_release_imported(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
) -> Result<(), CatalogHandoffImportError> {
    sqlx::query(
        "UPDATE raw.dataset_release SET status = 'imported', imported_at = COALESCE(imported_at, now())
          WHERE id = $1 AND status IN ('received', 'validated', 'imported')",
    )
    .bind(id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn existing_catalog_release(
    tx: &mut Transaction<'_, Postgres>,
    version: &str,
) -> Result<Option<(Uuid, String)>, CatalogHandoffImportError> {
    sqlx::query_as::<_, (Uuid, String)>(
        "SELECT id, checksum_sha256 FROM catalog.catalog_release WHERE version = $1",
    )
    .bind(version)
    .fetch_optional(&mut **tx)
    .await
    .map_err(CatalogHandoffImportError::Query)
}

async fn create_catalog_release(
    tx: &mut Transaction<'_, Postgres>,
    created_by: Uuid,
    package: &LoadedPackage,
    checksum: String,
) -> Result<Uuid, CatalogHandoffImportError> {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO catalog.catalog_release (id, version, status, manifest, checksum_sha256, created_by)
         VALUES ($1, $2, 'staged', $3, $4, $5)",
    )
    .bind(id)
    .bind(&package.manifest.catalog_release_version)
    .bind(&package.manifest_value)
    .bind(checksum)
    .bind(created_by)
    .execute(&mut **tx)
    .await?;
    Ok(id)
}

async fn ensure_core_nutrients(
    tx: &mut Transaction<'_, Postgres>,
) -> Result<(), CatalogHandoffImportError> {
    for (code, name, unit, group, energy) in [
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
                (id, code, preferred_name, canonical_unit, nutrient_group, external_identifiers, is_energy_component)
             VALUES ($1, $2, $3, $4, $5, '{}'::jsonb, $6) ON CONFLICT (code) DO NOTHING",
        )
        .bind(Uuid::now_v7())
        .bind(code)
        .bind(name)
        .bind(unit)
        .bind(group)
        .bind(energy)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn stage_selection(
    tx: &mut Transaction<'_, Postgres>,
    dataset_release_id: Uuid,
    catalog_release_id: Uuid,
    package: &LoadedPackage,
) -> Result<(), CatalogHandoffImportError> {
    let concepts: BTreeMap<&str, &FoodConcept> = package
        .food_concepts
        .iter()
        .map(|item| (item.source_id.as_str(), item))
        .collect();
    let names: BTreeMap<&str, &FoodName> = package
        .food_names
        .iter()
        .map(|item| (item.source_id.as_str(), item))
        .collect();
    let mappings: BTreeMap<&str, &SourceFoodMapping> = package
        .mappings
        .iter()
        .map(|item| (item.source_id.as_str(), item))
        .collect();
    for record in &package.raw_records {
        let source_record_id = source_record_id(tx, dataset_release_id, &record.source_id).await?;
        let concept = concepts[record.source_id.as_str()];
        let name = names[record.source_id.as_str()];
        let mapping = mappings[record.source_id.as_str()];
        let food_id = ensure_food_entity(tx, &concept.semantic_key).await?;
        ensure_food_mapping(tx, source_record_id, food_id, mapping).await?;
        let food_name_id = ensure_food_name(tx, source_record_id, food_id, name).await?;
        add_name_to_release(tx, catalog_release_id, food_name_id).await?;
        let profile_id = ensure_profile(tx, source_record_id, food_id, record, package).await?;
        add_profile_to_release(tx, catalog_release_id, profile_id).await?;
    }
    Ok(())
}

async fn source_record_id(
    tx: &mut Transaction<'_, Postgres>,
    dataset_release_id: Uuid,
    source_id: &str,
) -> Result<Uuid, CatalogHandoffImportError> {
    sqlx::query_scalar(
        "SELECT id FROM raw.source_food_record WHERE dataset_release_id = $1 AND external_id = $2",
    )
    .bind(dataset_release_id)
    .bind(source_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(CatalogHandoffImportError::Query)
}

async fn ensure_food_entity(
    tx: &mut Transaction<'_, Postgres>,
    semantic_key: &str,
) -> Result<Uuid, CatalogHandoffImportError> {
    sqlx::query("INSERT INTO catalog.food_entity (id, semantic_key, entity_kind, lifecycle_status) VALUES ($1, $2, 'basic_food', 'draft') ON CONFLICT (semantic_key) DO NOTHING")
        .bind(Uuid::now_v7()).bind(semantic_key).execute(&mut **tx).await?;
    sqlx::query_scalar("SELECT id FROM catalog.food_entity WHERE semantic_key = $1")
        .bind(semantic_key)
        .fetch_one(&mut **tx)
        .await
        .map_err(CatalogHandoffImportError::Query)
}

async fn ensure_food_mapping(
    tx: &mut Transaction<'_, Postgres>,
    source_record_id: Uuid,
    food_id: Uuid,
    mapping: &SourceFoodMapping,
) -> Result<(), CatalogHandoffImportError> {
    sqlx::query(
        "INSERT INTO catalog.food_mapping
            (id, source_food_record_id, food_id, mapping_type, mapping_method, score, policy_version, review_status, rationale)
         SELECT $1, $2, $3, 'exact', $4, $5, $6, 'proposed', $7
         WHERE NOT EXISTS (SELECT 1 FROM catalog.food_mapping WHERE source_food_record_id = $2 AND food_id = $3)",
    )
    .bind(Uuid::now_v7()).bind(source_record_id).bind(food_id).bind(&mapping.mapping_method).bind(mapping.score).bind(&mapping.policy_version).bind(&mapping.rationale)
    .execute(&mut **tx).await?;
    Ok(())
}

async fn ensure_food_name(
    tx: &mut Transaction<'_, Postgres>,
    source_record_id: Uuid,
    food_id: Uuid,
    name: &FoodName,
) -> Result<Uuid, CatalogHandoffImportError> {
    if let Some(id) = sqlx::query_scalar::<_, Uuid>("SELECT id FROM catalog.food_name WHERE food_id = $1 AND source_record_id = $2 AND locale = $3 AND name = $4 LIMIT 1")
        .bind(food_id).bind(source_record_id).bind(&name.locale).bind(&name.name).fetch_optional(&mut **tx).await? { return Ok(id); }
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO catalog.food_name (id, food_id, locale, name, normalized_name, name_type, source_record_id, is_curated, search_weight) VALUES ($1, $2, $3, $4, $5, 'preferred', $6, false, 0)")
        .bind(id).bind(food_id).bind(&name.locale).bind(&name.name).bind(&name.normalized_name).bind(source_record_id).execute(&mut **tx).await?;
    Ok(id)
}

async fn add_name_to_release(
    tx: &mut Transaction<'_, Postgres>,
    release_id: Uuid,
    name_id: Uuid,
) -> Result<(), CatalogHandoffImportError> {
    sqlx::query("INSERT INTO catalog.catalog_release_food_name (catalog_release_id, food_name_id) VALUES ($1, $2) ON CONFLICT DO NOTHING").bind(release_id).bind(name_id).execute(&mut **tx).await?;
    Ok(())
}

async fn ensure_profile(
    tx: &mut Transaction<'_, Postgres>,
    source_record_id: Uuid,
    food_id: Uuid,
    record: &RawSourceRecord,
    package: &LoadedPackage,
) -> Result<Uuid, CatalogHandoffImportError> {
    if let Some(id) = sqlx::query_scalar::<_, Uuid>("SELECT id FROM composition.composition_profile WHERE food_id = $1 AND source_record_id = $2 AND basis_amount = 100 AND basis_unit = 'g' AND edible_basis AND method_metadata->>'contract_version' = $3 LIMIT 1")
        .bind(food_id).bind(source_record_id).bind(CATALOG_HANDOFF_CONTRACT_VERSION).fetch_optional(&mut **tx).await? { return Ok(id); }
    let profile_id = Uuid::now_v7();
    let metadata = json!({
        "contract_version": package.manifest.contract_version,
        "package_id": package.manifest.package_id,
        "package_sha256": package.package_sha256,
        "source": record.source_code,
        "source_release": record.release,
        "source_id": record.source_id,
        "production_eligible": false
    });
    sqlx::query("INSERT INTO composition.composition_profile (id, food_id, source_record_id, profile_type, basis_amount, basis_unit, edible_basis, quality_grade, method_metadata, status) VALUES ($1, $2, $3, 'laboratory', 100, 'g', true, 'U', $4, 'in_review')")
        .bind(profile_id).bind(food_id).bind(source_record_id).bind(metadata).execute(&mut **tx).await?;
    insert_values(tx, profile_id, source_record_id, record, package).await?;
    Ok(profile_id)
}

async fn insert_values(
    tx: &mut Transaction<'_, Postgres>,
    profile_id: Uuid,
    source_record_id: Uuid,
    record: &RawSourceRecord,
    package: &LoadedPackage,
) -> Result<(), CatalogHandoffImportError> {
    let nutrient_ids: BTreeMap<String, Uuid> = sqlx::query_as::<_, (String, Uuid)>("SELECT code, id FROM composition.nutrient WHERE code IN ('energy_kcal', 'protein_g', 'fat_g', 'carbohydrate_g')").fetch_all(&mut **tx).await?.into_iter().collect();
    for value in package
        .compositions
        .iter()
        .filter(|value| value.source_id == record.source_id)
    {
        let nutrient_id = nutrient_ids.get(&value.target_code).ok_or_else(|| {
            CatalogHandoffImportError::InvalidInput(format!(
                "internal nutrient {} is unavailable",
                value.target_code
            ))
        })?;
        let amount = value
            .value
            .map(|number| rust_decimal::Decimal::from_str(&number.to_string()))
            .transpose()
            .map_err(|_| {
                CatalogHandoffImportError::InvalidInput(format!(
                    "composition value {} is not representable as decimal",
                    value.value_id
                ))
            })?;
        let status = match value.value_status.as_str() {
            "missing" => "missing",
            "trace" => "trace",
            "not_detected" => "not_detected",
            _ => "compiled",
        };
        let unit = value.canonical_unit.as_str();
        let metadata = json!({
            "contract_version": package.manifest.contract_version,
            "package_id": package.manifest.package_id,
            "source_record_id": source_record_id,
            "source_nutrient_id": value.source_nutrient_id,
            "source_method": value.source_method,
            "policy_version": value.policy_version,
            "production_eligible": false
        });
        sqlx::query("INSERT INTO composition.composition_value (profile_id, nutrient_id, amount, canonical_amount, unit, value_status, method_code, source_nutrient_id, source_method, source_metadata) VALUES ($1, $2, $3, $3, $4, $5, $6, $7, $8, $9)")
            .bind(profile_id).bind(nutrient_id).bind(amount).bind(unit).bind(status).bind(&value.source_method).bind(i64::try_from(value.source_nutrient_id).map_err(|_| CatalogHandoffImportError::InvalidInput("source nutrient ID exceeds PostgreSQL bigint".to_owned()))?).bind(&value.source_method).bind(metadata)
            .execute(&mut **tx).await?;
    }
    Ok(())
}

async fn add_profile_to_release(
    tx: &mut Transaction<'_, Postgres>,
    release_id: Uuid,
    profile_id: Uuid,
) -> Result<(), CatalogHandoffImportError> {
    sqlx::query("INSERT INTO catalog.catalog_release_profile (catalog_release_id, profile_id) VALUES ($1, $2) ON CONFLICT DO NOTHING").bind(release_id).bind(profile_id).execute(&mut **tx).await?;
    Ok(())
}
