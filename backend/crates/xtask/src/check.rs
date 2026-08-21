use std::{error::Error, path::Path};

use crate::{architecture, benchmark, docs, json, migrations, privacy, process};

pub fn run(root: &Path) -> Result<(), Box<dyn Error>> {
    process::run(root, "cargo", &["fmt", "--all", "--", "--check"])?;
    process::run(
        root,
        "cargo",
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
    )?;
    process::run(
        root,
        "cargo",
        &["test", "--workspace", "--", "--test-threads=1"],
    )?;
    json::run(root)?;
    benchmark::run(root)?;
    architecture::run(root)?;
    privacy::run(root)?;
    migrations::run(root, false)?;
    docs::run(root)?;
    if process::command_available("docker") {
        process::run(
            root,
            "docker",
            &["compose", "-f", "deploy/compose.yaml", "config", "--quiet"],
        )?;
    } else {
        println!("[SKIP] Docker Compose config (docker is unavailable locally; CI requires it)");
    }
    println!("[PASS] xtask check");
    Ok(())
}
