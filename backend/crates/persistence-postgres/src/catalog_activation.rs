use serde_json::{Value, json};
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogReleaseRollbackRequest {
    pub source_release_id: Uuid,
    pub new_version: String,
    pub created_by: Uuid,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogReleaseRollbackReport {
    pub rollback_release_id: Uuid,
    pub source_release_id: Uuid,
    pub validation_report_hash: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogReleaseProductionEligibilityRequest {
    pub release_id: Uuid,
    pub rollback_target_release_id: Uuid,
    pub validation_report: Vec<u8>,
    pub impact_report: Vec<u8>,
    pub reviewer_id: Uuid,
    pub approval_reference: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogReleaseProductionEligibilityReport {
    pub catalog_release_id: Uuid,
    pub catalog_release_version: String,
    pub validation_report_hash: String,
    pub impact_report_hash: String,
    pub rollback_target_release_id: Uuid,
    pub selected_profile_count: usize,
    pub selected_food_count: usize,
}

struct ActiveRollbackTarget {
    id: Uuid,
    version: String,
    manifest_checksum: String,
}

struct SourceDatasetEvidence {
    version: String,
    payload_sha256: String,
    schema_fingerprint: String,
    metadata: Value,
}

struct SelectedCatalogEvidence {
    fdc_ids: Vec<u64>,
    memberships: Vec<Value>,
    profile_count: usize,
    food_count: usize,
    selection_sha256: String,
}

struct SelectionReviewInput<'a> {
    request: &'a CatalogReleaseProductionEligibilityRequest,
    catalog_release_version: &'a str,
    dataset_release_id: Uuid,
    manifest: &'a Value,
    impact_report: &'a Value,
    rollback_target: &'a ActiveRollbackTarget,
}

struct ProductionReviewRecord<'a> {
    request: &'a CatalogReleaseProductionEligibilityRequest,
    catalog_release_version: &'a str,
    dataset_release_id: Uuid,
    source: &'a SourceDatasetEvidence,
    selected: &'a SelectedCatalogEvidence,
    validation_report_hash: &'a str,
    impact_report_hash: &'a str,
    rollback_target: &'a ActiveRollbackTarget,
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
    #[error("catalog rollback source {release_id} has status {status}; expected superseded")]
    RollbackSourceStatus { release_id: Uuid, status: String },
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
    let active_release_checksum =
        active_release_checksum(&mut tx, previous_active_release_id).await?;

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
    let dataset_release_id =
        validate_release_evidence(&mut tx, request, &manifest, &active_release_checksum).await?;

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

/// Creates a new staged immutable snapshot from a superseded catalog release.
///
/// A superseded release is never changed back to `active`. Its memberships and validated manifest
/// are copied into a new staged release, which must still pass the normal explicit activation
/// command and reviewer gate. This preserves the release history and gives the activation
/// transaction a distinct rollback release to audit.
///
/// # Errors
///
/// Returns an error when the source release is not superseded, its manifest checksum or validation
/// evidence is invalid, or the new version conflicts with an existing release.
pub async fn stage_catalog_rollback(
    pool: &PgPool,
    request: &CatalogReleaseRollbackRequest,
) -> Result<CatalogReleaseRollbackReport, CatalogReleaseActivationError> {
    validate_rollback_request(request)?;
    let mut tx = pool.begin().await?;
    let source = sqlx::query(
        "SELECT status, manifest, checksum_sha256
           FROM catalog.catalog_release
          WHERE id = $1
          FOR UPDATE",
    )
    .bind(request.source_release_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(CatalogReleaseActivationError::ReleaseNotFound(
        request.source_release_id,
    ))?;
    let status: String = source.try_get("status")?;
    if status != "superseded" {
        return Err(CatalogReleaseActivationError::RollbackSourceStatus {
            release_id: request.source_release_id,
            status,
        });
    }
    let source_manifest: Value = source.try_get("manifest")?;
    let source_checksum: String = source.try_get("checksum_sha256")?;
    verify_release_checksum(
        request.source_release_id,
        &source_manifest,
        &source_checksum,
    )?;
    let validation_report_hash = rollback_validation_hash(&source_manifest)?;
    let _dataset_release_id = manifest_dataset_release_id(&source_manifest)?;

    let version_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM catalog.catalog_release WHERE version = $1
         )",
    )
    .bind(&request.new_version)
    .fetch_one(&mut *tx)
    .await?;
    if version_exists {
        return Err(CatalogReleaseActivationError::InvalidInput(format!(
            "catalog release version already exists: {}",
            request.new_version
        )));
    }

    let mut rollback_manifest = source_manifest;
    rollback_manifest["rollback_of_catalog_release_id"] =
        Value::String(request.source_release_id.to_string());
    rollback_manifest["rollback_source_checksum_sha256"] = Value::String(source_checksum);
    rollback_manifest["catalog_release_version"] = Value::String(request.new_version.clone());
    rollback_manifest["validation"]["production_eligible"] = Value::Bool(false);
    rollback_manifest["production_eligible"] = Value::Bool(false);
    if let Some(manifest) = rollback_manifest.as_object_mut() {
        manifest.remove("production_eligibility_review");
        manifest.remove("impact_report");
    }
    let rollback_checksum = sha256_hex(&serde_json::to_vec(&rollback_manifest)?);
    let rollback_release_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO catalog.catalog_release
            (id, version, status, manifest, checksum_sha256, created_by)
         VALUES ($1, $2, 'staged', $3, $4, $5)",
    )
    .bind(rollback_release_id)
    .bind(&request.new_version)
    .bind(&rollback_manifest)
    .bind(rollback_checksum)
    .bind(request.created_by)
    .execute(&mut *tx)
    .await?;
    copy_release_memberships(&mut tx, request.source_release_id, rollback_release_id).await?;
    tx.commit().await?;

    Ok(CatalogReleaseRollbackReport {
        rollback_release_id,
        source_release_id: request.source_release_id,
        validation_report_hash,
    })
}

