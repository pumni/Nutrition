#![allow(clippy::wildcard_imports)]

use super::*;

pub(crate) async fn ensure_fdc_dataset(
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

pub(crate) async fn ensure_dataset_release(
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

pub(crate) fn validate_existing_release(
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

pub(crate) async fn create_dataset_release(
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

pub(crate) async fn store_raw_records(
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
