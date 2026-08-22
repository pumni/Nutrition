use super::model::{
    CATALOG_HANDOFF_CONTRACT_VERSION, CATALOG_HANDOFF_PROFILE, CatalogHandoffImportError,
    CatalogHandoffImportRequest, CompositionValue, DatasetRelease, FoodConcept, FoodName,
    LoadedPackage, Manifest, RawSourceRecord, SourceFoodMapping, SourceReleases,
};
use jsonschema::validator_for;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
};

#[allow(clippy::too_many_lines)]
pub(crate) fn load_package(
    request: &CatalogHandoffImportRequest,
) -> Result<LoadedPackage, CatalogHandoffImportError> {
    let root = request
        .package_path
        .canonicalize()
        .map_err(|error| CatalogHandoffImportError::UnsafePackagePath(error.to_string()))?;
    if !root.is_dir() {
        return Err(CatalogHandoffImportError::UnsafePackagePath(
            "package path must resolve to a directory".to_owned(),
        ));
    }
    let manifest_path = root.join("manifest.json");
    let manifest_bytes = fs::read(&manifest_path)
        .map_err(|_| CatalogHandoffImportError::MissingFile("manifest.json".to_owned()))?;
    let manifest_value: Value = serde_json::from_slice(&manifest_bytes)?;
    let contract_version = manifest_value
        .get("contract_version")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            CatalogHandoffImportError::Schema("manifest.contract_version is required".to_owned())
        })?;
    if contract_version != CATALOG_HANDOFF_CONTRACT_VERSION {
        return Err(CatalogHandoffImportError::UnsupportedContractVersion(
            contract_version.to_owned(),
        ));
    }
    let profile = manifest_value
        .get("handoff_profile")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if profile != CATALOG_HANDOFF_PROFILE {
        return Err(CatalogHandoffImportError::UnsupportedProfile(
            profile.to_owned(),
        ));
    }
    reject_unknown_roles(&manifest_value)?;
    validate_manifest_paths(&manifest_value)?;
    validate_value(&manifest_value, "manifest.schema.json")?;
    let manifest: Manifest = serde_json::from_value(manifest_value.clone())?;
    if manifest.files.len() != 7 {
        return Err(CatalogHandoffImportError::UnexpectedFile(
            "manifest must declare exactly seven payload files".to_owned(),
        ));
    }
    validate_manifest_entries(&manifest)?;
    validate_package_directory(&root, &manifest)?;
    let package_sha256 = verify_checksums(&root, &manifest)?;

    let dataset_value = read_json_file(&root, "dataset-release.json")?;
    validate_value(&dataset_value, "dataset-release.schema.json")?;
    let dataset_release: DatasetRelease = serde_json::from_value(dataset_value)?;

    let source_value = read_json_file(&root, "source-releases.json")?;
    let source_array = source_value
        .get("sources")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            CatalogHandoffImportError::Schema("source-releases.sources must be an array".to_owned())
        })?;
    for source in source_array {
        validate_value(source, "source-release.schema.json")?;
    }
    let source_releases: SourceReleases = serde_json::from_value(source_value)?;
    let raw_records = read_jsonl::<RawSourceRecord>(
        &root,
        &manifest,
        "raw_source_records",
        "raw-source-record.schema.json",
        "raw-source-records.jsonl",
    )?;
    let food_concepts = read_jsonl::<FoodConcept>(
        &root,
        &manifest,
        "food_concepts",
        "food-concept.schema.json",
        "food-concepts.jsonl",
    )?;
    let food_names = read_jsonl::<FoodName>(
        &root,
        &manifest,
        "food_names",
        "food-name.schema.json",
        "food-names.jsonl",
    )?;
    let mappings = read_jsonl::<SourceFoodMapping>(
        &root,
        &manifest,
        "source_food_mappings",
        "source-food-mapping.schema.json",
        "source-food-mappings.jsonl",
    )?;
    let compositions = read_jsonl::<CompositionValue>(
        &root,
        &manifest,
        "composition_values",
        "composition-value.schema.json",
        "composition-values.jsonl",
    )?;

    Ok(LoadedPackage {
        manifest,
        manifest_value,
        manifest_bytes,
        package_sha256,
        dataset_release,
        source_releases,
        raw_records,
        food_concepts,
        food_names,
        mappings,
        compositions,
    })
}

