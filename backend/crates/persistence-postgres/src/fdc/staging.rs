#![allow(clippy::wildcard_imports)]

use super::*;

pub(crate) async fn stage_import(
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

pub(crate) async fn mark_dataset_release_imported(
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

pub(crate) async fn existing_catalog_release(
    tx: &mut Transaction<'_, Postgres>,
    version: &str,
) -> Result<Option<Uuid>, FdcFoundationImportError> {
    sqlx::query_scalar::<_, Uuid>("SELECT id FROM catalog.catalog_release WHERE version = $1")
        .bind(version)
        .fetch_optional(&mut **tx)
        .await
        .map_err(FdcFoundationImportError::Query)
}

pub(crate) async fn create_staged_catalog_release(
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

pub(crate) async fn stage_reviewed_selection(
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

pub(crate) async fn ensure_core_nutrients(
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

pub(crate) async fn stage_selected_food(
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

pub(crate) async fn source_record_id(
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

pub(crate) async fn ensure_food_entity(
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

pub(crate) async fn ensure_food_mapping(
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

pub(crate) async fn ensure_food_name(
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

pub(crate) async fn add_name_to_release(
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

pub(crate) async fn ensure_staged_profile(
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

pub(crate) async fn add_profile_to_release(
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

pub(crate) async fn create_staged_profile(
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

pub(crate) async fn insert_staged_values(
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