/// Records the explicit human review that transitions one exact staged import to production eligible.
///
/// The machine validation report must remain non-eligible. This function verifies that it belongs
/// to the staged release, verifies the independently reviewed impact report and rollback target,
/// then records the reviewer and report hashes in the staged manifest. It does not activate the
/// catalog release.
///
/// # Errors
///
/// Returns an error if source, selection, membership, nutrient mapping, impact, reviewer, or
/// rollback-target evidence is missing or does not match the staged release.
pub async fn review_catalog_release_production_eligibility(
    pool: &PgPool,
    request: &CatalogReleaseProductionEligibilityRequest,
) -> Result<CatalogReleaseProductionEligibilityReport, CatalogReleaseActivationError> {
    validate_production_eligibility_request(request)?;
    let validation_report: Value = serde_json::from_slice(&request.validation_report)?;
    let impact_report: Value = serde_json::from_slice(&request.impact_report)?;
    let validation_report_hash = sha256_hex(&request.validation_report);
    let impact_report_hash = sha256_hex(&request.impact_report);

    let mut tx = pool.begin().await?;
    let rollback_target =
        load_active_rollback_target(&mut tx, request.rollback_target_release_id).await?;
    let (catalog_release_version, mut manifest) =
        load_staged_release_for_review(&mut tx, request.release_id).await?;
    let dataset_release_id = manifest_dataset_release_id(&manifest)?;
    let source = load_source_dataset_evidence(
        &mut tx,
        dataset_release_id,
        manifest.get("rollback_of_catalog_release_id").is_some(),
    )
    .await?;
    let selected_fdc_ids = validate_fdc_production_validation_report(
        &manifest,
        &validation_report,
        &source.metadata,
        &source.version,
        &source.payload_sha256,
        &source.schema_fingerprint,
    )?;
    let selected = validate_selection_review(
        &mut tx,
        SelectionReviewInput {
            request,
            catalog_release_version: &catalog_release_version,
            dataset_release_id,
            manifest: &manifest,
            impact_report: &impact_report,
            rollback_target: &rollback_target,
        },
        selected_fdc_ids,
    )
    .await?;
    approve_reviewed_catalog_content(&mut tx, request.release_id, request.reviewer_id).await?;
    store_production_review(
        &mut manifest,
        &ProductionReviewRecord {
            request,
            catalog_release_version: &catalog_release_version,
            dataset_release_id,
            source: &source,
            selected: &selected,
            validation_report_hash: &validation_report_hash,
            impact_report_hash: &impact_report_hash,
            rollback_target: &rollback_target,
        },
    );
    let release_checksum = sha256_hex(&serde_json::to_vec(&manifest)?);
    sqlx::query(
        "UPDATE catalog.catalog_release
            SET manifest = $2, checksum_sha256 = $3
          WHERE id = $1 AND status = 'staged'",
    )
    .bind(request.release_id)
    .bind(&manifest)
    .bind(release_checksum)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(CatalogReleaseProductionEligibilityReport {
        catalog_release_id: request.release_id,
        catalog_release_version,
        validation_report_hash,
        impact_report_hash,
        rollback_target_release_id: rollback_target.id,
        selected_profile_count: selected.profile_count,
        selected_food_count: selected.food_count,
    })
}

