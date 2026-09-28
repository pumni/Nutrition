use persistence_postgres::{
    FDC_ENERGY_MAPPING_POLICY_VERSION, FDC_FOUNDATION_2026_04_ARCHIVE_SHA256,
    FDC_FOUNDATION_2026_04_EXTRACTED_JSON_SHA256, FDC_FOUNDATION_2026_04_NULL_TAIL_POLICY_VERSION,
    FDC_FOUNDATION_2026_04_RELEASE_VERSION, FdcFoundationImportReport, FdcFoundationImportRequest,
    FdcFoundationValidationRequest, connect, import_fdc_foundation_json, migrate,
    validate_fdc_foundation_json,
};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use std::{env, fs};
use uuid::Uuid;

const OBJECT_URI: &str =
    "https://fdc.nal.usda.gov/fdc-datasets/FoodData_Central_foundation_food_json_2026-04-30.zip";
const PAYLOAD_FILENAME: &str = "FoodData_Central_foundation_food_json_2026-04-30.json";
const REVIEWED_FDC_IDS: [u64; 20] = [
    1_750_339, 1_750_340, 1_750_341, 1_750_342, 1_750_343, 1_999_626, 1_999_627, 1_999_628,
    1_999_629, 1_999_630, 1_999_631, 1_999_632, 1_999_633, 1_999_634, 2_003_586, 2_003_587,
    2_003_588, 2_003_589, 2_003_590, 2_003_591,
];

#[tokio::test]
#[ignore = "requires the pinned USDA archive and extracted JSON in environment variables"]
async fn exact_april_2026_selection_is_staged_and_inactive() {
    let payload_bytes = validated_source_payload();
    let database_url = env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL is required");
    let pool = connect(&database_url, 4)
        .await
        .expect("disposable PostgreSQL must be reachable");
    migrate(&pool).await.expect("catalog migrations must apply");
    let active_before = active_release_id(&pool).await;
    assert_eq!(
        active_before, None,
        "exact M3 import drill must use a fresh disposable database"
    );

    let imported = import_fdc_foundation_json(&pool, &payload_bytes, &build_import_request())
        .await
        .expect("exact selected source must stage");
    assert_import_summary(&imported);
    assert_staged_release_metadata(&pool, &imported).await;
    assert_staged_membership_and_print(&pool, &imported, active_before).await;
}

fn validated_source_payload() -> Vec<u8> {
    let archive_path = env::var("FDC_FOUNDATION_ARCHIVE")
        .expect("FDC_FOUNDATION_ARCHIVE must point to the pinned USDA ZIP");
    let payload_path = env::var("FDC_FOUNDATION_JSON")
        .expect("FDC_FOUNDATION_JSON must point to the extracted USDA JSON");
    let archive_bytes = fs::read(&archive_path).expect("pinned USDA archive must be readable");
    let payload_bytes = fs::read(&payload_path).expect("pinned USDA JSON must be readable");
    assert_eq!(
        sha256_hex(&archive_bytes),
        FDC_FOUNDATION_2026_04_ARCHIVE_SHA256
    );
    assert_eq!(
        sha256_hex(&payload_bytes),
        FDC_FOUNDATION_2026_04_EXTRACTED_JSON_SHA256
    );

    let validation_request = FdcFoundationValidationRequest {
        release_version: FDC_FOUNDATION_2026_04_RELEASE_VERSION.to_owned(),
        source_published_date: "2026-04-30".to_owned(),
        object_uri: OBJECT_URI.to_owned(),
        source_payload_filename: Some(PAYLOAD_FILENAME.to_owned()),
        source_archive_sha256: Some(FDC_FOUNDATION_2026_04_ARCHIVE_SHA256.to_owned()),
        expected_sha256: FDC_FOUNDATION_2026_04_EXTRACTED_JSON_SHA256.to_owned(),
        reviewed_fdc_ids: REVIEWED_FDC_IDS.to_vec(),
        preprocessing_policy_version: Some(
            FDC_FOUNDATION_2026_04_NULL_TAIL_POLICY_VERSION.to_owned(),
        ),
    };
    let validation = validate_fdc_foundation_json(&payload_bytes, &validation_request);
    assert_eq!(validation.validation_status, "passed");
    assert_eq!(validation.selected_record_count, REVIEWED_FDC_IDS.len());
    assert_eq!(
        validation.selection_fingerprint.as_deref(),
        Some("ad867dbbb6a9387c4cb3e3837fb337353097d7ebd99f774eded25cf56dd9ffc2")
    );
    assert_eq!(
        validation.normalized_payload_sha256.as_deref(),
        Some("8af923182f75bce502ba9c14aca2228fd4dad095eb1a8d6a7aba6a2b5101c19d")
    );
    let machine_report = validation.to_json(&validation_request);
    assert_eq!(machine_report["production_eligible"], false);
    assert_eq!(machine_report["release"]["reviewer_approved"], false);
    assert_eq!(machine_report["release"]["activation_attempted"], false);
    payload_bytes
}

