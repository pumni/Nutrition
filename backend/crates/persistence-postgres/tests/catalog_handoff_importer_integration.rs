use persistence_postgres::{
    CatalogHandoffImportCapability, CatalogHandoffImportError, CatalogHandoffImportRequest,
    FdcFoundationImportRequest, connect, import_catalog_handoff_v1, import_fdc_foundation_json,
    migrate,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use std::{env, fs, path::PathBuf};
use uuid::Uuid;

const LEGACY_FIXTURE: &str = r#"{
  "FoundationFoods": [
    {
      "fdcId": 1750339,
      "dataType": "Foundation",
      "description": "Synthetic handoff fixture 1750339",
      "foodNutrients": [
        {"amount": 1.0, "nutrient": {"id": 1003, "unitName": "G"}},
        {"amount": 2.0, "nutrient": {"id": 1004, "unitName": "G"}},
        {"amount": 3.0, "nutrient": {"id": 1005, "unitName": "G"}},
        {"amount": 40.0, "nutrient": {"id": 2048, "unitName": "KCAL"}}
      ]
    }
  ]
}"#;

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and PostgreSQL 18"]
async fn catalog_handoff_stages_replays_and_rejects_same_release_conflicts() {
    let pool = setup_database().await;
    let denied = import_catalog_handoff_v1(
        &pool,
        &CatalogHandoffImportRequest {
            package_path: fixture(),
            created_by: "0198f100-0000-7000-8000-000000000099".to_owned(),
            capability: CatalogHandoffImportCapability::Production,
        },
    )
    .await
    .expect_err("test fixture must require an explicit test capability");
    assert!(matches!(denied, CatalogHandoffImportError::Policy(_)));

    let request = CatalogHandoffImportRequest {
        package_path: fixture(),
        created_by: "0198f100-0000-7000-8000-000000000099".to_owned(),
        capability: CatalogHandoffImportCapability::TestFixture,
    };
    let first = import_catalog_handoff_v1(&pool, &request)
        .await
        .expect("golden handoff fixture must stage");
    assert!(!first.replayed);
    assert_staged_content(&pool, &first.catalog_release_id).await;

    let replay = import_catalog_handoff_v1(&pool, &request)
        .await
        .expect("identical handoff must replay");
    assert!(replay.replayed);
    assert_eq!(replay.dataset_release_id, first.dataset_release_id);
    assert_eq!(replay.catalog_release_id, first.catalog_release_id);

    let temporary = copy_fixture("conflict");
    mutate_valid_payload(&temporary);
    let conflict = import_catalog_handoff_v1(
        &pool,
        &CatalogHandoffImportRequest {
            package_path: temporary,
            created_by: request.created_by.clone(),
            capability: CatalogHandoffImportCapability::TestFixture,
        },
    )
    .await
    .expect_err("same release identity with different content must conflict");
    assert!(matches!(
        conflict,
        CatalogHandoffImportError::ReleaseConflict(_)
    ));
    assert_transition_orders(&pool).await;
}

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../contracts/catalog-handoff/v1/fixtures/minimal-valid")
}

async fn setup_database() -> PgPool {
    let url = env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL is required");
    let pool = connect(&url, 4).await.expect("database must connect");
    migrate(&pool).await.expect("migrations must apply");
    reset_database(&pool).await;
    pool
}

