use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fs,
    path::Path,
};

pub fn run(root: &Path) -> Result<(), Box<dyn Error>> {
    let crates = runtime_crates(root)?;
    let mut violations = Vec::new();
    for (name, manifest) in &crates {
        let content = fs::read_to_string(manifest)?;
        for violation in violations_for_manifest(name, &content) {
            violations.push(format!(
                "[architecture] violation\ncrate: {name}\nforbidden dependency: {violation}\nexpected: {}\nsource: {}\nsee: docs/architecture/index.md#dependency-direction",
                expected_rule(name),
                manifest.display()
            ));
        }
    }
    if violations.is_empty() {
        println!("[PASS] architecture dependency direction");
        Ok(())
    } else {
        Err(violations.join("\n\n").into())
    }
}

pub fn violations_for_manifest(crate_name: &str, manifest: &str) -> Vec<String> {
    let dependencies = dependency_names(manifest);
    forbidden_dependencies(crate_name)
        .iter()
        .filter(|dependency| dependencies.contains(**dependency))
        .map(|dependency| (*dependency).to_owned())
        .collect()
}

fn runtime_crates(root: &Path) -> Result<BTreeMap<String, std::path::PathBuf>, Box<dyn Error>> {
    let mut crates = BTreeMap::new();
    for entry in fs::read_dir(root.join("crates"))? {
        let entry = entry?;
        let manifest = entry.path().join("Cargo.toml");
        if !manifest.is_file() || entry.file_name() == "xtask" {
            continue;
        }
        let name = package_name(&fs::read_to_string(&manifest)?)
            .ok_or_else(|| format!("missing package.name in {}", manifest.display()))?;
        crates.insert(name, manifest);
    }
    Ok(crates)
}

fn package_name(manifest: &str) -> Option<String> {
    let mut in_package = false;
    for raw_line in manifest.lines() {
        let line = raw_line.trim();
        if line.starts_with('[') {
            in_package = line == "[package]";
        } else if in_package && line.starts_with("name") {
            return line
                .split('=')
                .nth(1)
                .map(|value| value.trim().trim_matches('"').to_owned());
        }
    }
    None
}

pub fn dependency_names(manifest: &str) -> BTreeSet<String> {
    let mut dependencies = BTreeSet::new();
    let mut in_dependencies = false;
    for raw_line in manifest.lines() {
        let line = raw_line.trim();
        if line.starts_with('[') {
            in_dependencies = matches!(
                line,
                "[dependencies]" | "[dev-dependencies]" | "[build-dependencies]"
            );
            continue;
        }
        if in_dependencies
            && !line.is_empty()
            && !line.starts_with('#')
            && let Some((name, _)) = line.split_once('=')
        {
            dependencies.insert(name.trim().trim_end_matches(".workspace").to_owned());
        }
    }
    dependencies
}

fn forbidden_dependencies(crate_name: &str) -> &'static [&'static str] {
    match crate_name {
        "domain" => &[
            "application",
            "adapters",
            "persistence-postgres",
            "api-http",
            "worker",
            "axum",
            "sqlx",
            "tokio",
            "reqwest",
            "jsonwebtoken",
            "metrics",
            "tracing",
        ],
        "application" => &["adapters", "persistence-postgres", "api-http", "worker"],
        "adapters" | "persistence-postgres" => &["api-http", "worker"],
        _ => &[],
    }
}

fn expected_rule(crate_name: &str) -> &'static str {
    match crate_name {
        "domain" => "domain must remain transport/runtime independent",
        "application" => "application must not depend on outer concrete runtime crates",
        "adapters" | "persistence-postgres" => {
            "outer adapters must not depend on API/worker composition crates"
        }
        _ => "workspace dependency direction must remain inward",
    }
}

#[cfg(test)]
mod tests {
    use super::{dependency_names, violations_for_manifest};

    #[test]
    fn parses_workspace_dependency_forms() {
        let dependencies =
            dependency_names("[dependencies]\ndomain.workspace = true\nserde = \"1\"");
        assert!(dependencies.contains("domain"));
        assert!(dependencies.contains("serde"));
    }

    #[test]
    fn rejects_forbidden_domain_dependency_in_fixture() {
        let violations = violations_for_manifest(
            "domain",
            "[dependencies]\nsqlx.workspace = true\nserde = \"1\"",
        );
        assert_eq!(violations, vec!["sqlx"]);
    }

    #[test]
    fn allows_domain_foundation_dependencies() {
        let violations = violations_for_manifest("domain", "[dependencies]\nserde = \"1\"");
        assert!(violations.is_empty());
    }
}
