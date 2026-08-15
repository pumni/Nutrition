use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use thiserror::Error;
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogReleaseActivationRequest {
    pub release_id: Uuid,
    pub expected_current_active_release: Option<Uuid>,
    pub validation_report_hash: String,
    pub reviewer_id: Uuid,
    pub approval_reference: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogReleaseActivationReport {
    pub catalog_release_id: Uuid,
    pub previous_active_release_id: Option<Uuid>,
    pub dataset_release_id: Uuid,
}

#[derive(Debug, Error)]
pub enum CatalogReleaseActivationError {
    #[error("invalid catalog activation input: {0}")]
    InvalidInput(String),
    #[error("catalog release {0} was not found")]
    ReleaseNotFound(Uuid),
    #[error("catalog release {release_id} is not staged; current status is {status}")]
    ReleaseNotStaged { release_id: Uuid, status: String },
    #[error("catalog activation validation failed: {0}")]
    ValidationFailed(String),
    #[error(
        "active catalog release changed concurrently: expected {expected:?}, actual {actual:?}"
    )]
    ActiveReleaseConflict {
        expected: Option<Uuid>,
        actual: Option<Uuid>,
    },
    #[error(
        "catalog release {release_id} manifest checksum mismatch: stored {stored}, actual {actual}"
    )]
    ReleaseChecksumMismatch {
        release_id: Uuid,
        stored: String,
        actual: String,
    },
    #[error("catalog activation database query failed")]
    Query(#[from] sqlx::Error),
    #[error("catalog activation JSON serialization failed")]
    Json(#[from] serde_json::Error),
}

/// Explicitly activates a validated staged catalog release.
///
/// The transaction locks the staged release and the current active pointer, verifies the
/// validation evidence and every staged profile, promotes the reviewed catalog content, marks the
/// previous release superseded, and updates the raw source pointer atomically. The importer never
/// calls this function.
///
/// # Errors
///
/// Returns an error without committing any lifecycle mutation when a release, validation report,
/// provenance record, reviewer approval, or expected-current-release invariant is invalid.
pub async fn activate_catalog_release(
    pool: &PgPool,
    request: &CatalogReleaseActivationRequest,
) -> Result<CatalogReleaseActivationReport, CatalogReleaseActivationError> {
    validate_request(request)?;
    let mut tx = pool.begin().await?;

    let previous_active_release_id = lock_current_active_release(&mut tx).await?;
    if previous_active_release_id != request.expected_current_active_release {
        return Err(CatalogReleaseActivationError::ActiveReleaseConflict {
            expected: request.expected_current_active_release,
            actual: previous_active_release_id,
        });
    }

    let release = sqlx::query(
        "SELECT status, manifest, checksum_sha256
           FROM catalog.catalog_release
          WHERE id = $1
          FOR UPDATE",
    )
    .bind(request.release_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(CatalogReleaseActivationError::ReleaseNotFound(
        request.release_id,
    ))?;
    let status: String = release.try_get("status")?;
    if status != "staged" {
        return Err(CatalogReleaseActivationError::ReleaseNotStaged {
            release_id: request.release_id,
            status,
        });
    }

    let manifest: Value = release.try_get("manifest")?;
    let stored_checksum: String = release.try_get("checksum_sha256")?;
    verify_release_checksum(request.release_id, &manifest, &stored_checksum)?;
    let dataset_release_id = validate_release_evidence(&mut tx, request, &manifest).await?;

    if let Some(previous_active_release_id) = previous_active_release_id {
        sqlx::query(
            "UPDATE catalog.catalog_release
                SET status = 'superseded'
              WHERE id = $1 AND status = 'active'",
        )
        .bind(previous_active_release_id)
        .execute(&mut *tx)
        .await?;
    }

    promote_release_content(&mut tx, request.release_id).await?;
    sqlx::query(
        "UPDATE catalog.catalog_release
            SET status = 'active', activated_at = now()
          WHERE id = $1 AND status = 'staged'",
    )
    .bind(request.release_id)
    .execute(&mut *tx)
    .await?;

    update_source_activation(
        &mut tx,
        dataset_release_id,
        request.reviewer_id,
        &request.approval_reference,
    )
    .await?;

    tx.commit().await?;
    Ok(CatalogReleaseActivationReport {
        catalog_release_id: request.release_id,
        previous_active_release_id,
        dataset_release_id,
    })
}

fn validate_request(
    request: &CatalogReleaseActivationRequest,
) -> Result<(), CatalogReleaseActivationError> {
    if request.release_id.is_nil() {
        return Err(CatalogReleaseActivationError::InvalidInput(
            "release_id must not be nil".to_owned(),
        ));
    }
    if request.reviewer_id.is_nil() {
        return Err(CatalogReleaseActivationError::InvalidInput(
            "reviewer_id must not be nil".to_owned(),
        ));
    }
    if normalize_hash(&request.validation_report_hash).is_err() {
        return Err(CatalogReleaseActivationError::InvalidInput(
            "validation_report_hash must be exactly 64 hexadecimal characters".to_owned(),
        ));
    }
    if request.approval_reference.trim().is_empty() {
        return Err(CatalogReleaseActivationError::InvalidInput(
            "approval_reference must not be empty".to_owned(),
        ));
    }
    Ok(())
}

async fn lock_current_active_release(
    tx: &mut Transaction<'_, Postgres>,
) -> Result<Option<Uuid>, CatalogReleaseActivationError> {
    sqlx::query_scalar(
        "SELECT id
           FROM catalog.catalog_release
          WHERE status = 'active'
          FOR UPDATE",
    )
    .fetch_optional(&mut **tx)
    .await
    .map_err(CatalogReleaseActivationError::Query)
}

fn verify_release_checksum(
    release_id: Uuid,
    manifest: &Value,
    stored_checksum: &str,
) -> Result<(), CatalogReleaseActivationError> {
    let actual = sha256_hex(&serde_json::to_vec(manifest)?);
    if actual != stored_checksum {
        return Err(CatalogReleaseActivationError::ReleaseChecksumMismatch {
            release_id,
            stored: stored_checksum.to_owned(),
            actual,
        });
    }
    Ok(())
}

async fn validate_release_evidence(
    tx: &mut Transaction<'_, Postgres>,
    request: &CatalogReleaseActivationRequest,
    manifest: &Value,
) -> Result<Uuid, CatalogReleaseActivationError> {
    validate_manifest_evidence(request, manifest)?;
    let dataset_release_id = manifest_dataset_release_id(manifest)?;
    validate_dataset_release(tx, dataset_release_id).await?;
    validate_profile_readiness(tx, request.release_id).await?;
    validate_approved_mappings(tx, request.release_id).await?;
    validate_catalog_names(tx, request.release_id).await?;
    Ok(dataset_release_id)
}

fn validate_manifest_evidence(
    request: &CatalogReleaseActivationRequest,
    manifest: &Value,
) -> Result<(), CatalogReleaseActivationError> {
    let validation = manifest
        .get("validation")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            CatalogReleaseActivationError::ValidationFailed(
                "catalog manifest has no validation evidence".to_owned(),
            )
        })?;
    let expected_report_hash = normalize_hash(&request.validation_report_hash).map_err(|()| {
        CatalogReleaseActivationError::InvalidInput(
            "validation_report_hash must be exactly 64 hexadecimal characters".to_owned(),
        )
    })?;
    let report_hash = validation
        .get("report_sha256")
        .and_then(Value::as_str)
        .and_then(|value| normalize_hash(value).ok());
    if report_hash.as_deref() != Some(expected_report_hash.as_str()) {
        return Err(CatalogReleaseActivationError::ValidationFailed(
            "validation report hash does not belong to the staged release".to_owned(),
        ));
    }
    if validation.get("status").and_then(Value::as_str) != Some("passed") {
        return Err(CatalogReleaseActivationError::ValidationFailed(
            "validation report status is not passed".to_owned(),
        ));
    }
    if validation
        .get("production_eligible")
        .and_then(Value::as_bool)
        != Some(true)
        || manifest.get("production_eligible").and_then(Value::as_bool) != Some(true)
    {
        return Err(CatalogReleaseActivationError::ValidationFailed(
            "production eligibility evidence is not approved".to_owned(),
        ));
    }
    Ok(())
}

