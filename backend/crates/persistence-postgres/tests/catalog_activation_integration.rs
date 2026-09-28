use persistence_postgres::{
    CatalogReleaseActivationError, CatalogReleaseActivationRequest,
    CatalogReleaseProductionEligibilityReport, CatalogReleaseProductionEligibilityRequest,
    CatalogReleaseRollbackRequest, FdcFoundationImportRequest, FdcFoundationValidationRequest,
    activate_catalog_release, connect, import_fdc_foundation_json, migrate,
    review_catalog_release_production_eligibility, stage_catalog_rollback,
    validate_fdc_foundation_json,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use std::env;
use uuid::Uuid;

const FOUNDATION_FIXTURE: &str = r#"{
  "FoundationFoods": [
    {
      "fdcId": 910000001,
      "dataType": "Foundation",
      "description": "Synthetic activation apple, raw",
      "foodNutrients": [
        {"amount": 0.30, "nutrient": {"id": 1003, "unitName": "G"}},
        {"amount": 0.20, "nutrient": {"id": 1004, "unitName": "G"}},
        {"amount": 13.80, "nutrient": {"id": 1005, "unitName": "G"}},
        {"amount": 52.0, "nutrient": {"id": 2048, "unitName": "KCAL"}}
      ]
    }
  ]
}"#;

const REVIEWER_ID: &str = "0198f100-0000-7000-8000-000000000098";

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and PostgreSQL 18"]
async fn catalog_activation_validates_and_supersedes_atomically() {
    let pool = setup_database().await;
    let previous_release_id = ensure_previous_active_release(&pool).await;
    let release_version = format!("activation-{}", Uuid::now_v7());
    let import_request = build_import_request(&release_version);
    let report = import_fdc_foundation_json(&pool, FOUNDATION_FIXTURE.as_bytes(), &import_request)
        .await
        .expect("activation fixture must stage");
    let reviewer_id = Uuid::parse_str(REVIEWER_ID).expect("reviewer UUID must be valid");
    let unreviewed_activation_request = CatalogReleaseActivationRequest {
        release_id: report.catalog_release_id,
        expected_current_active_release: Some(previous_release_id),
        validation_report_hash: "b".repeat(64),
        reviewer_id,
        approval_reference: "human-review:test-activation".to_owned(),
    };

    let validation_error = activate_catalog_release(&pool, &unreviewed_activation_request)
        .await
        .expect_err("an unvalidated staged release must remain inactive");
    assert!(matches!(
        validation_error,
        CatalogReleaseActivationError::ValidationFailed(_)
    ));
    assert_release_status(&pool, report.catalog_release_id, "staged").await;
    assert_release_status(&pool, previous_release_id, "active").await;

    assert_machine_eligibility_flags_cannot_enable_release(
        &pool,
        report.catalog_release_id,
        &report.catalog_release_version,
        &unreviewed_activation_request,
    )
    .await;

    let eligibility = review_test_release(
        &pool,
        TestReviewInput {
            release_id: report.catalog_release_id,
            source_release_version: &release_version,
            catalog_release_version: &report.catalog_release_version,
            source_bytes: FOUNDATION_FIXTURE.as_bytes(),
            fdc_id: 910_000_001,
            rollback_target_release_id: previous_release_id,
            reviewer_id,
            approval_reference: "human-review:test-activation",
        },
    )
    .await;
    assert_activation_rejects_mismatched_validation_hash(
        &pool,
        report.catalog_release_id,
        previous_release_id,
        reviewer_id,
    )
    .await;
    assert_activation_rejects_mismatched_approval_reference(
        &pool,
        report.catalog_release_id,
        previous_release_id,
        reviewer_id,
        &eligibility,
    )
    .await;
    assert_release_status(&pool, report.catalog_release_id, "staged").await;
    assert_release_status(&pool, previous_release_id, "active").await;
    let activation_request = CatalogReleaseActivationRequest {
        release_id: report.catalog_release_id,
        expected_current_active_release: Some(previous_release_id),
        validation_report_hash: eligibility.validation_report_hash.clone(),
        reviewer_id,
        approval_reference: "human-review:test-activation".to_owned(),
    };

    let activation = activate_catalog_release(&pool, &activation_request)
        .await
        .expect("reviewed staged release must activate");
    assert_eq!(activation.catalog_release_id, report.catalog_release_id);
    assert_eq!(
        activation.previous_active_release_id,
        Some(previous_release_id)
    );
    assert_eq!(activation.dataset_release_id, report.dataset_release_id);

    assert_release_status(&pool, report.catalog_release_id, "active").await;
    assert_release_status(&pool, previous_release_id, "superseded").await;
    assert_published_content(&pool, report.catalog_release_id).await;
    assert_source_activation(
        &pool,
        report.dataset_release_id,
        reviewer_id,
        "approval_reference=human-review:test-activation",
    )
    .await;

    let retry_error = activate_catalog_release(&pool, &activation_request)
        .await
        .expect_err("an active release must not be activated a second time");
    assert!(matches!(
        retry_error,
        CatalogReleaseActivationError::ActiveReleaseConflict { .. }
            | CatalogReleaseActivationError::ReleaseNotStaged { .. }
    ));
}

