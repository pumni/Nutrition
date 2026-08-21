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
    for (start, end) in log_macro_ranges(content) {
        let invocation = &content[start..end];
        let lowered = invocation.to_ascii_lowercase();
        let line = content[..start]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            + 1;
        for (field, description) in FORBIDDEN_FIELDS {
            if lowered.contains(field) {
                violations.push(format!(
                    "{}:{}: {description}: {}",
                    path.display(),
                    line,
                    invocation.trim().replace(['\r', '\n'], " ")
                ));
            }
        }
    }
    violations
}

fn log_macro_ranges(content: &str) -> Vec<(usize, usize)> {
    let lowered = content.to_ascii_lowercase();
    let mut ranges = Vec::new();
    for macro_name in LOG_MACROS {
        let mut cursor = 0;
        while let Some(relative) = lowered[cursor..].find(macro_name) {
            let start = cursor + relative;
            let open = start + macro_name.len() - 1;
            if let Some(end) = balanced_parenthesis_end(content, open) {
                ranges.push((start, end));
                cursor = end;
            } else {
                cursor = open.saturating_add(1);
            }
        }
    }
    ranges.sort_unstable_by_key(|(start, _)| *start);
    ranges
}

fn balanced_parenthesis_end(content: &str, open: usize) -> Option<usize> {
    let bytes = content.as_bytes();
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (index, byte) in bytes.iter().enumerate().skip(open) {
        if in_string {
            if escaped {
                escaped = false;
            } else if *byte == b'\\' {
                escaped = true;
            } else if *byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match *byte {
            b'"' => in_string = true,
            b'(' => depth += 1,
            b')' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(index + 1);
                }
            }
            _ => {}
        }
    }
    None
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

    #[test]
    fn rejects_sensitive_fields_across_multiline_invocation() {
        let violations = find_violations(
            std::path::Path::new("test.rs"),
            "tracing::info!(\n    raw_text = request.text,\n    \"parser failure\"\n);",
        );
        assert_eq!(violations.len(), 2);
    }

    #[test]
    fn ignores_sensitive_words_outside_logging_invocation() {
        let violations = find_violations(
            std::path::Path::new("test.rs"),
            "let raw_text = request.text;\ntracing::info!(status = \"ok\");",
        );
        assert!(violations.is_empty());
    }
}
