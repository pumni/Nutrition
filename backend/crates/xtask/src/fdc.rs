use std::{env, error::Error, path::Path};

use crate::process::{command_available, run_owned};

const TEST_DATABASE_URL: &str = "postgres://nutrition:nutrition@127.0.0.1:5432/nutrition";

pub fn run(root: &Path) -> Result<(), Box<dyn Error>> {
    if !command_available("docker") {
        return Err("[fdc] Docker is required for staged importer verification".into());
    }
    let run_exact_m3_drill = exact_m3_drill_requested()?;
    let project = format!("xtask-fdc-{}", std::process::id());
    run_compose(root, &project, &["up", "-d", "--wait", "postgres"])?;
    let result = run_fdc_integration_tests(root);
    let cleanup = run_compose(root, &project, &["down", "--remove-orphans"]);
    result?;
    cleanup?;

    if run_exact_m3_drill {
        run_exact_m3_staging_drill(root)?;
    }
    println!("[PASS] staged FDC importer, catalog handoff, and explicit activation verification");
    Ok(())
}

fn exact_m3_drill_requested() -> Result<bool, Box<dyn Error>> {
    match (
        env::var_os("FDC_FOUNDATION_ARCHIVE"),
        env::var_os("FDC_FOUNDATION_JSON"),
    ) {
        (Some(_), Some(_)) => Ok(true),
        (None, None) => Ok(false),
        _ => Err("[fdc] set both FDC_FOUNDATION_ARCHIVE and FDC_FOUNDATION_JSON for the exact M3 staging drill".into()),
    }
}

fn run_fdc_integration_tests(root: &Path) -> Result<(), Box<dyn Error>> {
    run_persistence_test(root, "fdc_importer_integration", &["--ignored"])?;
    run_persistence_test(
        root,
        "catalog_handoff_importer_integration",
        &["--ignored", "--test-threads=1"],
    )?;
    run_persistence_test(
        root,
        "catalog_activation_integration",
        &["--ignored", "--test-threads=1"],
    )
}

fn run_exact_m3_staging_drill(root: &Path) -> Result<(), Box<dyn Error>> {
    let project = format!("xtask-fdc-m3-{}", std::process::id());
    run_compose(root, &project, &["up", "-d", "--wait", "postgres"])?;
    let result = run_persistence_test(
        root,
        "fdc_m3_staging_drill",
        &["--ignored", "--test-threads=1", "--nocapture"],
    );
    let cleanup = run_compose(root, &project, &["down", "--remove-orphans"]);
    result?;
    cleanup
}

fn run_persistence_test(
    root: &Path,
    test_name: &str,
    test_args: &[&str],
) -> Result<(), Box<dyn Error>> {
    let mut args = vec![
        "test",
        "-p",
        "persistence-postgres",
        "--test",
        test_name,
        "--",
    ];
    args.extend_from_slice(test_args);
    run_owned(
        root,
        "cargo",
        &args,
        &[("TEST_DATABASE_URL", TEST_DATABASE_URL)],
    )
}

fn run_compose(root: &Path, project: &str, operation: &[&str]) -> Result<(), Box<dyn Error>> {
    let mut args = vec![
        "compose".to_owned(),
        "-p".to_owned(),
        project.to_owned(),
        "-f".to_owned(),
        "deploy/compose.yaml".to_owned(),
    ];
    args.extend(operation.iter().map(|argument| (*argument).to_owned()));
    run_owned(root, "docker", &args, &[])
}