async fn assert_machine_eligibility_flags_cannot_enable_release(
    pool: &PgPool,
    release_id: Uuid,
    catalog_release_version: &str,
    activation_request: &CatalogReleaseActivationRequest,
) {
    let original_manifest: Value =
        sqlx::query_scalar("SELECT manifest FROM catalog.catalog_release WHERE id = $1")
            .bind(release_id)
            .fetch_one(pool)
            .await
            .expect("original staged manifest must be readable");
    let mut forged_manifest = original_manifest.clone();
    forged_manifest["catalog_release_version"] = json!(catalog_release_version);
    forged_manifest["validation"] = json!({
        "report_sha256": activation_request.validation_report_hash,
        "status": "passed",
        "production_eligible": true
    });
    forged_manifest["production_eligible"] = Value::Bool(true);
    let forged_checksum =
        sha256_hex(&serde_json::to_vec(&forged_manifest).expect("forged manifest must serialize"));
    sqlx::query(
        "UPDATE catalog.catalog_release SET manifest = $2, checksum_sha256 = $3 WHERE id = $1",
    )
    .bind(release_id)
    .bind(&forged_manifest)
    .bind(forged_checksum)
    .execute(pool)
    .await
    .expect("test manifest must simulate a machine-only eligibility flag");
    let forged_activation_error = activate_catalog_release(pool, activation_request)
        .await
        .expect_err("eligibility booleans without a human review must not activate");
    assert!(matches!(
        forged_activation_error,
        CatalogReleaseActivationError::ValidationFailed(_)
    ));
    let original_checksum = sha256_hex(
        &serde_json::to_vec(&original_manifest).expect("original manifest must serialize"),
    );
    sqlx::query(
        "UPDATE catalog.catalog_release SET manifest = $2, checksum_sha256 = $3 WHERE id = $1",
    )
    .bind(release_id)
    .bind(&original_manifest)
    .bind(original_checksum)
    .execute(pool)
    .await
    .expect("original staged manifest must be restored");
}

async fn assert_activation_rejects_mismatched_validation_hash(
    pool: &PgPool,
    release_id: Uuid,
    expected_current_active_release: Uuid,
    reviewer_id: Uuid,
) {
    let error = activate_catalog_release(
        pool,
        &CatalogReleaseActivationRequest {
            release_id,
            expected_current_active_release: Some(expected_current_active_release),
            validation_report_hash: "c".repeat(64),
            reviewer_id,
            approval_reference: "human-review:test-activation".to_owned(),
        },
    )
    .await
    .expect_err("activation must reject a mismatched validation report hash");
    assert!(matches!(
        error,
        CatalogReleaseActivationError::ValidationFailed(_)
    ));
}