fn manifest_dataset_release_id(manifest: &Value) -> Result<Uuid, CatalogReleaseActivationError> {
    manifest
        .get("source_dataset_release_id")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            CatalogReleaseActivationError::ValidationFailed(
                "catalog manifest has no source dataset release".to_owned(),
            )
        })?
        .parse::<Uuid>()
        .map_err(|_| {
            CatalogReleaseActivationError::ValidationFailed(
                "source dataset release ID is not a UUID".to_owned(),
            )
        })
}

async fn validate_dataset_release(
    tx: &mut Transaction<'_, Postgres>,
    dataset_release_id: Uuid,
) -> Result<(), CatalogReleaseActivationError> {
    let dataset_release_status = sqlx::query_scalar::<_, String>(
        "SELECT status
           FROM raw.dataset_release
          WHERE id = $1
          FOR UPDATE",
    )
    .bind(dataset_release_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| {
        CatalogReleaseActivationError::ValidationFailed(
            "source dataset release does not exist".to_owned(),
        )
    })?;
    if dataset_release_status != "imported" {
        return Err(CatalogReleaseActivationError::ValidationFailed(format!(
            "source dataset release is not imported: {dataset_release_status}"
        )));
    }
    Ok(())
}

async fn validate_profile_readiness(
    tx: &mut Transaction<'_, Postgres>,
    release_id: Uuid,
) -> Result<(), CatalogReleaseActivationError> {
    let profile_stats = sqlx::query(
        "SELECT count(*)::bigint AS total,
                count(*) FILTER (WHERE profile.status IN ('in_review', 'published'))::bigint AS reviewable,
                count(*) FILTER (WHERE profile.method_metadata->'energy_mapping'->>'status' = 'complete')::bigint AS complete_energy,
                count(*) FILTER (WHERE profile.method_metadata->>'production_eligible' = 'true')::bigint AS eligible
           FROM catalog.catalog_release_profile membership
           JOIN composition.composition_profile profile
             ON profile.id = membership.profile_id
          WHERE membership.catalog_release_id = $1",
    )
    .bind(release_id)
    .fetch_one(&mut **tx)
    .await?;
    let total: i64 = profile_stats.try_get("total")?;
    let reviewable: i64 = profile_stats.try_get("reviewable")?;
    let complete_energy: i64 = profile_stats.try_get("complete_energy")?;
    let eligible: i64 = profile_stats.try_get("eligible")?;
    if total == 0 || reviewable != total || complete_energy != total || eligible != total {
        return Err(CatalogReleaseActivationError::ValidationFailed(format!(
            "profile readiness failed: total={total}, reviewable={reviewable}, complete_energy={complete_energy}, eligible={eligible}"
        )));
    }
    Ok(())
}