async fn load_active_rollback_target(
    tx: &mut Transaction<'_, Postgres>,
    expected_id: Uuid,
) -> Result<ActiveRollbackTarget, CatalogReleaseActivationError> {
    let row = sqlx::query(
        "SELECT id, version, manifest, checksum_sha256
           FROM catalog.catalog_release
          WHERE status = 'active'
          FOR UPDATE",
    )
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| {
        CatalogReleaseActivationError::ValidationFailed(
            "production eligibility requires a current active rollback target".to_owned(),
        )
    })?;
    let id: Uuid = row.try_get("id")?;
    if id != expected_id {
        return Err(CatalogReleaseActivationError::ActiveReleaseConflict {
            expected: Some(expected_id),
            actual: Some(id),
        });
    }
    let manifest: Value = row.try_get("manifest")?;
    let manifest_checksum: String = row.try_get("checksum_sha256")?;
    verify_release_checksum(id, &manifest, &manifest_checksum)?;
    Ok(ActiveRollbackTarget {
        id,
        version: row.try_get("version")?,
        manifest_checksum,
    })
}

async fn load_staged_release_for_review(
    tx: &mut Transaction<'_, Postgres>,
    release_id: Uuid,
) -> Result<(String, Value), CatalogReleaseActivationError> {
    let release = sqlx::query(
        "SELECT version, status, manifest, checksum_sha256
           FROM catalog.catalog_release
          WHERE id = $1
          FOR UPDATE",
    )
    .bind(release_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(CatalogReleaseActivationError::ReleaseNotFound(release_id))?;
    let status: String = release.try_get("status")?;
    if status != "staged" {
        return Err(CatalogReleaseActivationError::ReleaseNotStaged { release_id, status });
    }
    let manifest: Value = release.try_get("manifest")?;
    let stored_checksum: String = release.try_get("checksum_sha256")?;
    verify_release_checksum(release_id, &manifest, &stored_checksum)?;
    if manifest.get("production_eligible").and_then(Value::as_bool) != Some(false) {
        return Err(CatalogReleaseActivationError::ValidationFailed(
            "staged release must be non-eligible before human review".to_owned(),
        ));
    }
    Ok((release.try_get("version")?, manifest))
}

async fn load_source_dataset_evidence(
    tx: &mut Transaction<'_, Postgres>,
    dataset_release_id: Uuid,
    is_rollback: bool,
) -> Result<SourceDatasetEvidence, CatalogReleaseActivationError> {
    let release = sqlx::query(
        "SELECT version, status, checksum_sha256, schema_fingerprint, metadata
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
    let status: String = release.try_get("status")?;
    if status != "imported" && !(is_rollback && status == "superseded") {
        return Err(CatalogReleaseActivationError::ValidationFailed(format!(
            "source dataset release is not imported: {status}"
        )));
    }
    Ok(SourceDatasetEvidence {
        version: release.try_get("version")?,
        payload_sha256: release.try_get("checksum_sha256")?,
        schema_fingerprint: release.try_get("schema_fingerprint")?,
        metadata: release.try_get("metadata")?,
    })
}

async fn validate_selection_review(
    tx: &mut Transaction<'_, Postgres>,
    input: SelectionReviewInput<'_>,
    selected_fdc_ids: Vec<u64>,
) -> Result<SelectedCatalogEvidence, CatalogReleaseActivationError> {
    let memberships = staged_profile_memberships(tx, input.request.release_id).await?;
    let profile_count = memberships.len();
    let food_count = memberships
        .iter()
        .map(|membership| membership["food_id"].as_str().unwrap_or_default())
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let staged_fdc_ids = memberships
        .iter()
        .filter_map(|membership| membership["fdc_id"].as_u64())
        .collect::<Vec<_>>();
    if staged_fdc_ids != selected_fdc_ids
        || profile_count != selected_fdc_ids.len()
        || food_count != selected_fdc_ids.len()
    {
        return Err(CatalogReleaseActivationError::ValidationFailed(
            "staged profile and food membership does not match the reviewed FDC selection"
                .to_owned(),
        ));
    }
    let selection_sha256 = input
        .manifest
        .get("selection_sha256")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            CatalogReleaseActivationError::ValidationFailed(
                "staged release has no reviewed selection checksum".to_owned(),
            )
        })?
        .to_owned();
    validate_m3_impact_report(
        input.impact_report,
        input.request.release_id,
        input.catalog_release_version,
        input.dataset_release_id,
        &selection_sha256,
        profile_count,
        food_count,
        input.rollback_target.id,
        &input.rollback_target.version,
        &input.rollback_target.manifest_checksum,
    )?;
    Ok(SelectedCatalogEvidence {
        fdc_ids: selected_fdc_ids,
        memberships,
        profile_count,
        food_count,
        selection_sha256,
    })
}