fn reject_unknown_roles(value: &Value) -> Result<(), CatalogHandoffImportError> {
    let Some(files) = value.get("files").and_then(Value::as_array) else {
        return Ok(());
    };
    let mut roles = BTreeSet::new();
    for file in files {
        let role = file.get("role").and_then(Value::as_str).unwrap_or_default();
        if !matches!(
            role,
            "dataset_release"
                | "source_releases"
                | "raw_source_records"
                | "food_concepts"
                | "food_names"
                | "source_food_mappings"
                | "composition_values"
        ) {
            return Err(CatalogHandoffImportError::UnexpectedFile(role.to_owned()));
        }
        if !roles.insert(role.to_owned()) {
            return Err(CatalogHandoffImportError::UnexpectedFile(format!(
                "duplicate file role {role}"
            )));
        }
    }
    Ok(())
}

fn validate_manifest_paths(value: &Value) -> Result<(), CatalogHandoffImportError> {
    let Some(files) = value.get("files").and_then(Value::as_array) else {
        return Ok(());
    };
    for file in files {
        if let Some(path) = file.get("path").and_then(Value::as_str) {
            validate_relative_path(path)?;
        }
    }
    Ok(())
}

fn validate_manifest_entries(manifest: &Manifest) -> Result<(), CatalogHandoffImportError> {
    let expected: BTreeMap<&str, (&str, &str)> = BTreeMap::from([
        (
            "dataset_release",
            ("dataset-release.json", "dataset-release"),
        ),
        (
            "source_releases",
            ("source-releases.json", "source-release"),
        ),
        (
            "raw_source_records",
            ("raw-source-records.jsonl", "raw-source-record"),
        ),
        ("food_concepts", ("food-concepts.jsonl", "food-concept")),
        ("food_names", ("food-names.jsonl", "food-name")),
        (
            "source_food_mappings",
            ("source-food-mappings.jsonl", "source-food-mapping"),
        ),
        (
            "composition_values",
            ("composition-values.jsonl", "composition-value"),
        ),
    ]);
    let mut paths = BTreeSet::new();
    for entry in &manifest.files {
        if !expected.contains_key(entry.role.as_str()) {
            return Err(CatalogHandoffImportError::UnexpectedFile(
                entry.role.clone(),
            ));
        }
        let (path, schema) = expected[entry.role.as_str()];
        if entry.path != path || entry.schema != schema {
            return Err(CatalogHandoffImportError::UnexpectedFile(format!(
                "role {} has unexpected path or schema",
                entry.role
            )));
        }
        if !paths.insert(entry.path.clone()) {
            return Err(CatalogHandoffImportError::UnexpectedFile(format!(
                "duplicate path {}",
                entry.path
            )));
        }
        if !is_sha256(&entry.sha256) {
            return Err(CatalogHandoffImportError::Schema(format!(
                "invalid SHA-256 for {}",
                entry.path
            )));
        }
    }
    if paths.len() != expected.len() {
        return Err(CatalogHandoffImportError::UnexpectedFile(
            "manifest does not declare the exact v1 payload set".to_owned(),
        ));
    }
    Ok(())
}