async fn validate_approved_mappings(
    tx: &mut Transaction<'_, Postgres>,
    release_id: Uuid,
) -> Result<(), CatalogReleaseActivationError> {
    let missing_mappings: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint
           FROM catalog.catalog_release_profile membership
           JOIN composition.composition_profile profile
             ON profile.id = membership.profile_id
          WHERE membership.catalog_release_id = $1
            AND NOT EXISTS (
                SELECT 1
                  FROM catalog.food_mapping mapping
                 WHERE mapping.food_id = profile.food_id
                   AND mapping.source_food_record_id = profile.source_record_id
                   AND mapping.mapping_type <> 'rejected'
                   AND mapping.review_status = 'approved'
            )",
    )
    .bind(release_id)
    .fetch_one(&mut **tx)
    .await?;
    if missing_mappings != 0 {
        return Err(CatalogReleaseActivationError::ValidationFailed(format!(
            "{missing_mappings} staged profiles have no approved source mapping"
        )));
    }
    Ok(())
}

async fn validate_catalog_names(
    tx: &mut Transaction<'_, Postgres>,
    release_id: Uuid,
) -> Result<(), CatalogReleaseActivationError> {
    let missing_names: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint
           FROM catalog.catalog_release_profile membership
           JOIN composition.composition_profile profile
             ON profile.id = membership.profile_id
          WHERE membership.catalog_release_id = $1
            AND NOT EXISTS (
                SELECT 1
                  FROM catalog.catalog_release_food_name name_membership
                  JOIN catalog.food_name name ON name.id = name_membership.food_name_id
                 WHERE name_membership.catalog_release_id = $1
                   AND name.food_id = profile.food_id
                   AND name.valid_to IS NULL
            )",
    )
    .bind(release_id)
    .fetch_one(&mut **tx)
    .await?;
    if missing_names != 0 {
        return Err(CatalogReleaseActivationError::ValidationFailed(format!(
            "{missing_names} staged profiles have no active catalog name"
        )));
    }
    Ok(())
}

