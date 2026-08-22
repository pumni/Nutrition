use persistence_postgres::{CatalogHandoffImportError, validate_catalog_handoff_v1_package};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../contracts/catalog-handoff/v1/fixtures/minimal-valid")
}

fn copy_fixture(name: &str) -> (tempfile_like::TempDir, PathBuf) {
    let temporary = tempfile_like::TempDir::new(name);
    let target = temporary.path().join("package");
    fs::create_dir_all(&target).expect("temporary package directory must be creatable");
    for entry in fs::read_dir(fixture()).expect("fixture must be readable") {
        let entry = entry.expect("fixture entry");
        fs::copy(entry.path(), target.join(entry.file_name())).expect("fixture file must copy");
    }
    (temporary, target)
}

fn refresh_package_integrity(package: &Path) {
    let manifest_path = package.join("manifest.json");
    let mut manifest: Value = serde_json::from_slice(&fs::read(&manifest_path).expect("manifest"))
        .expect("manifest JSON");
    for entry in manifest["files"].as_array_mut().expect("manifest files") {
        let path = entry["path"].as_str().expect("manifest path");
        let bytes = fs::read(package.join(path)).expect("payload");
        entry["sha256"] = Value::String(hex::encode(Sha256::digest(&bytes)));
        entry["size_bytes"] = Value::from(bytes.len());
    }
    let manifest_bytes = [
        serde_json::to_vec_pretty(&manifest).expect("manifest serialization"),
        b"\n".to_vec(),
    ]
    .concat();
    fs::write(&manifest_path, &manifest_bytes).expect("manifest write");
    refresh_checksums(package);
}

fn refresh_checksums(package: &Path) {
    let manifest: Value =
        serde_json::from_slice(&fs::read(package.join("manifest.json")).expect("manifest"))
            .expect("manifest JSON");
    let mut paths = vec!["manifest.json".to_owned()];
    paths.extend(
        manifest["files"]
            .as_array()
            .expect("manifest files")
            .iter()
            .map(|entry| entry["path"].as_str().expect("manifest path").to_owned()),
    );
    paths.sort();
    let checksums = paths
        .into_iter()
        .map(|path| {
            let hash = hex::encode(Sha256::digest(fs::read(package.join(&path)).expect("file")));
            format!("{hash}  {path}")
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    fs::write(package.join("checksums.sha256"), checksums).expect("checksums");
}

fn mutate_jsonl_record(
    package: &Path,
    filename: &str,
    index: usize,
    mutate: impl FnOnce(&mut Value),
) {
    let path = package.join(filename);
    let mut lines = fs::read_to_string(&path)
        .expect("JSONL")
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let mut value: Value = serde_json::from_str(&lines[index]).expect("JSONL object");
    mutate(&mut value);
    lines[index] = serde_json::to_string(&value).expect("JSONL serialization");
    fs::write(path, format!("{}\n", lines.join("\n"))).expect("JSONL write");
}

fn assert_schema_or_semantic(error: &CatalogHandoffImportError) {
    assert!(matches!(
        error,
        CatalogHandoffImportError::Schema(_)
            | CatalogHandoffImportError::Semantic(_)
            | CatalogHandoffImportError::UnsupportedProfile(_)
    ));
}

#[test]
fn canonical_fixture_is_accepted_by_the_rust_consumer() {
    let report =
        validate_catalog_handoff_v1_package(&fixture()).expect("golden fixture must validate");
    assert_eq!(report.contract_version, "catalog-handoff-1.0.0");
    assert_eq!(report.selected_record_count, 20);
    assert_eq!(report.composition_value_count, 80);
}

#[test]
fn unsupported_version_fails_before_checksum_processing() {
    let (_temporary, package) = copy_fixture("unsupported-version");
    let manifest_path = package.join("manifest.json");
    let manifest = fs::read_to_string(&manifest_path).expect("manifest");
    fs::write(
        &manifest_path,
        manifest.replace("catalog-handoff-1.0.0", "catalog-handoff-2.0.0"),
    )
    .expect("mutate manifest");
    let error = validate_catalog_handoff_v1_package(&package)
        .expect_err("unsupported version must fail closed");
    assert!(matches!(
        error,
        CatalogHandoffImportError::UnsupportedContractVersion(_)
    ));
}

#[test]
fn payload_tampering_is_a_typed_checksum_failure() {
    let (_temporary, package) = copy_fixture("tamper");
    let path = package.join("food-names.jsonl");
    let mut bytes = fs::read(&path).expect("payload");
    bytes.push(b' ');
    fs::write(path, bytes).expect("tamper payload");
    let error = validate_catalog_handoff_v1_package(&package)
        .expect_err("tampered package must fail closed");
    assert!(matches!(
        error,
        CatalogHandoffImportError::ChecksumMismatch { .. }
    ));
}

#[test]
fn traversal_and_missing_files_fail_closed() {
    let (_temporary, package) = copy_fixture("path-safety");
    let manifest_path = package.join("manifest.json");
    let manifest = fs::read_to_string(&manifest_path).expect("manifest");
    fs::write(
        &manifest_path,
        manifest.replace("food-names.jsonl", "../food-names.jsonl"),
    )
    .expect("mutate manifest path");
    let error =
        validate_catalog_handoff_v1_package(&package).expect_err("path traversal must fail closed");
    assert!(matches!(
        error,
        CatalogHandoffImportError::UnsafePackagePath(_)
    ));

    let (_temporary, package) = copy_fixture("missing-file");
    fs::remove_file(package.join("food-names.jsonl")).expect("remove temporary fixture file");
    let error = validate_catalog_handoff_v1_package(&package)
        .expect_err("missing payload must fail closed");
    assert!(matches!(error, CatalogHandoffImportError::MissingFile(_)));
}

#[test]
fn unsupported_profile_and_unsafe_flags_fail_closed() {
    let (_temporary, package) = copy_fixture("profile");
    let manifest_path = package.join("manifest.json");
    let mut manifest: Value = serde_json::from_slice(&fs::read(&manifest_path).expect("manifest"))
        .expect("manifest JSON");
    manifest["handoff_profile"] = Value::String("other-profile".to_owned());
    manifest["production_eligible"] = Value::Bool(true);
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).expect("manifest"),
    )
    .expect("manifest write");
    refresh_checksums(&package);
    let error =
        validate_catalog_handoff_v1_package(&package).expect_err("unsafe profile must fail");
    assert_schema_or_semantic(&error);
}