async fn reset_database(pool: &PgPool) {
    sqlx::query(
        "TRUNCATE raw.dataset, catalog.catalog_release, catalog.food_entity, composition.nutrient CASCADE",
    )
    .execute(pool)
    .await
    .expect("test database reset");
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and PostgreSQL 18"]
async fn catalog_handoff_rolls_back_partial_staging() {
    let pool = setup_database().await;
    let food_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO catalog.food_entity (id, semantic_key, entity_kind, lifecycle_status)
         VALUES ($1, 'usda-fdc:1750339', 'basic_food', 'draft')",
    )
    .bind(food_id)
    .execute(&pool)
    .await
    .expect("pre-existing food");
    sqlx::query(
        "INSERT INTO catalog.food_name (id, food_id, locale, name, normalized_name, name_type)
         VALUES ($1, $2, 'en-US', 'Existing preferred name', 'existing preferred name', 'preferred')",
    )
    .bind(Uuid::now_v7())
    .bind(food_id)
    .execute(&pool)
    .await
    .expect("pre-existing preferred name");

    let error = import_catalog_handoff_v1(
        &pool,
        &CatalogHandoffImportRequest {
            package_path: fixture(),
            created_by: "0198f100-0000-7000-8000-000000000099".to_owned(),
            capability: CatalogHandoffImportCapability::TestFixture,
        },
    )
    .await
    .expect_err("database conflict must roll back the package");
    assert!(matches!(error, CatalogHandoffImportError::Semantic(_)));
    let staged_rows: (i64, i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT count(*) FROM raw.dataset WHERE code = 'usda_fdc_foundation'),
            (SELECT count(*) FROM raw.source_food_record),
            (SELECT count(*) FROM catalog.catalog_release)",
    )
    .fetch_one(&pool)
    .await
    .expect("rollback counts");
    assert_eq!(staged_rows, (0, 0, 0));
}

async fn assert_staged_content(pool: &PgPool, release_id: &Uuid) {
    let release = sqlx::query("SELECT status, activated_at IS NULL AS inactive, manifest->>'production_eligible' AS eligible FROM catalog.catalog_release WHERE id = $1")
        .bind(release_id).fetch_one(pool).await.expect("release must exist");
    let status: String = release.try_get("status").expect("status");
    let inactive: bool = release.try_get("inactive").expect("activation state");
    let eligible: String = release.try_get("eligible").expect("production flag");
    assert_eq!(status, "staged");
    assert!(inactive);
    assert_eq!(eligible, "false");
    let counts: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT count(*) FROM raw.source_food_record),
            (SELECT count(*) FROM catalog.catalog_release_food_name WHERE catalog_release_id = $1),
            (SELECT count(*) FROM catalog.catalog_release_profile WHERE catalog_release_id = $1),
            (SELECT count(*) FROM composition.composition_value value JOIN composition.composition_profile profile ON profile.id = value.profile_id JOIN catalog.catalog_release_profile membership ON membership.profile_id = profile.id WHERE membership.catalog_release_id = $1)",
    ).bind(release_id).fetch_one(pool).await.expect("staged content counts");
    assert!(counts.0 >= 20);
    assert_eq!(counts.1, 20);
    assert_eq!(counts.2, 20);
    assert_eq!(counts.3, 80);
}

async fn assert_transition_orders(pool: &PgPool) {
    import_legacy_release(pool, "handoff-first").await;
    let codes: Vec<String> = sqlx::query_scalar("SELECT code FROM raw.dataset ORDER BY code")
        .fetch_all(pool)
        .await
        .expect("dataset codes");
    assert!(codes.iter().any(|code| code == "usda_fdc"));
    assert!(codes.iter().any(|code| code == "synthetic_fixture"));
    assert_overlap_identity(pool).await;

    reset_database(pool).await;

    import_legacy_release(pool, "legacy-first").await;
    let handoff = import_catalog_handoff_v1(
        pool,
        &CatalogHandoffImportRequest {
            package_path: fixture(),
            created_by: "0198f100-0000-7000-8000-000000000099".to_owned(),
            capability: CatalogHandoffImportCapability::TestFixture,
        },
    )
    .await
    .expect("handoff after legacy must stage");
    assert!(!handoff.replayed);
    assert_staged_content(pool, &handoff.catalog_release_id).await;
    let codes: Vec<String> = sqlx::query_scalar("SELECT code FROM raw.dataset ORDER BY code")
        .fetch_all(pool)
        .await
        .expect("dataset codes");
    assert!(codes.iter().any(|code| code == "usda_fdc"));
    assert!(codes.iter().any(|code| code == "synthetic_fixture"));
    assert_overlap_identity(pool).await;
}