async fn promote_release_content(
    tx: &mut Transaction<'_, Postgres>,
    release_id: Uuid,
) -> Result<(), CatalogReleaseActivationError> {
    sqlx::query(
        "UPDATE composition.composition_profile profile
            SET status = 'published', valid_from = COALESCE(valid_from, now())
          WHERE profile.id IN (
              SELECT profile_id
                FROM catalog.catalog_release_profile
               WHERE catalog_release_id = $1
          )
            AND profile.status = 'in_review'",
    )
    .bind(release_id)
    .execute(&mut **tx)
    .await?;

    sqlx::query(
        "UPDATE catalog.food_entity food
            SET lifecycle_status = 'active', updated_at = now()
          WHERE food.id IN (
              SELECT profile.food_id
                FROM catalog.catalog_release_profile membership
                JOIN composition.composition_profile profile
                  ON profile.id = membership.profile_id
               WHERE membership.catalog_release_id = $1
          )
            AND food.lifecycle_status = 'draft'",
    )
    .bind(release_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn update_source_activation(
    tx: &mut Transaction<'_, Postgres>,
    dataset_release_id: Uuid,
    reviewer_id: Uuid,
    approval_reference: &str,
) -> Result<(), CatalogReleaseActivationError> {
    let dataset_id: Uuid =
        sqlx::query_scalar("SELECT dataset_id FROM raw.dataset_release WHERE id = $1 FOR UPDATE")
            .bind(dataset_release_id)
            .fetch_one(&mut **tx)
            .await?;
    let previous_source_release_id = sqlx::query_scalar::<_, Uuid>(
        "SELECT active_release_id
           FROM raw.source_activation
          WHERE dataset_id = $1
          FOR UPDATE",
    )
    .bind(dataset_id)
    .fetch_optional(&mut **tx)
    .await?;

    let rollback_source_release_id =
        previous_source_release_id.filter(|id| *id != dataset_release_id);
    if let Some(previous_source_release_id) = rollback_source_release_id {
        sqlx::query(
            "UPDATE raw.dataset_release
                SET status = 'superseded'
              WHERE id = $1 AND status = 'imported'",
        )
        .bind(previous_source_release_id)
        .execute(&mut **tx)
        .await?;
    }

    let reason = format!("approval_reference={approval_reference}");
    sqlx::query(
        "INSERT INTO raw.source_activation
            (dataset_id, active_release_id, previous_release_id, activated_by, reason)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (dataset_id) DO UPDATE
            SET active_release_id = EXCLUDED.active_release_id,
                previous_release_id = EXCLUDED.previous_release_id,
                activated_by = EXCLUDED.activated_by,
                activated_at = now(),
                reason = EXCLUDED.reason",
    )
    .bind(dataset_id)
    .bind(dataset_release_id)
    .bind(rollback_source_release_id)
    .bind(reviewer_id)
    .bind(reason)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn normalize_hash(value: &str) -> Result<String, ()> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.len() != 64 || !normalized.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(());
    }
    Ok(normalized)
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
