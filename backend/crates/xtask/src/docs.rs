use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
};

pub fn run(root: &Path) -> Result<(), Box<dyn Error>> {
    let mut markdown = Vec::new();
    collect_markdown(root, &mut markdown)?;
    let mut broken = Vec::new();
    for source in markdown {
        let content = fs::read_to_string(&source)?;
        for target in markdown_targets(&content) {
            if target.starts_with("http://")
                || target.starts_with("https://")
                || target.starts_with('#')
            {
                continue;
            }
            let path = target.split('#').next().unwrap_or_default();
            if path.is_empty() {
                continue;
            }
            let resolved = source.parent().unwrap_or(root).join(path);
            if !resolved.is_file() && !resolved.is_dir() {
                broken.push(format!("{} -> {}", source.display(), target));
            }
        }
    }
    if broken.is_empty() {
        println!("[PASS] active Markdown links");
        Ok(())
    } else {
        Err(format!(
            "[docs] broken active Markdown link(s)\n{}\nrule: active documentation must route only to current repository paths",
            broken.join("\n")
        ).into())
    }
}

fn collect_markdown(directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), Box<dyn Error>> {
    if directory.file_name().is_some_and(|name| {
        name == "target" || name == ".git" || name == "docs" && directory.ends_with("archive")
    }) {
        return Ok(());
    }
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if path
                .file_name()
                .is_some_and(|name| name == "target" || name == ".git" || name == "archive")
            {
                continue;
            }
            collect_markdown(&path, files)?;
        } else if path.extension().and_then(|value| value.to_str()) == Some("md") {
            files.push(path);
        }
    }
    Ok(())
}

fn markdown_targets(content: &str) -> Vec<String> {
    content
        .split("](")
        .skip(1)
        .filter_map(|part| part.split(')').next())
        .map(|target| target.trim_matches('<').trim_matches('>').to_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::markdown_targets;

    #[test]
    fn extracts_relative_links() {
        assert_eq!(
            markdown_targets("[current](docs/index.md) [web](https://example.com)"),
            vec!["docs/index.md", "https://example.com"]
        );
    }
}
