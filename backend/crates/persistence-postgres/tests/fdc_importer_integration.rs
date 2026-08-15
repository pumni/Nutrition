use persistence_postgres::{
    FdcFoundationImportError, FdcFoundationImportReport, FdcFoundationImportRequest, connect,
    import_fdc_foundation_json, migrate,
};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::env;
use uuid::Uuid;

const FOUNDATION_FIXTURE: &str = r#"{
  "FoundationFoods": [
    {
      "fdcId": 900000001,
      "dataType": "Foundation",
      "description": "Synthetic foundation apple, raw",
      "foodNutrients": [
        {"amount": 0.30, "min": 0.20, "max": 0.40, "nutrient": {"id": 1003, "unitName": "G"}, "foodNutrientDerivation": {"code": "ANALYTICAL"}},
        {"amount": 0.20, "nutrient": {"id": 1004, "unitName": "G"}, "foodNutrientDerivation": {"code": "ANALYTICAL"}},
        {"amount": 13.80, "nutrient": {"id": 1005, "unitName": "G"}, "foodNutrientDerivation": {"code": "CALCULATED"}},
        {"amount": 52.0, "nutrient": {"id": 2048, "unitName": "KCAL"}}
      ]
    },
    {
      "fdcId": 900000002,
      "dataType": "Foundation",
      "description": "Synthetic foundation pear, raw",
      "foodNutrients": [
        {"amount": 0.40, "nutrient": {"id": 1003, "unitName": "G"}},
        {"amount": 0.10, "nutrient": {"id": 1004, "unitName": "G"}},
        {"amount": 15.20, "nutrient": {"id": 1005, "unitName": "G"}},
        {"amount": 57.0, "nutrient": {"id": 2047, "unitName": "KCAL"}}
      ]
    }
  ]
}"#;

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and PostgreSQL 18"]
async fn fdc_import_is_release_pinned_idempotent_and_non_publishing() {
    let pool = setup_database().await;
    let release_version = format!("integration-{}", Uuid::now_v7());
    let request = build_import_request(&release_version);

    let report = import_fdc_foundation_json(&pool, FOUNDATION_FIXTURE.as_bytes(), &request)
        .await
        .expect("staged FDC import must succeed");
    assert_initial_import(&report);
    assert_dataset_release(&pool, &report).await;
    assert_raw_records(&pool, &report).await;
    assert_staged_catalog(&pool, &report).await;
    assert_staged_profile(&pool, &report).await;
    assert_idempotent_replay(&pool, &request, &report).await;
    assert_release_conflict(&pool, &request).await;
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
    FdcFoundationImportRequest {
        release_version: release_version.to_owned(),
        source_published_date: "2026-04-30".to_owned(),
        object_uri: format!("fixture://fdc/{release_version}.json"),
        expected_sha256: hex::encode(Sha256::digest(FOUNDATION_FIXTURE.as_bytes())),
        include_fdc_ids: vec![900_000_001],
        created_by: "0198f100-0000-7000-8000-000000000098".to_owned(),
    }
}

fn assert_initial_import(report: &FdcFoundationImportReport) {
    assert!(!report.replayed);
    assert_eq!(report.raw_record_count, 2);
    assert_eq!(report.selected_record_count, 1);
}

async fn assert_dataset_release(pool: &PgPool, report: &FdcFoundationImportReport) {
    let release_state: (String, i64, String, String) = sqlx::query_as(
        "SELECT status, record_count, checksum_sha256, schema_fingerprint
           FROM raw.dataset_release
          WHERE id = $1",
    )
    .bind(report.dataset_release_id)
    .fetch_one(pool)
    .await
    .expect("dataset release must be readable");
    assert_eq!(release_state.0, "imported");
    assert_eq!(release_state.1, 2);
    assert_eq!(release_state.2, report.source_sha256.as_str());
    assert_eq!(release_state.3, report.schema_fingerprint.as_str());
}

async fn assert_raw_records(pool: &PgPool, report: &FdcFoundationImportReport) {
    let raw_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM raw.source_food_record WHERE dataset_release_id = $1",
    )
    .bind(report.dataset_release_id)
    .fetch_one(pool)
    .await
    .expect("raw source records must be readable");
    assert_eq!(raw_count, 2);
}

async fn assert_staged_catalog(pool: &PgPool, report: &FdcFoundationImportReport) {
    let catalog_state: (String, bool, bool) = sqlx::query_as(
        "SELECT status,
                activated_at IS NULL,
                (manifest->>'production_eligible')::boolean
           FROM catalog.catalog_release
          WHERE id = $1",
    )
    .bind(report.catalog_release_id)
    .fetch_one(pool)
    .await
    .expect("catalog release must be readable");
    assert_eq!(catalog_state.0, "staged");
    assert!(catalog_state.1);
    assert!(!catalog_state.2);

    let membership_counts: (i64, i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT count(*) FROM catalog.catalog_release_food_name WHERE catalog_release_id = $1),
            (SELECT count(*) FROM catalog.catalog_release_profile WHERE catalog_release_id = $1),
            (SELECT count(*) FROM catalog.catalog_release_portion_observation WHERE catalog_release_id = $1)",
    )
    .bind(report.catalog_release_id)
    .fetch_one(pool)
    .await
    .expect("catalog memberships must be readable");
    assert_eq!(membership_counts, (1, 1, 0));
}

async fn assert_staged_profile(pool: &PgPool, report: &FdcFoundationImportReport) {
    let profile_state: (String, String, i64, i64) = sqlx::query_as(
        "SELECT p.status,
                p.quality_grade,
                count(v.*),
                count(v.*) FILTER (WHERE n.code = 'energy_kcal')
           FROM catalog.catalog_release_profile rp
           JOIN composition.composition_profile p ON p.id = rp.profile_id
           LEFT JOIN composition.composition_value v ON v.profile_id = p.id
           LEFT JOIN composition.nutrient n ON n.id = v.nutrient_id
          WHERE rp.catalog_release_id = $1
          GROUP BY p.status, p.quality_grade",
    )
    .bind(report.catalog_release_id)
    .fetch_one(pool)
    .await
    .expect("staged profile must be readable");
    assert_eq!(profile_state.0, "in_review");
    assert_eq!(profile_state.1, "U");
    assert_eq!(profile_state.2, 3);
    assert_eq!(profile_state.3, 0);
}

async fn assert_idempotent_replay(
    pool: &PgPool,
    request: &FdcFoundationImportRequest,
    report: &FdcFoundationImportReport,
) {
    let replay = import_fdc_foundation_json(pool, FOUNDATION_FIXTURE.as_bytes(), request)
        .await
        .expect("identical import must replay safely");
    assert!(replay.replayed);
    assert_eq!(replay.dataset_release_id, report.dataset_release_id);
    assert_eq!(replay.catalog_release_id, report.catalog_release_id);
}

async fn assert_release_conflict(pool: &PgPool, request: &FdcFoundationImportRequest) {
    let conflicting_fixture = FOUNDATION_FIXTURE.replace(
        "Synthetic foundation apple, raw",
        "Synthetic foundation apple, changed",
    );
    let conflicting_request = FdcFoundationImportRequest {
        expected_sha256: hex::encode(Sha256::digest(conflicting_fixture.as_bytes())),
        ..request.clone()
    };
    let error =
        import_fdc_foundation_json(pool, conflicting_fixture.as_bytes(), &conflicting_request)
            .await
            .expect_err("same release version with different artifact must fail closed");
    assert!(matches!(
        error,
        FdcFoundationImportError::ReleaseConflict(_)
    ));
}