fn validate_package_directory(
    root: &Path,
    manifest: &Manifest,
) -> Result<(), CatalogHandoffImportError> {
    let mut expected = BTreeSet::from(["manifest.json".to_owned(), "checksums.sha256".to_owned()]);
    expected.extend(manifest.files.iter().map(|entry| entry.path.clone()));
    for entry in
        fs::read_dir(root).map_err(|error| CatalogHandoffImportError::Io(error.to_string()))?
    {
        let entry = entry.map_err(|error| CatalogHandoffImportError::Io(error.to_string()))?;
        if entry
            .file_type()
            .map_err(|error| CatalogHandoffImportError::Io(error.to_string()))?
            .is_symlink()
        {
            return Err(CatalogHandoffImportError::UnsafePackagePath(
                entry.path().display().to_string(),
            ));
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if !expected.contains(&name) {
            return Err(CatalogHandoffImportError::UnexpectedFile(name));
        }
    }
    for path in expected {
        let resolved = resolve_file(root, &path)?;
        if !resolved.is_file() {
            return Err(CatalogHandoffImportError::MissingFile(path));
        }
    }
    Ok(())
}

fn verify_checksums(root: &Path, manifest: &Manifest) -> Result<String, CatalogHandoffImportError> {
    let checksum_path = resolve_file(root, "checksums.sha256")?;
    let checksum_bytes = fs::read(&checksum_path)
        .map_err(|error| CatalogHandoffImportError::Io(error.to_string()))?;
    let text = std::str::from_utf8(&checksum_bytes).map_err(|_| {
        CatalogHandoffImportError::Schema("checksums.sha256 must be UTF-8".to_owned())
    })?;
    let mut declared = BTreeMap::new();
    for line in text.lines() {
        let Some((hash, path)) = line.split_once("  ") else {
            return Err(CatalogHandoffImportError::Schema(
                "invalid checksum line".to_owned(),
            ));
        };
        if !is_sha256(hash) || path == "checksums.sha256" || path.is_empty() {
            return Err(CatalogHandoffImportError::Schema(format!(
                "invalid checksum entry {path}"
            )));
        }
        validate_relative_path(path)?;
        if declared.insert(path.to_owned(), hash.to_owned()).is_some() {
            return Err(CatalogHandoffImportError::Schema(format!(
                "duplicate checksum entry {path}"
            )));
        }
    }
    let mut expected = BTreeSet::from(["manifest.json".to_owned()]);
    expected.extend(manifest.files.iter().map(|entry| entry.path.clone()));
    if declared.keys().cloned().collect::<BTreeSet<_>>() != expected {
        return Err(CatalogHandoffImportError::Schema(
            "checksums.sha256 does not cover exactly manifest and payload files".to_owned(),
        ));
    }
    for (path, expected_hash) in declared {
        let bytes = fs::read(resolve_file(root, &path)?)
            .map_err(|error| CatalogHandoffImportError::Io(error.to_string()))?;
        let actual = sha256_hex(&bytes);
        if actual != expected_hash {
            return Err(CatalogHandoffImportError::ChecksumMismatch {
                path,
                expected: expected_hash,
                actual,
            });
        }
        if path != "manifest.json" {
            let entry = manifest
                .files
                .iter()
                .find(|entry| entry.path == path)
                .expect("manifest entry checked above");
            if entry.sha256 != actual {
                return Err(CatalogHandoffImportError::ChecksumMismatch {
                    path,
                    expected: entry.sha256.clone(),
                    actual,
                });
            }
            if entry.size_bytes != bytes.len() as u64 {
                return Err(CatalogHandoffImportError::Semantic(format!(
                    "{} declares size {}, actual {}",
                    entry.path,
                    entry.size_bytes,
                    bytes.len()
                )));
            }
        }
    }
    Ok(sha256_hex(&checksum_bytes))
}

fn read_json_file(root: &Path, path: &str) -> Result<Value, CatalogHandoffImportError> {
    let bytes = fs::read(resolve_file(root, path)?)
        .map_err(|error| CatalogHandoffImportError::Io(error.to_string()))?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn read_jsonl<T: for<'de> serde::Deserialize<'de>>(
    root: &Path,
    manifest: &Manifest,
    role: &str,
    schema: &str,
    path: &str,
) -> Result<Vec<T>, CatalogHandoffImportError> {
    let bytes = fs::read(resolve_file(root, path)?)
        .map_err(|error| CatalogHandoffImportError::Io(error.to_string()))?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| CatalogHandoffImportError::Schema(format!("{path} must be UTF-8")))?;
    let entry = manifest
        .files
        .iter()
        .find(|entry| entry.role == role)
        .ok_or_else(|| CatalogHandoffImportError::MissingFile(path.to_owned()))?;
    let mut values = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            return Err(CatalogHandoffImportError::Schema(format!(
                "{path} contains a blank line at {}",
                index + 1
            )));
        }
        let value: Value = serde_json::from_str(line).map_err(|error| {
            CatalogHandoffImportError::Schema(format!("{path} line {}: {error}", index + 1))
        })?;
        validate_value(&value, schema)?;
        values.push(serde_json::from_value(value)?);
    }
    if values.len() != entry.record_count {
        return Err(CatalogHandoffImportError::Semantic(format!(
            "{path} declares {} records, found {}",
            entry.record_count,
            values.len()
        )));
    }
    Ok(values)
}

