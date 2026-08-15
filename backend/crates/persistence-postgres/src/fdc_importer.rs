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

pub const FDC_FOUNDATION_IMPORTER_VERSION: &str = "fdc-foundation-json-0.1.0";
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
    pub replayed: bool,
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

struct RawFood {
    fdc_id: u64,
    description: String,
    payload: Value,
}

struct StagedNutrient {
    internal_code: &'static str,
    amount: Decimal,
    minimum: Option<Decimal>,
    maximum: Option<Decimal>,
    method_code: Option<String>,
}

struct PreparedImport {
    created_by: Uuid,
    foods: Vec<RawFood>,
    selected_ids: BTreeSet<u64>,
    source_sha256: String,
    schema_fingerprint: String,
    selection_fingerprint: String,
    catalog_release_version: String,
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

    let created_by = request.created_by.parse::<Uuid>().map_err(|_| {
        FdcFoundationImportError::InvalidInput("created_by must be a UUID".to_owned())
    })?;
    let selected_ids = reviewed_selection(&request.include_fdc_ids)?;
    let foods = parse_source_foods(source_bytes)?;
    validate_selection_exists(&foods, &selected_ids)?;
    for food in foods
        .iter()
        .filter(|food| selected_ids.contains(&food.fdc_id))
    {
        extract_unambiguous_macronutrients(food)?;
    }

    let schema_fingerprint = sha256_hex(FDC_SCHEMA_CONTRACT.as_bytes());
    let selection_fingerprint = selection_fingerprint(&selected_ids);
    let release_fingerprint = sha256_hex(
        format!(
            "{}:{selection_fingerprint}:{FDC_FOUNDATION_IMPORTER_VERSION}",
            request.release_version
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
    })
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

    ensure_core_macronutrients(tx).await?;
    let catalog_release_id = create_staged_catalog_release(tx, prepared, dataset_release_id).await?;
    stage_reviewed_selection(tx, prepared, dataset_release_id, catalog_release_id).await?;
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
        "selection_sha256": prepared.selection_fingerprint,
        "selected_fdc_ids": prepared.selected_ids.iter().copied().collect::<Vec<_>>(),
        "selected_count": prepared.selected_ids.len(),
        "raw_record_count": prepared.foods.len(),
        "energy_policy": "pending_issue_22",
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
    dataset_release_id: Uuid,
    catalog_release_id: Uuid,
) -> Result<(), FdcFoundationImportError> {
    for food in prepared
        .foods
        .iter()
        .filter(|food| prepared.selected_ids.contains(&food.fdc_id))
    {
        stage_selected_food(tx, dataset_release_id, catalog_release_id, food).await?;
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
    let mut root: Value = serde_json::from_slice(source_bytes)?;
    let food_values = root
        .get_mut("FoundationFoods")
        .and_then(Value::as_array_mut)
        .map(std::mem::take)
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
        let fdc_id = payload
            .get("fdcId")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                FdcFoundationImportError::InvalidInput(
                    "every Foundation food must contain an unsigned integer fdcId".to_owned(),
                )
            })?;
        if !seen_ids.insert(fdc_id) {
            return Err(FdcFoundationImportError::InvalidInput(format!(
                "duplicate FDC ID {fdc_id} in source artifact"
            )));
        }
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
        foods.push(RawFood {
            fdc_id,
            description,
            payload,
        });
    }
    Ok(foods)
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

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