fn build_import_request() -> FdcFoundationImportRequest {
    FdcFoundationImportRequest {
        release_version: FDC_FOUNDATION_2026_04_RELEASE_VERSION.to_owned(),
        source_published_date: "2026-04-30".to_owned(),
        object_uri: OBJECT_URI.to_owned(),
        expected_sha256: FDC_FOUNDATION_2026_04_EXTRACTED_JSON_SHA256.to_owned(),
        source_archive_sha256: Some(FDC_FOUNDATION_2026_04_ARCHIVE_SHA256.to_owned()),
        preprocessing_policy_version: Some(
            FDC_FOUNDATION_2026_04_NULL_TAIL_POLICY_VERSION.to_owned(),
        ),
        include_fdc_ids: REVIEWED_FDC_IDS.to_vec(),
        created_by: "0198f100-0000-7000-8000-000000000099".to_owned(),
    }
}

fn assert_import_summary(imported: &FdcFoundationImportReport) {
    assert_eq!(imported.raw_record_count, 363);
    assert_eq!(imported.selected_record_count, REVIEWED_FDC_IDS.len());
    assert_eq!(
        imported.energy_atwater_specific_count,
        REVIEWED_FDC_IDS.len()
    );
    assert_eq!(imported.energy_atwater_general_count, 0);
    assert_eq!(imported.energy_missing_count, 0);
    assert_eq!(imported.unexpected_legacy_energy_count, 0);
}

async fn assert_staged_release_metadata(pool: &PgPool, imported: &FdcFoundationImportReport) {
    let release = sqlx::query(
        "SELECT status, activated_at IS NULL AS inactive, manifest
           FROM catalog.catalog_release WHERE id = $1",
    )
    .bind(imported.catalog_release_id)
    .fetch_one(pool)
    .await
    .expect("staged release must be readable");
    assert_eq!(
        release.try_get::<String, _>("status").expect("status"),
        "staged"
    );
    assert!(release.try_get::<bool, _>("inactive").expect("inactive"));
    let manifest: serde_json::Value = release.try_get("manifest").expect("manifest");
    assert_eq!(manifest["production_eligible"], false);
    assert_eq!(
        manifest["source_archive_sha256"],
        FDC_FOUNDATION_2026_04_ARCHIVE_SHA256
    );
    assert_eq!(
        manifest["source_payload_sha256"],
        FDC_FOUNDATION_2026_04_EXTRACTED_JSON_SHA256
    );
    assert_eq!(manifest["energy_policy"], FDC_ENERGY_MAPPING_POLICY_VERSION);
    assert_eq!(
        manifest["preprocessing_policy_version"],
        FDC_FOUNDATION_2026_04_NULL_TAIL_POLICY_VERSION
    );

    let dataset_release_state: (String, i64, String) = sqlx::query_as(
        "SELECT status, record_count, checksum_sha256
           FROM raw.dataset_release WHERE id = $1",
    )
    .bind(imported.dataset_release_id)
    .fetch_one(pool)
    .await
    .expect("source release evidence must be readable");
    assert_eq!(dataset_release_state.0, "imported");
    assert_eq!(dataset_release_state.1, 363);
    assert_eq!(
        dataset_release_state.2,
        FDC_FOUNDATION_2026_04_EXTRACTED_JSON_SHA256
    );
}