fn validate_value(value: &Value, schema_file: &str) -> Result<(), CatalogHandoffImportError> {
    let schema_text = match schema_file {
        "manifest.schema.json" => {
            include_str!("../../../../../contracts/catalog-handoff/v1/manifest.schema.json")
        }
        "dataset-release.schema.json" => {
            include_str!("../../../../../contracts/catalog-handoff/v1/dataset-release.schema.json")
        }
        "source-release.schema.json" => {
            include_str!("../../../../../contracts/catalog-handoff/v1/source-release.schema.json")
        }
        "raw-source-record.schema.json" => include_str!(
            "../../../../../contracts/catalog-handoff/v1/raw-source-record.schema.json"
        ),
        "food-concept.schema.json" => {
            include_str!("../../../../../contracts/catalog-handoff/v1/food-concept.schema.json")
        }
        "food-name.schema.json" => {
            include_str!("../../../../../contracts/catalog-handoff/v1/food-name.schema.json")
        }
        "source-food-mapping.schema.json" => include_str!(
            "../../../../../contracts/catalog-handoff/v1/source-food-mapping.schema.json"
        ),
        "composition-value.schema.json" => include_str!(
            "../../../../../contracts/catalog-handoff/v1/composition-value.schema.json"
        ),
        other => {
            return Err(CatalogHandoffImportError::Schema(format!(
                "unknown schema {other}"
            )));
        }
    };
    let schema: Value = serde_json::from_str(schema_text)?;
    let validator = validator_for(&schema)
        .map_err(|error| CatalogHandoffImportError::Schema(error.to_string()))?;
    validator
        .validate(value)
        .map_err(|error| CatalogHandoffImportError::Schema(error.to_string()))
}

fn resolve_file(root: &Path, relative: &str) -> Result<PathBuf, CatalogHandoffImportError> {
    validate_relative_path(relative)?;
    let candidate = root.join(relative);
    let resolved = candidate
        .canonicalize()
        .map_err(|_| CatalogHandoffImportError::MissingFile(relative.to_owned()))?;
    if !resolved.starts_with(root) {
        return Err(CatalogHandoffImportError::UnsafePackagePath(
            relative.to_owned(),
        ));
    }
    Ok(resolved)
}

fn validate_relative_path(path: &str) -> Result<(), CatalogHandoffImportError> {
    if path.is_empty() || Path::new(path).is_absolute() || path.contains('\\') || path.contains(':')
    {
        return Err(CatalogHandoffImportError::UnsafePackagePath(
            path.to_owned(),
        ));
    }
    if Path::new(path).components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(CatalogHandoffImportError::UnsafePackagePath(
            path.to_owned(),
        ));
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value.bytes().all(|byte| byte.is_ascii_hexdigit())
        && value == value.to_ascii_lowercase()
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