#[test]
fn production_flags_selection_and_file_roles_fail_closed() {
    for (field, value) in [
        ("production_eligible", Value::Bool(true)),
        ("activation_authorized", Value::Bool(true)),
    ] {
        let (_temporary, package) = copy_fixture(field);
        let manifest_path = package.join("manifest.json");
        let mut manifest: Value =
            serde_json::from_slice(&fs::read(&manifest_path).expect("manifest"))
                .expect("manifest JSON");
        manifest[field] = value;
        fs::write(
            &manifest_path,
            [
                serde_json::to_vec_pretty(&manifest).expect("manifest serialization"),
                b"\n".to_vec(),
            ]
            .concat(),
        )
        .expect("manifest write");
        let error = validate_catalog_handoff_v1_package(&package)
            .expect_err("production-controlled flags must fail");
        assert!(matches!(error, CatalogHandoffImportError::Schema(_)));
    }

    let (_temporary, package) = copy_fixture("auto-add");
    let manifest_path = package.join("manifest.json");
    let mut manifest: Value = serde_json::from_slice(&fs::read(&manifest_path).expect("manifest"))
        .expect("manifest JSON");
    manifest["selection"]["auto_add_records"] = Value::Bool(true);
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).expect("manifest serialization"),
    )
    .expect("manifest write");
    let error =
        validate_catalog_handoff_v1_package(&package).expect_err("auto-add must fail closed");
    assert!(matches!(error, CatalogHandoffImportError::Schema(_)));

    let (_temporary, package) = copy_fixture("unknown-role");
    let manifest_path = package.join("manifest.json");
    let mut manifest: Value = serde_json::from_slice(&fs::read(&manifest_path).expect("manifest"))
        .expect("manifest JSON");
    manifest["files"][0]["role"] = Value::String("unknown_role".to_owned());
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).expect("manifest serialization"),
    )
    .expect("manifest write");
    let error =
        validate_catalog_handoff_v1_package(&package).expect_err("unknown role must fail closed");
    assert!(matches!(
        error,
        CatalogHandoffImportError::UnexpectedFile(_)
    ));
}