async fn assert_staged_membership_and_print(
    pool: &PgPool,
    imported: &FdcFoundationImportReport,
    active_before: Option<Uuid>,
) {
    let staged_membership = read_staged_membership(pool, imported.catalog_release_id).await;
    let staged_ids = staged_membership
        .iter()
        .map(|membership| membership["fdc_id"].as_u64().expect("FDC ID"))
        .collect::<Vec<_>>();
    assert_eq!(staged_ids, REVIEWED_FDC_IDS);
    let profile_state: (i64, i64, i64) = sqlx::query_as(
        "SELECT count(*)::bigint,
                count(*) FILTER (WHERE profile.method_metadata->>'production_eligible' = 'true')::bigint,
                count(*) FILTER (WHERE profile.method_metadata->'energy_mapping'->>'status' = 'complete')::bigint
           FROM catalog.catalog_release_profile membership
           JOIN composition.composition_profile profile
             ON profile.id = membership.profile_id
          WHERE membership.catalog_release_id = $1",
    )
    .bind(imported.catalog_release_id)
    .fetch_one(pool)
    .await
    .expect("profile validation summary must be readable");
    assert_eq!(profile_state, (20, 0, 20));
    assert_eq!(active_release_id(pool).await, active_before);

    let membership_fingerprint = serde_json::to_vec(&staged_membership)
        .map(|membership| sha256_hex(&membership))
        .expect("membership summary must serialize");
    println!(
        "M3 staged-only evidence: {}",
        serde_json::json!({
            "catalog_release_id": imported.catalog_release_id,
            "catalog_release_version": imported.catalog_release_version,
            "dataset_release_id": imported.dataset_release_id,
            "selected_count": imported.selected_record_count,
            "source_payload_sha256": imported.source_sha256,
            "selection_sha256": "ad867dbbb6a9387c4cb3e3837fb337353097d7ebd99f774eded25cf56dd9ffc2",
            "membership_sha256": membership_fingerprint,
            "selected_membership": staged_membership,
            "active_release_before": active_before,
            "active_release_after": active_release_id(pool).await,
            "production_eligible": false,
            "status": "staged",
            "activation_attempted": false
        })
    );
}

async fn read_staged_membership(pool: &PgPool, release_id: Uuid) -> Vec<serde_json::Value> {
    sqlx::query(
        "SELECT profile.method_metadata->>'fdc_id' AS fdc_id,
                profile.id AS profile_id,
                profile.food_id,
                profile.source_record_id
           FROM catalog.catalog_release_profile membership
           JOIN composition.composition_profile profile
             ON profile.id = membership.profile_id
          WHERE membership.catalog_release_id = $1
          ORDER BY (profile.method_metadata->>'fdc_id')::bigint",
    )
    .bind(release_id)
    .fetch_all(pool)
    .await
    .expect("staged membership must be readable")
    .into_iter()
    .map(|row| {
        let fdc_id = row
            .try_get::<String, _>("fdc_id")
            .expect("selected profile must carry its FDC ID")
            .parse::<u64>()
            .expect("FDC ID must be numeric");
        serde_json::json!({
            "fdc_id": fdc_id,
            "profile_id": row.try_get::<Uuid, _>("profile_id").expect("profile ID"),
            "food_id": row.try_get::<Uuid, _>("food_id").expect("food ID"),
            "source_record_id": row.try_get::<Uuid, _>("source_record_id").expect("source record ID")
        })
    })
    .collect()
}

async fn active_release_id(pool: &PgPool) -> Option<Uuid> {
    sqlx::query_scalar("SELECT id FROM catalog.catalog_release WHERE status = 'active'")
        .fetch_optional(pool)
        .await
        .expect("active release pointer must be readable")
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
