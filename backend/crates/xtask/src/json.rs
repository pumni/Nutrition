use serde_json::Value;
use std::{error::Error, fs, path::Path};

const ARTIFACTS: &[&str] = &[
    "schemas/parsed-meal-0.1.0.json",
    "schemas/vietnamese-meal-bench-annotation-0.1.0.json",
    "schemas/vietnamese-meal-bench-prediction-0.1.0.json",
    "fixtures/vietnamese-meal-bench/manifest.json",
    "fixtures/vietnamese-meal-bench/foundation-cases.json",
];

pub fn run(root: &Path) -> Result<(), Box<dyn Error>> {
    for relative in ARTIFACTS {
        let path = root.join(relative);
        let value: Value = serde_json::from_str(&fs::read_to_string(&path)?).map_err(|error| {
            format!(
                "[json] invalid JSON\npath: {}\nerror: {error}",
                path.display()
            )
        })?;
        if !value.is_object() && !value.is_array() {
            return Err(format!(
                "[json] artifact must be an object or array: {}",
                path.display()
            )
            .into());
        }
    }
    println!("[PASS] JSON artifact syntax ({} files)", ARTIFACTS.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    #[test]
    fn accepts_valid_json() {
        let value: Value = serde_json::from_str("{\"ok\":true}").expect("valid JSON");
        assert!(value.is_object());
    }

    #[test]
    fn rejects_invalid_json() {
        assert!(serde_json::from_str::<Value>("{").is_err());
    }
}