async fn assert_activation_rejects_mismatched_approval_reference(
    pool: &PgPool,
    release_id: Uuid,
    expected_current_active_release: Uuid,
    reviewer_id: Uuid,
    eligibility: &CatalogReleaseProductionEligibilityReport,
) {
    let error = activate_catalog_release(
        pool,
        &CatalogReleaseActivationRequest {
            release_id,
            expected_current_active_release: Some(expected_current_active_release),
            validation_report_hash: eligibility.validation_report_hash.clone(),
            reviewer_id,
            approval_reference: "human-review:other-approval".to_owned(),
        },
    )
    .await
    .expect_err("activation must use the reviewer's exact approval reference");
    assert!(matches!(
        error,
        CatalogReleaseActivationError::ValidationFailed(_)
    ));
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and PostgreSQL 18"]
async fn catalog_rollback_creates_new_release_and_preserves_history() {
    let pool = setup_database().await;
    let initial_active_release_id = ensure_previous_active_release(&pool).await;
    let reviewer_id = Uuid::parse_str(REVIEWER_ID).expect("reviewer UUID must be valid");

    let first = stage_and_prepare_release(&pool, "rollback-a", 910_000_101, reviewer_id).await;
    activate_prepared_release(
        &pool,
        &first,
        Some(initial_active_release_id),
        reviewer_id,
        "human-review:rollback-a",
    )
    .await;

    let second = stage_and_prepare_release(&pool, "rollback-b", 910_000_102, reviewer_id).await;
    activate_prepared_release(
        &pool,
        &second,
        Some(first.catalog_release_id),
        reviewer_id,
        "human-review:rollback-b",
    )
    .await;
    assert_release_status(&pool, first.catalog_release_id, "superseded").await;

    let rollback_version = format!("rollback-target-{}", Uuid::now_v7());
    let rollback = stage_catalog_rollback(
        &pool,
        &CatalogReleaseRollbackRequest {
            source_release_id: first.catalog_release_id,
            new_version: rollback_version.clone(),
            created_by: reviewer_id,
        },
    )
    .await
    .expect("superseded validated release must produce a staged rollback snapshot");
    assert_eq!(rollback.source_release_id, first.catalog_release_id);
    assert_eq!(
        rollback.validation_report_hash,
        first.validation_report_hash
    );
    assert_release_status(&pool, rollback.rollback_release_id, "staged").await;
    assert_release_membership_counts(&pool, rollback.rollback_release_id).await;
    let unreviewed_rollback_error = activate_catalog_release(
        &pool,
        &CatalogReleaseActivationRequest {
            release_id: rollback.rollback_release_id,
            expected_current_active_release: Some(second.catalog_release_id),
            validation_report_hash: rollback.validation_report_hash.clone(),
            reviewer_id,
            approval_reference: "human-review:rollback-execute".to_owned(),
        },
    )
    .await
    .expect_err("rollback snapshots must receive a fresh review before activation");
    assert!(matches!(
        unreviewed_rollback_error,
        CatalogReleaseActivationError::ValidationFailed(_)
    ));
    assert_release_status(&pool, rollback.rollback_release_id, "staged").await;
    assert_release_status(&pool, second.catalog_release_id, "active").await;

    let rollback_eligibility = review_test_release(
        &pool,
        TestReviewInput {
            release_id: rollback.rollback_release_id,
            source_release_version: &first.source_release_version,
            catalog_release_version: &rollback_version,
            source_bytes: first.source_bytes.as_bytes(),
            fdc_id: first.fdc_id,
            rollback_target_release_id: second.catalog_release_id,
            reviewer_id,
            approval_reference: "human-review:rollback-execute",
        },
    )
    .await;

    let rollback_activation = activate_catalog_release(
        &pool,
        &CatalogReleaseActivationRequest {
            release_id: rollback.rollback_release_id,
            expected_current_active_release: Some(second.catalog_release_id),
            validation_report_hash: rollback_eligibility.validation_report_hash,
            reviewer_id,
            approval_reference: "human-review:rollback-execute".to_owned(),
        },
    )
    .await
    .expect("rollback snapshot must activate through the normal activation gate");
    assert_eq!(
        rollback_activation.previous_active_release_id,
        Some(second.catalog_release_id)
    );
    assert_eq!(
        rollback_activation.dataset_release_id,
        first.dataset_release_id
    );
    assert_release_status(&pool, first.catalog_release_id, "superseded").await;
    assert_release_status(&pool, second.catalog_release_id, "superseded").await;
    assert_release_status(&pool, rollback.rollback_release_id, "active").await;
}

async fn stage_and_prepare_release(
    pool: &PgPool,
    version_prefix: &str,
    fdc_id: u64,
    reviewer_id: Uuid,
) -> PreparedRelease {
    let release_version = format!("{version_prefix}-{}", Uuid::now_v7());
    let fixture = FOUNDATION_FIXTURE
        .replace("910000001", &fdc_id.to_string())
        .replace(
            "Synthetic activation apple, raw",
            &format!("Synthetic {version_prefix} apple, raw"),
        );
    let report = import_fdc_foundation_json(
        pool,
        fixture.as_bytes(),
        &build_import_request_for_source(&release_version, fixture.as_bytes(), fdc_id),
    )
    .await
    .expect("rollback fixture must stage");
    let previous_active_release_id = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM catalog.catalog_release WHERE status = 'active'",
    )
    .fetch_one(pool)
    .await
    .expect("active release must be readable");
    let approval_reference = format!("human-review:{version_prefix}");
    let eligibility = review_test_release(
        pool,
        TestReviewInput {
            release_id: report.catalog_release_id,
            source_release_version: &release_version,
            catalog_release_version: &report.catalog_release_version,
            source_bytes: fixture.as_bytes(),
            fdc_id,
            rollback_target_release_id: previous_active_release_id,
            reviewer_id,
            approval_reference: &approval_reference,
        },
    )
    .await;
    PreparedRelease {
        catalog_release_id: report.catalog_release_id,
        dataset_release_id: report.dataset_release_id,
        validation_report_hash: eligibility.validation_report_hash,
        source_release_version: release_version,
        source_bytes: fixture,
        fdc_id,
    }
}