async fn approve_reviewed_catalog_content(
    tx: &mut Transaction<'_, Postgres>,
    release_id: Uuid,
    reviewer_id: Uuid,
) -> Result<(), CatalogReleaseActivationError> {
    sqlx::query(
        "UPDATE catalog.food_mapping mapping
            SET review_status = 'approved', reviewed_by = $2, reviewed_at = now()
          WHERE mapping.mapping_type = 'exact'
            AND mapping.mapping_method = 'fdc_exact_external_id'
            AND mapping.review_status = 'proposed'
            AND EXISTS (
                SELECT 1
                  FROM catalog.catalog_release_profile membership
                  JOIN composition.composition_profile profile
                    ON profile.id = membership.profile_id
                 WHERE membership.catalog_release_id = $1
                   AND profile.food_id = mapping.food_id
                   AND profile.source_record_id = mapping.source_food_record_id
            )",
    )
    .bind(release_id)
    .bind(reviewer_id)
    .execute(&mut **tx)
    .await?;
    let missing_exact_approvals: i64 = sqlx::query_scalar(
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
                   AND mapping.mapping_type = 'exact'
                   AND mapping.mapping_method = 'fdc_exact_external_id'
                   AND mapping.review_status = 'approved'
            )",
    )
    .bind(release_id)
    .fetch_one(&mut **tx)
    .await?;
    if missing_exact_approvals != 0 {
        return Err(CatalogReleaseActivationError::ValidationFailed(format!(
            "{missing_exact_approvals} selected FDC identities have no approved exact mapping"
        )));
    }
    sqlx::query(
        "UPDATE composition.composition_profile profile
            SET method_metadata = jsonb_set(
                profile.method_metadata, '{production_eligible}', 'true'::jsonb
            )
          WHERE profile.id IN (
              SELECT profile_id
                FROM catalog.catalog_release_profile
               WHERE catalog_release_id = $1
          )
            AND profile.method_metadata->'production_eligible' IS DISTINCT FROM 'true'::jsonb",
    )
    .bind(release_id)
    .execute(&mut **tx)
    .await?;
    validate_profile_readiness(tx, release_id).await?;
    validate_approved_mappings(tx, release_id).await?;
    validate_catalog_names(tx, release_id).await
}

fn store_production_review(manifest: &mut Value, record: &ProductionReviewRecord<'_>) {
    let ProductionReviewRecord {
        request,
        catalog_release_version,
        dataset_release_id,
        source,
        selected,
        validation_report_hash,
        impact_report_hash,
        rollback_target,
    } = record;
    manifest["catalog_release_version"] = json!(catalog_release_version);
    manifest["validation"] = json!({
        "report_sha256": validation_report_hash,
        "status": "passed",
        "production_eligible": true
    });
    manifest["impact_report"] = json!({
        "report_sha256": impact_report_hash,
        "report_version": "m3-catalog-impact-0.1.0",
        "status": "reviewed"
    });
    manifest["production_eligible"] = Value::Bool(true);
    manifest["production_eligibility_review"] = json!({
        "schema_version": "m3-catalog-production-eligibility-0.1.0",
        "catalog_release_id": request.release_id,
        "catalog_release_version": catalog_release_version,
        "reviewer_id": request.reviewer_id,
        "approval_reference": request.approval_reference,
        "source_dataset_release_id": dataset_release_id,
        "source_release_version": source.version,
        "source_archive_sha256": manifest["source_archive_sha256"],
        "source_payload_sha256": source.payload_sha256,
        "normalized_payload_sha256": manifest["normalized_payload_sha256"],
        "importer_version": manifest["importer_version"],
        "preprocessing_policy_version": manifest["preprocessing_policy_version"],
        "energy_policy": manifest["energy_policy"],
        "schema_fingerprint": source.schema_fingerprint,
        "selection_sha256": selected.selection_sha256,
        "selected_fdc_ids": selected.fdc_ids,
        "selected_membership": selected.memberships,
        "selected_profile_count": selected.profile_count,
        "selected_food_count": selected.food_count,
        "nutrient_mapping_validation": "passed",
        "validation_report_sha256": validation_report_hash,
        "impact_report_sha256": impact_report_hash,
        "rollback_target": {
            "catalog_release_id": rollback_target.id,
            "catalog_release_version": rollback_target.version,
            "manifest_sha256": rollback_target.manifest_checksum
        },
        "production_eligible": true,
        "production_activation_authorized": false
    });
}

