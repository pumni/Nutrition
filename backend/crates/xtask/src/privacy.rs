use std::{error::Error, fs, path::Path};

const LOG_MACROS: &[&str] = &["info!(", "warn!(", "error!(", "debug!(", "trace!("];
const FORBIDDEN_FIELDS: &[(&str, &str)] = &[
    ("request.text", "raw meal text"),
    ("raw_text", "raw meal payload"),
    ("authorization", "authorization material"),
    ("database_url", "database credentials"),
    ("api_key", "provider credentials"),
    ("response.body", "raw provider response"),
    ("raw_response", "raw provider response"),
];

pub fn run(root: &Path) -> Result<(), Box<dyn Error>> {
    let mut files = Vec::new();
    collect_runtime_rust_files(&root.join("crates"), &mut files)?;
    let mut violations = Vec::new();
    for file in files {
        let content = fs::read_to_string(&file)?;
        violations.extend(find_violations(&file, &content));
    }
    if violations.is_empty() {
        println!("[PASS] sensitive logging/privacy scan");
        Ok(())
    } else {
        Err(format!(
            "[privacy] prohibited logging source detected\n{}\nrule: raw meal, authorization, credential, and raw provider content must never enter logs/telemetry\nfix: log request ID, status, latency, and bounded metadata only\nsee: docs/operations/security.md",
            violations.join("\n")
        )
        .into())
    }
}

pub fn find_violations(path: &Path, content: &str) -> Vec<String> {
    let mut violations = Vec::new();
    for (index, line) in content.lines().enumerate() {
        let lowered = line.to_ascii_lowercase();
        if !LOG_MACROS
            .iter()
            .any(|macro_name| lowered.contains(macro_name))
        {
            continue;
        }
        for (field, description) in FORBIDDEN_FIELDS {
            if lowered.contains(field) {
                violations.push(format!(
                    "{}:{}: {description}: {}",
                    path.display(),
                    index + 1,
                    line.trim()
                ));
            }
        }
    }
    violations
}

fn collect_runtime_rust_files(
    directory: &Path,
    files: &mut Vec<std::path::PathBuf>,
) -> Result<(), Box<dyn Error>> {
    if !directory.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if entry.file_name() == "xtask" {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            collect_runtime_rust_files(&path, files)?;
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
            files.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::find_violations;

    #[test]
    fn accepts_safe_structured_logging() {
        assert!(
            find_violations(
                std::path::Path::new("test.rs"),
                "tracing::info!(status = \"ok\");"
            )
            .is_empty()
        );
    }

    #[test]
    fn rejects_raw_content_logging() {
        let violations = find_violations(
            std::path::Path::new("test.rs"),
            "tracing::info!(raw_text = request.text);",
        );
        assert_eq!(violations.len(), 2);
    }
}
