use sha2::{Digest, Sha256};
use std::{error::Error, fs, path::Path};

#[derive(Debug, Eq, PartialEq)]
struct Entry {
    path: String,
    hash: String,
}

pub fn run(root: &Path, record_new: bool) -> Result<(), Box<dyn Error>> {
    let manifest_path = root.join("migrations/manifest.sha256");
    let files = migration_files(root)?;
    validate_migration_files(&files)?;
    if record_new {
        record_new_entries(root, &manifest_path)?;
    }
    let entries = parse_manifest(&fs::read_to_string(&manifest_path)?)?;
    verify_entries(root, &entries)?;
    println!(
        "[PASS] migration manifest integrity ({} entries)",
        entries.len()
    );
    Ok(())
}

fn record_new_entries(root: &Path, manifest_path: &Path) -> Result<(), Box<dyn Error>> {
    let mut entries = parse_manifest(&fs::read_to_string(manifest_path)?)?;
    let known = entries
        .iter()
        .map(|entry| entry.path.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let mut added = Vec::new();
    for file in migration_files(root)? {
        let path = file
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("invalid migration filename")?;
        if !known.contains(path) {
            let hash = digest(&file)?;
            entries.push(Entry {
                path: path.to_owned(),
                hash: hash.clone(),
            });
            added.push(format!("{path} {hash}"));
        }
    }
    if !added.is_empty() {
        let mut content = fs::read_to_string(manifest_path)?;
        if !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(&added.join("\n"));
        content.push('\n');
        fs::write(manifest_path, content)?;
        println!("[migrations] recorded {} new migration(s)", added.len());
    }
    Ok(())
}

fn verify_entries(root: &Path, entries: &[Entry]) -> Result<(), Box<dyn Error>> {
    let mut previous: Option<&str> = None;
    for entry in entries {
        if let Some(previous) = previous
            && entry.path.as_str() <= previous
        {
            return Err(format!(
                "[migrations] manifest entries must be strictly ordered: {}",
                entry.path
            )
            .into());
        }
        previous = Some(entry.path.as_str());
        validate_migration_name(Path::new(&entry.path))?;
        let path = root.join("migrations").join(&entry.path);
        if !path.is_file() {
            return Err(format!(
                "[migrations] recorded migration is missing: {}",
                path.display()
            )
            .into());
        }
        let actual = digest(&path)?;
        if actual != entry.hash {
            return Err(format!("[migrations] checksum mismatch\npath: {}\nexpected: {}\nactual: {}\nrule: committed migrations are immutable; add a forward migration instead", path.display(), entry.hash, actual).into());
        }
    }
    Ok(())
}

fn parse_manifest(content: &str) -> Result<Vec<Entry>, Box<dyn Error>> {
    let mut entries = Vec::new();
    for line in content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let mut fields = line.split_whitespace();
        let path = fields.next().ok_or("migration manifest path is missing")?;
        let hash = fields.next().ok_or("migration manifest hash is missing")?;
        if fields.next().is_some()
            || hash.len() != 64
            || !hash.chars().all(|character| character.is_ascii_hexdigit())
        {
            return Err(format!("invalid migration manifest entry: {line}").into());
        }
        entries.push(Entry {
            path: path.to_owned(),
            hash: hash.to_ascii_lowercase(),
        });
    }
    if entries.is_empty() {
        return Err("migration manifest is empty".into());
    }
    Ok(entries)
}

fn migration_files(root: &Path) -> Result<Vec<std::path::PathBuf>, Box<dyn Error>> {
    let mut files = fs::read_dir(root.join("migrations"))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|extension| extension.to_str()) == Some("sql"))
        .collect::<Vec<_>>();
    files.sort();
    Ok(files)
}

fn validate_migration_files(files: &[std::path::PathBuf]) -> Result<(), Box<dyn Error>> {
    let mut previous_number = None;
    for file in files {
        let number = validate_migration_name(file)?;
        if let Some(previous) = previous_number
            && number <= previous
        {
            return Err(format!(
                "[migrations] SQL migration order must increase strictly: {}",
                file.display()
            )
            .into());
        }
        previous_number = Some(number);
    }
    Ok(())
}

fn validate_migration_name(path: &Path) -> Result<u32, Box<dyn Error>> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            format!(
                "[migrations] invalid migration filename: {}",
                path.display()
            )
        })?;
    let stem = name
        .strip_suffix(".sql")
        .ok_or_else(|| format!("[migrations] migration must use .sql: {name}"))?;
    let (number, description) = stem
        .split_once('_')
        .ok_or_else(|| format!("[migrations] migration must be NNNN_description.sql: {name}"))?;
    if number.len() != 4 || !number.chars().all(|character| character.is_ascii_digit()) {
        return Err(
            format!("[migrations] migration number must be exactly four digits: {name}").into(),
        );
    }
    if description.is_empty()
        || !description.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
    {
        return Err(format!(
            "[migrations] migration description must use lowercase snake_case: {name}"
        )
        .into());
    }
    Ok(number.parse()?)
}

fn digest(path: &Path) -> Result<String, Box<dyn Error>> {
    let mut hasher = Sha256::new();
    hasher.update(fs::read(path)?);
    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::{parse_manifest, validate_migration_files, validate_migration_name};
    use std::path::PathBuf;

    #[test]
    fn parses_comments_and_entries() {
        let entries = parse_manifest("# path sha256\n0001.sql abcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcd\n").expect("manifest");
        assert_eq!(entries[0].path, "0001.sql");
    }

    #[test]
    fn rejects_malformed_entries() {
        assert!(parse_manifest("0001.sql nope\n").is_err());
    }

    #[test]
    fn validates_forward_only_migration_names_and_order() {
        let files = vec![
            PathBuf::from("0001_foundations.sql"),
            PathBuf::from("0014_new_policy.sql"),
        ];
        assert!(validate_migration_files(&files).is_ok());
        assert!(validate_migration_name(PathBuf::from("0015_Bad.sql").as_path()).is_err());
    }

    #[test]
    fn rejects_duplicate_or_out_of_order_migrations() {
        let files = vec![
            PathBuf::from("0002_second.sql"),
            PathBuf::from("0001_first.sql"),
        ];
        assert!(validate_migration_files(&files).is_err());
    }
}