fn validate_production_eligibility_request(
    request: &CatalogReleaseProductionEligibilityRequest,
) -> Result<(), CatalogReleaseActivationError> {
    if request.release_id.is_nil() || request.rollback_target_release_id.is_nil() {
        return Err(CatalogReleaseActivationError::InvalidInput(
            "release and rollback target IDs must not be nil".to_owned(),
        ));
    }
    if request.release_id == request.rollback_target_release_id {
        return Err(CatalogReleaseActivationError::InvalidInput(
            "release and rollback target must be different catalog releases".to_owned(),
        ));
    }
    if request.reviewer_id.is_nil() {
        return Err(CatalogReleaseActivationError::InvalidInput(
            "reviewer_id must not be nil".to_owned(),
        ));
    }
    if request.approval_reference.trim().is_empty() {
        return Err(CatalogReleaseActivationError::InvalidInput(
            "approval_reference must not be empty".to_owned(),
        ));
    }
    if request.validation_report.is_empty() || request.impact_report.is_empty() {
        return Err(CatalogReleaseActivationError::InvalidInput(
            "validation and impact report artifacts are required".to_owned(),
        ));
    }
    Ok(())
}

fn validate_fdc_production_validation_report(
    manifest: &Value,
    report: &Value,
    source_metadata: &Value,
    source_release_version: &str,
    source_payload_sha256: &str,
    schema_fingerprint: &str,
) -> Result<Vec<u64>, CatalogReleaseActivationError> {
    validate_fdc_machine_report_binding(
        manifest,
        report,
        source_metadata,
        source_release_version,
        source_payload_sha256,
        schema_fingerprint,
    )?;
    validate_selected_fdc_records(manifest, report)
}

fn validate_fdc_machine_report_binding(
    manifest: &Value,
    report: &Value,
    source_metadata: &Value,
    source_release_version: &str,
    source_payload_sha256: &str,
    schema_fingerprint: &str,
) -> Result<(), CatalogReleaseActivationError> {
    let report_error = || {
        CatalogReleaseActivationError::ValidationFailed(
            "machine validation report does not match the staged FDC release".to_owned(),
        )
    };
    let source_archive_sha256 = manifest
        .get("source_archive_sha256")
        .and_then(Value::as_str)
        .filter(|value| normalize_hash(value).is_ok())
        .ok_or_else(report_error)?;
    let source_payload_manifest_sha256 = manifest
        .get("source_payload_sha256")
        .and_then(Value::as_str)
        .ok_or_else(report_error)?;
    if manifest.get("source").and_then(Value::as_str) != Some("usda_fdc")
        || manifest.get("source_release").and_then(Value::as_str) != Some(source_release_version)
        || source_payload_manifest_sha256 != source_payload_sha256
        || manifest.get("schema_fingerprint").and_then(Value::as_str) != Some(schema_fingerprint)
        || source_metadata
            .get("source_archive_sha256")
            .and_then(Value::as_str)
            != Some(source_archive_sha256)
        || source_metadata.get("preprocessing_policy_version")
            != manifest.get("preprocessing_policy_version")
        || report.get("source").and_then(Value::as_str) != Some("usda_fdc")
        || report.get("source_release").and_then(Value::as_str) != Some(source_release_version)
        || report.get("source_archive_sha256").and_then(Value::as_str)
            != Some(source_archive_sha256)
        || report.get("artifact_sha256").and_then(Value::as_str) != Some(source_payload_sha256)
        || report
            .get("expected_artifact_sha256")
            .and_then(Value::as_str)
            != Some(source_payload_sha256)
        || report.get("schema_fingerprint").and_then(Value::as_str) != Some(schema_fingerprint)
        || report.get("normalized_payload_sha256") != manifest.get("normalized_payload_sha256")
        || report.get("preprocessing_policy_version")
            != manifest.get("preprocessing_policy_version")
        || report.get("importer_version") != manifest.get("importer_version")
        || report.get("energy_policy") != manifest.get("energy_policy")
        || report.get("validation_passed").and_then(Value::as_bool) != Some(true)
        || report.get("artifact_valid").and_then(Value::as_bool) != Some(true)
        || report.get("checksum_valid").and_then(Value::as_bool) != Some(true)
        || report
            .get("source_integrity_valid")
            .and_then(Value::as_bool)
            != Some(true)
        || report
            .get("normalized_payload_valid")
            .and_then(Value::as_bool)
            != Some(true)
        || report.get("selection_valid").and_then(Value::as_bool) != Some(true)
        || report.get("selection_status").and_then(Value::as_str) != Some("validated")
        || report.get("production_eligible").and_then(Value::as_bool) != Some(false)
        || report.get("release").and_then(|value| value.get("status"))
            != Some(&Value::String("staged_only".to_owned()))
        || report
            .get("release")
            .and_then(|value| value.get("reviewer_approved"))
            .and_then(Value::as_bool)
            != Some(false)
        || report
            .get("release")
            .and_then(|value| value.get("activation_attempted"))
            .and_then(Value::as_bool)
            != Some(false)
    {
        return Err(report_error());
    }
    Ok(())
}