async fn ensure_fdc_dataset(
    tx: &mut Transaction<'_, Postgres>,
) -> Result<Uuid, FdcFoundationImportError> {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO raw.dataset
            (id, code, name, publisher, license_code, license_url, homepage_url,
             ingestion_policy_version, metadata)
         VALUES
            ($1, $2, 'USDA FoodData Central',
             'U.S. Department of Agriculture, Agricultural Research Service',
             'CC0-1.0', 'https://creativecommons.org/publicdomain/zero/1.0/',
             'https://fdc.nal.usda.gov/', $3,
             jsonb_build_object('initial_data_type', 'Foundation'))
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
            (id, dataset_id, version, source_published_at, object_uri, checksum_sha256,
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
                (id, dataset_release_id, external_id, source_data_type, source_name,
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

async fn ensure_core_macronutrients(
    tx: &mut Transaction<'_, Postgres>,
) -> Result<(), FdcFoundationImportError> {
    for (code, name, group_code) in [
        ("protein_g", "Protein", "macronutrient/protein"),
        ("fat_g", "Fat", "macronutrient/fat"),
        (
            "carbohydrate_g",
            "Carbohydrate",
            "macronutrient/carbohydrate",
        ),
    ] {
        sqlx::query(
            "INSERT INTO composition.nutrient
                (id, code, preferred_name, canonical_unit, nutrient_group, external_identifiers)
             VALUES ($1, $2, $3, 'g', $4, '{}'::jsonb)
             ON CONFLICT (code) DO NOTHING",
        )
        .bind(Uuid::now_v7())
        .bind(code)
        .bind(name)
        .bind(group_code)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn stage_selected_food(
    tx: &mut Transaction<'_, Postgres>,
    dataset_release_id: Uuid,
    catalog_release_id: Uuid,
    food: &RawFood,
) -> Result<(), FdcFoundationImportError> {
    let source_record_id = source_record_id(tx, dataset_release_id, food.fdc_id).await?;
    let food_id = ensure_food_entity(tx, food.fdc_id).await?;
    ensure_food_mapping(tx, source_record_id, food_id).await?;
    let food_name_id = ensure_food_name(tx, source_record_id, food_id, food).await?;
    add_name_to_release(tx, catalog_release_id, food_name_id).await?;
    let profile_id = ensure_staged_profile(tx, source_record_id, food_id, food).await?;
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
            (id, source_record_id, food_id, mapping_method, mapping_version, confidence,
             match_type, review_status, rationale)
         SELECT $1, $2, $3, 'fdc_exact_external_id', $4, 1.0, 'exact', 'proposed',
                'Deterministic mapping from the pinned FDC external ID; requires catalog review before publication'
          WHERE NOT EXISTS (
              SELECT 1 FROM catalog.food_mapping
               WHERE source_record_id = $2 AND food_id = $3
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
          ORDER BY created_at
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
    food: &RawFood,
) -> Result<Uuid, FdcFoundationImportError> {
    if let Some(id) = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM composition.composition_profile
          WHERE food_id = $1 AND source_record_id = $2 AND basis_code = 'edible_grams'
          ORDER BY created_at
          LIMIT 1",
    )
    .bind(food_id)
    .bind(source_record_id)
    .fetch_optional(&mut **tx)
    .await?
    {
        return Ok(id);
    }
    create_staged_profile(tx, food_id, source_record_id, food).await
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
    food: &RawFood,
) -> Result<Uuid, FdcFoundationImportError> {
    let nutrients = extract_unambiguous_macronutrients(food)?;
    let profile_id = Uuid::now_v7();
    let method_metadata = json!({
        "source": FDC_DATASET_CODE,
        "fdc_id": food.fdc_id,
        "importer_version": FDC_FOUNDATION_IMPORTER_VERSION,
        "energy_policy": "pending_issue_22",
        "production_eligible": false
    });
    sqlx::query(
        "INSERT INTO composition.composition_profile
            (id, food_id, source_record_id, profile_type, preparation_state, edible_fraction,
             basis_code, quality_grade, method_metadata, status)
         VALUES ($1, $2, $3, 'laboratory', 'as_published', 1.0, 'edible_grams', 'U', $4, 'in_review')",
    )
    .bind(profile_id)
    .bind(food_id)
    .bind(source_record_id)
    .bind(method_metadata)
    .execute(&mut **tx)
    .await?;

    let nutrient_ids = sqlx::query_as::<_, (String, Uuid)>(
        "SELECT code, id FROM composition.nutrient
          WHERE code IN ('protein_g', 'fat_g', 'carbohydrate_g')",
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
        sqlx::query(
            "INSERT INTO composition.composition_value
                (profile_id, nutrient_id, amount_per_100g, lower_amount_per_100g,
                 upper_amount_per_100g, value_status, method_code)
             VALUES ($1, $2, $3, $4, $5, 'compiled', $6)",
        )
        .bind(profile_id)
        .bind(nutrient_id)
        .bind(nutrient.amount)
        .bind(nutrient.minimum)
        .bind(nutrient.maximum)
        .bind(nutrient.method_code)
        .execute(&mut **tx)
        .await?;
    }
    Ok(profile_id)
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
    use super::{extract_unambiguous_macronutrients, parse_source_foods, reviewed_selection};
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
    fn parses_foundation_shape_and_ignores_unresolved_energy() {
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
        assert!(!codes.contains("energy_kcal"));
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
}