async fn activate_prepared_release(
    pool: &PgPool,
    release: &PreparedRelease,
    expected_current_active_release: Option<Uuid>,
    reviewer_id: Uuid,
    approval_reference: &str,
) {
    activate_catalog_release(
        pool,
        &CatalogReleaseActivationRequest {
            release_id: release.catalog_release_id,
            expected_current_active_release,
            validation_report_hash: release.validation_report_hash.clone(),
            reviewer_id,
            approval_reference: approval_reference.to_owned(),
        },
    )
    .await
    .expect("prepared release must activate");
}

#[derive(Clone, Debug)]
struct PreparedRelease {
    catalog_release_id: Uuid,
    dataset_release_id: Uuid,
    validation_report_hash: String,
    source_release_version: String,
    source_bytes: String,
    fdc_id: u64,
}

async fn setup_database() -> PgPool {
    let database_url =
        env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL is required for integration test");
    let pool = connect(&database_url, 4)
        .await
        .expect("integration database must connect");
    migrate(&pool)
        .await
        .expect("integration migrations must apply");
    pool
}

fn build_import_request(release_version: &str) -> FdcFoundationImportRequest {
    build_import_request_for_source(release_version, FOUNDATION_FIXTURE.as_bytes(), 910_000_001)
}

fn build_import_request_for_source(
    release_version: &str,
    source_bytes: &[u8],
    fdc_id: u64,
) -> FdcFoundationImportRequest {
    FdcFoundationImportRequest {
        release_version: release_version.to_owned(),
        source_published_date: "2026-04-30".to_owned(),
        object_uri: format!("fixture://fdc/{release_version}.json"),
        expected_sha256: sha256_hex(source_bytes),
        source_archive_sha256: Some(sha256_hex(source_bytes)),
        preprocessing_policy_version: None,
        include_fdc_ids: vec![fdc_id],
        created_by: "0198f100-0000-7000-8000-000000000099".to_owned(),
    }
}

async fn ensure_previous_active_release(pool: &PgPool) -> Uuid {
    if let Some(id) = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM catalog.catalog_release WHERE status = 'active'",
    )
    .fetch_optional(pool)
    .await
    .expect("current active release must be readable")
    {
        return id;
    }

    let id = Uuid::now_v7();
    let manifest = json!({"test_only": true});
    sqlx::query(
        "INSERT INTO catalog.catalog_release
            (id, version, status, manifest, checksum_sha256, created_by)
         VALUES ($1, $2, 'active', $3, $4, $5)",
    )
    .bind(id)
    .bind(format!("activation-previous-{id}"))
    .bind(&manifest)
    .bind(sha256_hex(
        &serde_json::to_vec(&manifest).expect("test JSON must serialize"),
    ))
    .bind(Uuid::parse_str("0198f100-0000-7000-8000-000000000099").expect("actor UUID"))
    .execute(pool)
    .await
    .expect("previous active release must be inserted");
    id
}

struct TestReviewInput<'a> {
    release_id: Uuid,
    source_release_version: &'a str,
    catalog_release_version: &'a str,
    source_bytes: &'a [u8],
    fdc_id: u64,
    rollback_target_release_id: Uuid,
    reviewer_id: Uuid,
    approval_reference: &'a str,
}