fn validate_selected_fdc_records(
    manifest: &Value,
    report: &Value,
) -> Result<Vec<u64>, CatalogReleaseActivationError> {
    let report_error = || {
        CatalogReleaseActivationError::ValidationFailed(
            "machine validation report does not match the staged FDC release".to_owned(),
        )
    };
    let mut selected_ids = manifest
        .get("selected_fdc_ids")
        .and_then(Value::as_array)
        .ok_or_else(report_error)?
        .iter()
        .map(|value| value.as_u64().ok_or_else(report_error))
        .collect::<Result<Vec<_>, _>>()?;
    selected_ids.sort_unstable();
    if selected_ids.is_empty()
        || selected_ids.windows(2).any(|pair| pair[0] == pair[1])
        || manifest.get("selected_count").and_then(Value::as_u64)
            != u64::try_from(selected_ids.len()).ok()
    {
        return Err(report_error());
    }
    let selection_sha256 = sha256_hex(
        selected_ids
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(",")
            .as_bytes(),
    );
    if manifest.get("selection_sha256").and_then(Value::as_str) != Some(&selection_sha256)
        || report.get("selection_fingerprint").and_then(Value::as_str) != Some(&selection_sha256)
        || report.get("selected_records").and_then(Value::as_u64)
            != u64::try_from(selected_ids.len()).ok()
    {
        return Err(report_error());
    }
    let selected_energy = report.get("selected_energy").ok_or_else(report_error)?;
    let energy_specific = selected_energy
        .get("atwater_specific_2048")
        .and_then(Value::as_u64)
        .ok_or_else(report_error)?;
    let energy_general = selected_energy
        .get("atwater_general_2047")
        .and_then(Value::as_u64)
        .ok_or_else(report_error)?;
    if selected_energy.get("missing").and_then(Value::as_u64) != Some(0)
        || selected_energy
            .get("unexpected_legacy_1008")
            .and_then(Value::as_u64)
            != Some(0)
        || energy_specific + energy_general != u64::try_from(selected_ids.len()).unwrap_or(0)
    {
        return Err(report_error());
    }
    Ok(selected_ids)
}

async fn staged_profile_memberships(
    tx: &mut Transaction<'_, Postgres>,
    release_id: Uuid,
) -> Result<Vec<Value>, CatalogReleaseActivationError> {
    let rows = sqlx::query(
        "SELECT profile.id AS profile_id,
                profile.food_id,
                profile.source_record_id,
                profile.method_metadata->>'fdc_id' AS fdc_id,
                profile.method_metadata->'energy_mapping'->>'status' AS energy_status,
                profile.status AS profile_status
           FROM catalog.catalog_release_profile membership
           JOIN composition.composition_profile profile
             ON profile.id = membership.profile_id
          WHERE membership.catalog_release_id = $1
          ORDER BY (profile.method_metadata->>'fdc_id')::bigint",
    )
    .bind(release_id)
    .fetch_all(&mut **tx)
    .await?;
    let mut memberships = Vec::with_capacity(rows.len());
    for row in rows {
        let fdc_id = row
            .try_get::<String, _>("fdc_id")?
            .parse::<u64>()
            .map_err(|_| {
                CatalogReleaseActivationError::ValidationFailed(
                    "staged FDC profile has an invalid external ID".to_owned(),
                )
            })?;
        let energy_status: String = row.try_get("energy_status")?;
        let profile_status: String = row.try_get("profile_status")?;
        if energy_status != "complete"
            || !matches!(profile_status.as_str(), "in_review" | "published")
        {
            return Err(CatalogReleaseActivationError::ValidationFailed(
                "staged profile readiness does not match the validation report".to_owned(),
            ));
        }
        memberships.push(json!({
            "fdc_id": fdc_id,
            "profile_id": row.try_get::<Uuid, _>("profile_id")?,
            "food_id": row.try_get::<Uuid, _>("food_id")?,
            "source_record_id": row.try_get::<Uuid, _>("source_record_id")?
        }));
    }
    Ok(memberships)
}