#[test]
fn unsafe_path_invalid_sha_and_wrong_size_fail_closed() {
    let (_temporary, package) = copy_fixture("absolute-path");
    let manifest_path = package.join("manifest.json");
    let mut manifest: Value = serde_json::from_slice(&fs::read(&manifest_path).expect("manifest"))
        .expect("manifest JSON");
    manifest["files"][0]["path"] = Value::String("C:/outside.json".to_owned());
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).expect("manifest serialization"),
    )
    .expect("manifest write");
    let error =
        validate_catalog_handoff_v1_package(&package).expect_err("absolute path must fail closed");
    assert!(matches!(
        error,
        CatalogHandoffImportError::UnsafePackagePath(_)
    ));

    let (_temporary, package) = copy_fixture("invalid-sha");
    let manifest_path = package.join("manifest.json");
    let mut manifest: Value = serde_json::from_slice(&fs::read(&manifest_path).expect("manifest"))
        .expect("manifest JSON");
    manifest["files"][0]["sha256"] = Value::String("z".repeat(64));
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).expect("manifest serialization"),
    )
    .expect("manifest write");
    let error =
        validate_catalog_handoff_v1_package(&package).expect_err("invalid SHA must fail closed");
    assert!(matches!(error, CatalogHandoffImportError::Schema(_)));

    let (_temporary, package) = copy_fixture("wrong-size");
    let manifest_path = package.join("manifest.json");
    let mut manifest: Value = serde_json::from_slice(&fs::read(&manifest_path).expect("manifest"))
        .expect("manifest JSON");
    manifest["files"][0]["size_bytes"] = Value::from(0);
    fs::write(
        &manifest_path,
        [
            serde_json::to_vec_pretty(&manifest).expect("manifest serialization"),
            b"\n".to_vec(),
        ]
        .concat(),
    )
    .expect("manifest write");
    refresh_checksums(&package);
    let error = validate_catalog_handoff_v1_package(&package)
        .expect_err("wrong declared size must fail closed");
    assert!(matches!(error, CatalogHandoffImportError::Semantic(_)));
}

#[test]
fn malformed_json_and_selection_mismatch_fail_closed() {
    let (_temporary, package) = copy_fixture("malformed-json");
    fs::write(package.join("dataset-release.json"), b"{not-json}\n").expect("malformed JSON");
    refresh_package_integrity(&package);
    let error =
        validate_catalog_handoff_v1_package(&package).expect_err("malformed JSON must fail closed");
    assert!(matches!(error, CatalogHandoffImportError::Json(_)));

    let (_temporary, package) = copy_fixture("selection-mismatch");
    let manifest_path = package.join("manifest.json");
    let mut manifest: Value = serde_json::from_slice(&fs::read(&manifest_path).expect("manifest"))
        .expect("manifest JSON");
    manifest["selection"]["selection_sha256"] = Value::String("c".repeat(64));
    fs::write(
        &manifest_path,
        [
            serde_json::to_vec_pretty(&manifest).expect("manifest serialization"),
            b"\n".to_vec(),
        ]
        .concat(),
    )
    .expect("manifest write");
    refresh_checksums(&package);
    let error = validate_catalog_handoff_v1_package(&package)
        .expect_err("selection mismatch must fail closed");
    assert!(matches!(error, CatalogHandoffImportError::Semantic(_)));
}

#[test]
fn provenance_files_must_agree() {
    let (_temporary, package) = copy_fixture("provenance");
    let dataset_path = package.join("dataset-release.json");
    let mut dataset: Value =
        serde_json::from_slice(&fs::read(&dataset_path).expect("dataset")).expect("dataset JSON");
    dataset["artifact_sha256"] = Value::String("c".repeat(64));
    fs::write(
        &dataset_path,
        [
            serde_json::to_vec_pretty(&dataset).expect("dataset"),
            b"\n".to_vec(),
        ]
        .concat(),
    )
    .expect("dataset write");
    refresh_package_integrity(&package);
    let error = validate_catalog_handoff_v1_package(&package)
        .expect_err("conflicting provenance must fail");
    assert!(matches!(error, CatalogHandoffImportError::Semantic(_)));

    for rights_state in ["reference_only", "prohibited", "unknown", ""] {
        let (_temporary, package) = copy_fixture("rights");
        let path = package.join("source-releases.json");
        let mut source: Value = serde_json::from_slice(&fs::read(&path).expect("source releases"))
            .expect("source release JSON");
        source["sources"][0]["rights_state"] = Value::String(rights_state.to_owned());
        fs::write(
            &path,
            [
                serde_json::to_vec_pretty(&source).expect("source serialization"),
                b"\n".to_vec(),
            ]
            .concat(),
        )
        .expect("source release write");
        refresh_package_integrity(&package);
        let error = validate_catalog_handoff_v1_package(&package)
            .expect_err("unsafe rights state must fail closed");
        assert!(matches!(
            error,
            CatalogHandoffImportError::Schema(_) | CatalogHandoffImportError::Semantic(_)
        ));
    }
}