async fn review_test_release(
    pool: &PgPool,
    input: TestReviewInput<'_>,
) -> CatalogReleaseProductionEligibilityReport {
    let (validation_report, impact_report) = build_test_review_artifacts(pool, &input).await;
    let mut machine_only_request = CatalogReleaseProductionEligibilityRequest {
        release_id: input.release_id,
        rollback_target_release_id: input.rollback_target_release_id,
        validation_report: validation_report.clone(),
        impact_report: impact_report.clone(),
        reviewer_id: input.reviewer_id,
        approval_reference: String::new(),
    };
    let missing_approval =
        review_catalog_release_production_eligibility(pool, &machine_only_request)
            .await
            .expect_err("machine validation without an owner reference must not promote a release");
    assert!(matches!(
        missing_approval,
        CatalogReleaseActivationError::InvalidInput(_)
    ));

    let mut forged_machine_report: Value =
        serde_json::from_slice(&validation_report).expect("machine validation JSON must parse");
    forged_machine_report["production_eligible"] = Value::Bool(true);
    machine_only_request.validation_report = serde_json::to_vec(&forged_machine_report)
        .expect("modified validation JSON must serialize");
    machine_only_request.approval_reference = input.approval_reference.to_owned();
    let inferred_eligibility =
        review_catalog_release_production_eligibility(pool, &machine_only_request)
            .await
            .expect_err("machine report cannot assert its own production eligibility");
    assert!(matches!(
        inferred_eligibility,
        CatalogReleaseActivationError::ValidationFailed(_)
    ));

    let review_request = CatalogReleaseProductionEligibilityRequest {
        release_id: input.release_id,
        rollback_target_release_id: input.rollback_target_release_id,
        validation_report,
        impact_report,
        reviewer_id: input.reviewer_id,
        approval_reference: input.approval_reference.to_owned(),
    };
    review_catalog_release_production_eligibility(pool, &review_request)
        .await
        .expect("exact staged release must accept explicit reviewed evidence")
}

async fn build_test_review_artifacts(
    pool: &PgPool,
    input: &TestReviewInput<'_>,
) -> (Vec<u8>, Vec<u8>) {
    let source_sha256 = sha256_hex(input.source_bytes);
    let validation_request = FdcFoundationValidationRequest {
        release_version: input.source_release_version.to_owned(),
        source_published_date: "2026-04-30".to_owned(),
        object_uri: format!("fixture://fdc/{}.json", input.source_release_version),
        source_payload_filename: Some(format!("{}.json", input.source_release_version)),
        source_archive_sha256: Some(source_sha256.clone()),
        expected_sha256: source_sha256,
        reviewed_fdc_ids: vec![input.fdc_id],
        preprocessing_policy_version: None,
    };
    let machine_report = validate_fdc_foundation_json(input.source_bytes, &validation_request);
    assert_eq!(machine_report.validation_status, "passed");
    let machine_json = machine_report.to_json(&validation_request);
    assert_eq!(machine_json["production_eligible"], Value::Bool(false));
    let validation_report = machine_report
        .to_pretty_json(&validation_request)
        .expect("machine validation report must serialize")
        .into_bytes();

    let manifest: Value = sqlx::query_scalar(
        "SELECT manifest FROM catalog.catalog_release WHERE id = $1 AND status = 'staged'",
    )
    .bind(input.release_id)
    .fetch_one(pool)
    .await
    .expect("staged manifest must be readable");
    let release_version_in_manifest: String =
        sqlx::query_scalar("SELECT version FROM catalog.catalog_release WHERE id = $1")
            .bind(input.release_id)
            .fetch_one(pool)
            .await
            .expect("staged release version must be readable");
    assert_eq!(release_version_in_manifest, input.catalog_release_version);
    let source_dataset_release_id = manifest["source_dataset_release_id"]
        .as_str()
        .expect("source dataset ID must exist");
    let selection_sha256 = manifest["selection_sha256"]
        .as_str()
        .expect("selection hash must exist");
    let selected_profile_count: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint
           FROM catalog.catalog_release_profile
          WHERE catalog_release_id = $1",
    )
    .bind(input.release_id)
    .fetch_one(pool)
    .await
    .expect("profile membership must be readable");
    let selected_food_count: i64 = sqlx::query_scalar(
        "SELECT count(DISTINCT profile.food_id)::bigint
           FROM catalog.catalog_release_profile membership
           JOIN composition.composition_profile profile
             ON profile.id = membership.profile_id
          WHERE membership.catalog_release_id = $1",
    )
    .bind(input.release_id)
    .fetch_one(pool)
    .await
    .expect("food membership must be readable");
    let (rollback_target_version, rollback_target_checksum): (String, String) = sqlx::query_as(
        "SELECT version, checksum_sha256 FROM catalog.catalog_release WHERE id = $1",
    )
    .bind(input.rollback_target_release_id)
    .fetch_one(pool)
    .await
    .expect("rollback target identity must be readable");
    let impact_report = serde_json::to_vec_pretty(&json!({
        "schema_version": "m3-catalog-impact-0.1.0",
        "status": "passed",
        "catalog_release_id": input.release_id,
        "catalog_release_version": input.catalog_release_version,
        "source_dataset_release_id": source_dataset_release_id,
        "selection_sha256": selection_sha256,
        "selected_profile_count": selected_profile_count,
        "selected_food_count": selected_food_count,
        "nutrient_mapping_validation": "passed",
        "rollback_target": {
            "catalog_release_id": input.rollback_target_release_id,
            "catalog_release_version": rollback_target_version,
            "manifest_sha256": rollback_target_checksum
        },
        "impact_summary": {"scope": "synthetic integration fixture"},
        "production_eligible": false,
        "clinical_correctness_claimed": false
    }))
    .expect("impact report must serialize");
    (validation_report, impact_report)
}