#[allow(clippy::too_many_arguments)]
fn validate_m3_impact_report(
    report: &Value,
    release_id: Uuid,
    release_version: &str,
    dataset_release_id: Uuid,
    selection_sha256: &str,
    profile_count: usize,
    food_count: usize,
    rollback_target_id: Uuid,
    rollback_target_version: &str,
    rollback_target_checksum: &str,
) -> Result<(), CatalogReleaseActivationError> {
    let rollback_target = report.get("rollback_target");
    let profile_count = u64::try_from(profile_count).ok();
    let food_count = u64::try_from(food_count).ok();
    let report_matches = report.get("schema_version").and_then(Value::as_str)
        == Some("m3-catalog-impact-0.1.0")
        && report.get("status").and_then(Value::as_str) == Some("passed")
        && report.get("catalog_release_id").and_then(Value::as_str)
            == Some(release_id.to_string().as_str())
        && report
            .get("catalog_release_version")
            .and_then(Value::as_str)
            == Some(release_version)
        && report
            .get("source_dataset_release_id")
            .and_then(Value::as_str)
            == Some(dataset_release_id.to_string().as_str())
        && report.get("selection_sha256").and_then(Value::as_str) == Some(selection_sha256)
        && report.get("selected_profile_count").and_then(Value::as_u64) == profile_count
        && report.get("selected_food_count").and_then(Value::as_u64) == food_count
        && report
            .get("nutrient_mapping_validation")
            .and_then(Value::as_str)
            == Some("passed")
        && report
            .get("impact_summary")
            .and_then(Value::as_object)
            .is_some()
        && report.get("production_eligible").and_then(Value::as_bool) == Some(false)
        && report
            .get("clinical_correctness_claimed")
            .and_then(Value::as_bool)
            == Some(false)
        && rollback_target
            .and_then(|value| value.get("catalog_release_id"))
            .and_then(Value::as_str)
            == Some(rollback_target_id.to_string().as_str())
        && rollback_target
            .and_then(|value| value.get("catalog_release_version"))
            .and_then(Value::as_str)
            == Some(rollback_target_version)
        && rollback_target
            .and_then(|value| value.get("manifest_sha256"))
            .and_then(Value::as_str)
            == Some(rollback_target_checksum);
    if !report_matches {
        return Err(CatalogReleaseActivationError::ValidationFailed(
            "impact report does not bind the staged release, selection, mappings, and rollback target"
                .to_owned(),
        ));
    }
    Ok(())
}

fn validate_rollback_request(
    request: &CatalogReleaseRollbackRequest,
) -> Result<(), CatalogReleaseActivationError> {
    if request.source_release_id.is_nil() {
        return Err(CatalogReleaseActivationError::InvalidInput(
            "source_release_id must not be nil".to_owned(),
        ));
    }
    if request.created_by.is_nil() {
        return Err(CatalogReleaseActivationError::InvalidInput(
            "created_by must not be nil".to_owned(),
        ));
    }
    if request.new_version.trim().is_empty() {
        return Err(CatalogReleaseActivationError::InvalidInput(
            "new_version must not be empty".to_owned(),
        ));
    }
    Ok(())
}

fn rollback_validation_hash(manifest: &Value) -> Result<String, CatalogReleaseActivationError> {
    let validation = manifest
        .get("validation")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            CatalogReleaseActivationError::ValidationFailed(
                "rollback source has no validation evidence".to_owned(),
            )
        })?;
    if validation.get("status").and_then(Value::as_str) != Some("passed")
        || validation
            .get("production_eligible")
            .and_then(Value::as_bool)
            != Some(true)
        || manifest.get("production_eligible").and_then(Value::as_bool) != Some(true)
    {
        return Err(CatalogReleaseActivationError::ValidationFailed(
            "rollback source validation evidence is not production eligible".to_owned(),
        ));
    }
    let report_hash = validation
        .get("report_sha256")
        .and_then(Value::as_str)
        .and_then(|value| normalize_hash(value).ok())
        .ok_or_else(|| {
            CatalogReleaseActivationError::ValidationFailed(
                "rollback source has no valid validation report hash".to_owned(),
            )
        })?;
    Ok(report_hash)
}

