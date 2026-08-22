use persistence_postgres::{CatalogHandoffImportError, validate_catalog_handoff_v1_package};
use std::{fs, path::PathBuf};

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

#[test]
fn canonical_fixture_is_accepted_by_the_rust_consumer() {
    let report =
        validate_catalog_handoff_v1_package(&fixture()).expect("golden fixture must validate");
    assert_eq!(report.contract_version, "catalog-handoff-1.0.0");
    assert_eq!(report.selected_record_count, 20);
    assert_eq!(report.composition_value_count, 4);
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