async fn assert_overlap_identity(pool: &PgPool) {
    let source_records: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM raw.source_food_record WHERE external_id = '1750339'",
    )
    .fetch_one(pool)
    .await
    .expect("overlapping source records");
    assert_eq!(source_records, 2);
    let entity_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM catalog.food_entity WHERE semantic_key = 'usda-fdc:1750339'",
    )
    .fetch_one(pool)
    .await
    .expect("overlapping food entity");
    assert_eq!(entity_count, 1);
    let preferred_names: (i64, String) = sqlx::query_as(
        "SELECT count(*), min(name) FROM catalog.food_name
          WHERE food_id = (SELECT id FROM catalog.food_entity WHERE semantic_key = 'usda-fdc:1750339')
            AND locale = 'en-US' AND name_type = 'preferred' AND valid_to IS NULL",
    )
    .fetch_one(pool)
    .await
    .expect("overlapping preferred name");
    assert_eq!(preferred_names.0, 1);
    assert_eq!(preferred_names.1, "Synthetic handoff fixture 1750339");
    let staged_releases: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM catalog.catalog_release WHERE status = 'staged' AND activated_at IS NULL",
    )
    .fetch_one(pool)
    .await
    .expect("staged transition releases");
    assert_eq!(staged_releases, 2);
}

async fn import_legacy_release(pool: &PgPool, label: &str) {
    let release_version = "2026-04-30".to_owned();
    let request = FdcFoundationImportRequest {
        release_version: release_version.clone(),
        source_published_date: "2026-04-30".to_owned(),
        object_uri: format!("fixture://legacy/{label}/2026-04-30.json"),
        expected_sha256: hex::encode(Sha256::digest(LEGACY_FIXTURE.as_bytes())),
        source_archive_sha256: None,
        preprocessing_policy_version: None,
        include_fdc_ids: vec![1_750_339],
        created_by: "0198f100-0000-7000-8000-000000000098".to_owned(),
    };
    import_fdc_foundation_json(pool, LEGACY_FIXTURE.as_bytes(), &request)
        .await
        .expect("legacy transition release must import");
}

fn copy_fixture(label: &str) -> PathBuf {
    let target = env::temp_dir().join(format!(
        "nutrition-catalog-handoff-{label}-{}",
        Uuid::now_v7()
    ));
    fs::create_dir_all(&target).expect("temporary package directory");
    for entry in fs::read_dir(fixture()).expect("fixture directory") {
        let entry = entry.expect("fixture entry");
        fs::copy(entry.path(), target.join(entry.file_name())).expect("copy fixture");
    }
    target
}

fn mutate_valid_payload(package: &PathBuf) {
    let manifest_path = package.join("manifest.json");
    let mut manifest: Value = serde_json::from_slice(&fs::read(&manifest_path).expect("manifest"))
        .expect("manifest JSON");
    manifest["backend_baseline"] = Value::String("different-valid-baseline".to_owned());
    let manifest_bytes = serde_json::to_vec_pretty(&manifest).expect("manifest serialization");
    let manifest_bytes = [manifest_bytes, b"\n".to_vec()].concat();
    fs::write(&manifest_path, &manifest_bytes).expect("mutate manifest");
    let mut entries = vec![(
        "manifest.json".to_owned(),
        Sha256::digest(&manifest_bytes).to_vec(),
    )];
    for entry in fs::read_dir(package).expect("package") {
        let entry = entry.expect("package entry");
        let name = entry.file_name().to_string_lossy().into_owned();
        if name != "checksums.sha256" && name != "manifest.json" {
            entries.push((
                name,
                Sha256::digest(fs::read(entry.path()).expect("payload")).to_vec(),
            ));
        }
    }
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    let checksums = entries
        .into_iter()
        .map(|(name, hash)| format!("{}  {name}", hex::encode(hash)))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    fs::write(package.join("checksums.sha256"), checksums).expect("checksums");
}