async fn copy_release_memberships(
    tx: &mut Transaction<'_, Postgres>,
    source_release_id: Uuid,
    rollback_release_id: Uuid,
) -> Result<(), CatalogReleaseActivationError> {
    for statement in [
        "INSERT INTO catalog.catalog_release_food_name (catalog_release_id, food_name_id)
         SELECT $1, food_name_id
           FROM catalog.catalog_release_food_name
          WHERE catalog_release_id = $2",
        "INSERT INTO catalog.catalog_release_profile (catalog_release_id, profile_id)
         SELECT $1, profile_id
           FROM catalog.catalog_release_profile
          WHERE catalog_release_id = $2",
        "INSERT INTO catalog.catalog_release_portion_observation
            (catalog_release_id, portion_observation_id)
         SELECT $1, portion_observation_id
           FROM catalog.catalog_release_portion_observation
          WHERE catalog_release_id = $2",
    ] {
        sqlx::query(statement)
            .bind(rollback_release_id)
            .bind(source_release_id)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
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

async fn active_release_checksum(
    tx: &mut Transaction<'_, Postgres>,
    release_id: Option<Uuid>,
) -> Result<String, CatalogReleaseActivationError> {
    let release_id = release_id.ok_or_else(|| {
        CatalogReleaseActivationError::ValidationFailed(
            "production eligibility requires a current active rollback target".to_owned(),
        )
    })?;
    let (manifest, checksum): (Value, String) = sqlx::query_as(
        "SELECT manifest, checksum_sha256
           FROM catalog.catalog_release
          WHERE id = $1 AND status = 'active'",
    )
    .bind(release_id)
    .fetch_one(&mut **tx)
    .await?;
    verify_release_checksum(release_id, &manifest, &checksum)?;
    Ok(checksum)
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
    active_release_checksum: &str,
) -> Result<Uuid, CatalogReleaseActivationError> {
    validate_manifest_evidence(request, manifest, active_release_checksum)?;
    let dataset_release_id = manifest_dataset_release_id(manifest)?;
    validate_dataset_release(
        tx,
        dataset_release_id,
        manifest.get("rollback_of_catalog_release_id").is_some(),
    )
    .await?;
    validate_profile_readiness(tx, request.release_id).await?;
    validate_approved_mappings(tx, request.release_id).await?;
    validate_catalog_names(tx, request.release_id).await?;
    Ok(dataset_release_id)
}

fn validate_manifest_evidence(
    request: &CatalogReleaseActivationRequest,
    manifest: &Value,
    active_release_checksum: &str,
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
    validate_production_eligibility_review(
        request,
        manifest,
        &expected_report_hash,
        active_release_checksum,
    )?;
    Ok(())
}

fn validate_production_eligibility_review(
    request: &CatalogReleaseActivationRequest,
    manifest: &Value,
    validation_report_hash: &str,
    active_release_checksum: &str,
) -> Result<(), CatalogReleaseActivationError> {
    let review = manifest
        .get("production_eligibility_review")
        .and_then(Value::as_object);
    let impact = manifest.get("impact_report").and_then(Value::as_object);
    let review_error = || {
        CatalogReleaseActivationError::ValidationFailed(
            "explicit production eligibility review does not match the staged release".to_owned(),
        )
    };
    let review = review.ok_or_else(review_error)?;
    let impact = impact.ok_or_else(review_error)?;
    let reviewer_id = review
        .get("reviewer_id")
        .and_then(Value::as_str)
        .and_then(|value| value.parse::<Uuid>().ok())
        .filter(|value| !value.is_nil())
        .ok_or_else(review_error)?;
    let approval_reference = review
        .get("approval_reference")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty());
    let rollback_target_id = request
        .expected_current_active_release
        .ok_or_else(review_error)?;
    let expected_release_version = manifest
        .get("catalog_release_version")
        .and_then(Value::as_str)
        .ok_or_else(review_error)?;
    let expected_impact_hash = review
        .get("impact_report_sha256")
        .and_then(Value::as_str)
        .and_then(|value| normalize_hash(value).ok());
    let report_matches = review.get("schema_version").and_then(Value::as_str)
        == Some("m3-catalog-production-eligibility-0.1.0")
        && review.get("catalog_release_id").and_then(Value::as_str)
            == Some(request.release_id.to_string().as_str())
        && review
            .get("catalog_release_version")
            .and_then(Value::as_str)
            == Some(expected_release_version)
        && review.get("source_dataset_release_id") == manifest.get("source_dataset_release_id")
        && review.get("selection_sha256") == manifest.get("selection_sha256")
        && review
            .get("validation_report_sha256")
            .and_then(Value::as_str)
            == Some(validation_report_hash)
        && reviewer_id == request.reviewer_id
        && approval_reference == Some(request.approval_reference.as_str())
        && review.get("production_eligible").and_then(Value::as_bool) == Some(true)
        && review
            .get("production_activation_authorized")
            .and_then(Value::as_bool)
            == Some(false)
        && review
            .get("nutrient_mapping_validation")
            .and_then(Value::as_str)
            == Some("passed")
        && review
            .get("selected_profile_count")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0
        && review
            .get("selected_food_count")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0
        && approval_reference.is_some()
        && impact.get("status").and_then(Value::as_str) == Some("reviewed")
        && impact.get("report_sha256").and_then(Value::as_str) == expected_impact_hash.as_deref()
        && review
            .get("rollback_target")
            .and_then(|value| value.get("catalog_release_id"))
            .and_then(Value::as_str)
            == Some(rollback_target_id.to_string().as_str())
        && review
            .get("rollback_target")
            .and_then(|value| value.get("manifest_sha256"))
            .and_then(Value::as_str)
            .and_then(|value| normalize_hash(value).ok())
            .as_deref()
            == Some(active_release_checksum);
    if !report_matches || reviewer_id.is_nil() {
        return Err(review_error());
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
    is_rollback: bool,
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
    let acceptable_status = dataset_release_status == "imported"
        || (is_rollback && dataset_release_status == "superseded");
    if !acceptable_status {
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
