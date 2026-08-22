use persistence_postgres::{
    CatalogHandoffImportError, CatalogHandoffImportRequest, connect, import_catalog_handoff_v1,
    migrate,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use std::{env, fs, path::PathBuf};
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and PostgreSQL 18"]
async fn catalog_handoff_stages_replays_and_rejects_same_release_conflicts() {
    let pool = setup_database().await;
    let request = CatalogHandoffImportRequest {
        package_path: fixture(),
        created_by: "0198f100-0000-7000-8000-000000000099".to_owned(),
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
        },
    )
    .await
    .expect_err("same release identity with different content must conflict");
    assert!(matches!(
        conflict,
        CatalogHandoffImportError::ReleaseConflict(_)
    ));
}

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../contracts/catalog-handoff/v1/fixtures/minimal-valid")
}

async fn setup_database() -> PgPool {
    let url = env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL is required");
    let pool = connect(&url, 4).await.expect("database must connect");
    migrate(&pool).await.expect("migrations must apply");
    pool
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
            (SELECT count(*) FROM raw.source_food_record record JOIN raw.dataset_release dataset_release ON dataset_release.id = record.dataset_release_id WHERE dataset_release.version = '2026-04-30'),
            (SELECT count(*) FROM catalog.catalog_release_food_name WHERE catalog_release_id = $1),
            (SELECT count(*) FROM catalog.catalog_release_profile WHERE catalog_release_id = $1),
            (SELECT count(*) FROM composition.composition_value value JOIN composition.composition_profile profile ON profile.id = value.profile_id JOIN catalog.catalog_release_profile membership ON membership.profile_id = profile.id WHERE membership.catalog_release_id = $1)",
    ).bind(release_id).fetch_one(pool).await.expect("staged content counts");
    assert!(counts.0 >= 20);
    assert_eq!(counts.1, 20);
    assert_eq!(counts.2, 20);
    assert_eq!(counts.3, 4);
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
    let payload_path = package.join("food-names.jsonl");
    let mut lines = fs::read_to_string(&payload_path)
        .expect("names payload")
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let mut first: Value = serde_json::from_str(&lines[0]).expect("name JSON");
    first["name"] = Value::String("Synthetic handoff fixture changed".to_owned());
    lines[0] = serde_json::to_string(&first).expect("name JSON serialization");
    let payload = format!("{}\n", lines.join("\n"));
    fs::write(&payload_path, payload.as_bytes()).expect("mutate names payload");

    let manifest_path = package.join("manifest.json");
    let mut manifest: Value = serde_json::from_slice(&fs::read(&manifest_path).expect("manifest"))
        .expect("manifest JSON");
    let hash = hex::encode(Sha256::digest(payload.as_bytes()));
    for file in manifest["files"].as_array_mut().expect("manifest files") {
        if file["path"] == "food-names.jsonl" {
            file["sha256"] = Value::String(hash.clone());
            file["size_bytes"] = Value::from(payload.len());
        }
    }
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