async fn assert_release_status(pool: &PgPool, release_id: Uuid, expected_status: &str) {
    let status: String =
        sqlx::query_scalar("SELECT status FROM catalog.catalog_release WHERE id = $1")
            .bind(release_id)
            .fetch_one(pool)
            .await
            .expect("catalog release status must be readable");
    assert_eq!(status, expected_status);
}

async fn assert_published_content(pool: &PgPool, release_id: Uuid) {
    let content_state: (i64, i64, i64) = sqlx::query_as(
        "SELECT
            count(*) FILTER (WHERE profile.status = 'published'),
            count(*) FILTER (WHERE food.lifecycle_status = 'active'),
            count(*) FILTER (WHERE mapping.review_status = 'approved')
           FROM catalog.catalog_release_profile membership
           JOIN composition.composition_profile profile
             ON profile.id = membership.profile_id
           JOIN catalog.food_entity food ON food.id = profile.food_id
           JOIN catalog.food_mapping mapping
             ON mapping.food_id = profile.food_id
            AND mapping.source_food_record_id = profile.source_record_id
          WHERE membership.catalog_release_id = $1",
    )
    .bind(release_id)
    .fetch_one(pool)
    .await
    .expect("published content must be readable");
    assert_eq!(content_state, (1, 1, 1));
}

async fn assert_release_membership_counts(pool: &PgPool, release_id: Uuid) {
    let counts: (i64, i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT count(*) FROM catalog.catalog_release_food_name WHERE catalog_release_id = $1),
            (SELECT count(*) FROM catalog.catalog_release_profile WHERE catalog_release_id = $1),
            (SELECT count(*) FROM catalog.catalog_release_portion_observation WHERE catalog_release_id = $1)",
    )
    .bind(release_id)
    .fetch_one(pool)
    .await
    .expect("rollback memberships must be readable");
    assert_eq!(counts, (1, 1, 0));
}

async fn assert_source_activation(
    pool: &PgPool,
    dataset_release_id: Uuid,
    reviewer_id: Uuid,
    expected_reason: &str,
) {
    let state = sqlx::query(
        "SELECT active_release_id, previous_release_id, activated_by, reason
           FROM raw.source_activation
          WHERE active_release_id = $1",
    )
    .bind(dataset_release_id)
    .fetch_one(pool)
    .await
    .expect("source activation pointer must be readable");
    let active_release_id: Uuid = state.try_get("active_release_id").expect("active ID");
    let previous_release_id: Option<Uuid> =
        state.try_get("previous_release_id").expect("previous ID");
    let activated_by: Uuid = state.try_get("activated_by").expect("reviewer ID");
    let reason: String = state.try_get("reason").expect("approval reason");
    assert_eq!(active_release_id, dataset_release_id);
    assert_eq!(previous_release_id, None);
    assert_eq!(activated_by, reviewer_id);
    assert_eq!(reason, expected_reason);
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