#[test]
fn names_and_compositions_must_be_grounded_in_raw_payloads() {
    let (_temporary, package) = copy_fixture("grounding");
    mutate_jsonl_record(&package, "food-names.jsonl", 0, |value| {
        value["name"] = Value::String("Ungrounded name".to_owned());
    });
    refresh_package_integrity(&package);
    let error =
        validate_catalog_handoff_v1_package(&package).expect_err("ungrounded name must fail");
    assert!(matches!(
        error,
        CatalogHandoffImportError::ReferenceIntegrity(_)
    ));

    let (_temporary, package) = copy_fixture("composition-grounding");
    mutate_jsonl_record(&package, "composition-values.jsonl", 0, |value| {
        value["value"] = Value::from(999.0);
    });
    refresh_package_integrity(&package);
    let error = validate_catalog_handoff_v1_package(&package)
        .expect_err("ungrounded composition must fail");
    assert!(matches!(
        error,
        CatalogHandoffImportError::ReferenceIntegrity(_)
    ));

    let (_temporary, package) = copy_fixture("method-grounding");
    mutate_jsonl_record(&package, "composition-values.jsonl", 0, |value| {
        value["source_method"] = Value::String("made_up_method".to_owned());
    });
    refresh_package_integrity(&package);
    let error = validate_catalog_handoff_v1_package(&package)
        .expect_err("ungrounded source method must fail");
    assert!(matches!(
        error,
        CatalogHandoffImportError::ReferenceIntegrity(_)
    ));
}

#[test]
fn malformed_jsonl_duplicate_source_and_dangling_reference_fail_closed() {
    let (_temporary, package) = copy_fixture("malformed-jsonl");
    fs::write(package.join("food-names.jsonl"), b"{not-json}\n").expect("malformed JSONL");
    refresh_package_integrity(&package);
    let error =
        validate_catalog_handoff_v1_package(&package).expect_err("malformed JSONL must fail");
    assert!(matches!(error, CatalogHandoffImportError::Schema(_)));

    let (_temporary, package) = copy_fixture("duplicate-source");
    mutate_jsonl_record(&package, "raw-source-records.jsonl", 1, |value| {
        value["source_id"] = value["source_id"].clone();
        value["source_id"] = Value::String("1750339".to_owned());
    });
    refresh_package_integrity(&package);
    let error =
        validate_catalog_handoff_v1_package(&package).expect_err("duplicate source must fail");
    assert!(matches!(
        error,
        CatalogHandoffImportError::ReferenceIntegrity(_)
    ));

    let (_temporary, package) = copy_fixture("dangling-name");
    mutate_jsonl_record(&package, "food-names.jsonl", 0, |value| {
        value["source_id"] = Value::String("999999999".to_owned());
    });
    refresh_package_integrity(&package);
    let error = validate_catalog_handoff_v1_package(&package).expect_err("dangling name must fail");
    assert!(matches!(
        error,
        CatalogHandoffImportError::ReferenceIntegrity(_)
    ));
}

#[test]
fn wrong_jsonl_count_nutrient_shape_and_unit_fail_closed() {
    let (_temporary, package) = copy_fixture("wrong-count");
    let path = package.join("food-names.jsonl");
    let mut lines = fs::read_to_string(&path)
        .expect("names")
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    lines.pop();
    fs::write(path, format!("{}\n", lines.join("\n"))).expect("short JSONL");
    refresh_package_integrity(&package);
    let error =
        validate_catalog_handoff_v1_package(&package).expect_err("wrong JSONL count must fail");
    assert!(matches!(error, CatalogHandoffImportError::Semantic(_)));

    let (_temporary, package) = copy_fixture("unsupported-nutrient");
    mutate_jsonl_record(&package, "composition-values.jsonl", 0, |value| {
        value["target_code"] = Value::String("calcium_mg".to_owned());
    });
    refresh_package_integrity(&package);
    let error =
        validate_catalog_handoff_v1_package(&package).expect_err("unsupported nutrient must fail");
    assert!(matches!(error, CatalogHandoffImportError::Schema(_)));

    let (_temporary, package) = copy_fixture("invalid-unit");
    mutate_jsonl_record(&package, "composition-values.jsonl", 0, |value| {
        value["source_unit"] = Value::String("MG".to_owned());
    });
    refresh_package_integrity(&package);
    let error = validate_catalog_handoff_v1_package(&package).expect_err("invalid unit must fail");
    assert!(matches!(error, CatalogHandoffImportError::Schema(_)));
}

mod tempfile_like {
    use std::{
        fs,
        path::{Path, PathBuf},
    };

    pub struct TempDir(PathBuf);

    impl TempDir {
        pub fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "nutrition-catalog-handoff-{label}-{}",
                uuid::Uuid::now_v7()
            ));
            fs::create_dir_all(&path).expect("temporary directory");
            Self(path)
        }
        pub fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
