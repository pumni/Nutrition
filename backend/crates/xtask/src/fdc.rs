use std::{error::Error, path::Path};

use crate::process::{command_available, run_owned};

pub fn run(root: &Path) -> Result<(), Box<dyn Error>> {
    if !command_available("docker") {
        return Err("[fdc] Docker is required for staged importer verification".into());
    }
    let compose = [
        "compose".to_owned(),
        "-p".to_owned(),
        format!("xtask-fdc-{}", std::process::id()),
        "-f".to_owned(),
        "deploy/compose.yaml".to_owned(),
    ];
    run_owned(
        root,
        "docker",
        &[
            compose[0].clone(),
            compose[1].clone(),
            compose[2].clone(),
            compose[3].clone(),
            compose[4].clone(),
            "up".to_owned(),
            "-d".to_owned(),
            "--wait".to_owned(),
            "postgres".to_owned(),
        ],
        &[],
    )?;
    let result = (|| {
        run_owned(
            root,
            "cargo",
            &[
                "test",
                "-p",
                "persistence-postgres",
                "--test",
                "fdc_importer_integration",
                "--",
                "--ignored",
            ],
            &[(
                "TEST_DATABASE_URL",
                "postgres://nutrition:nutrition@127.0.0.1:5432/nutrition",
            )],
        )?;
        run_owned(
            root,
            "cargo",
            &[
                "test",
                "-p",
                "persistence-postgres",
                "--test",
                "catalog_activation_integration",
                "--",
                "--ignored",
                "--test-threads=1",
            ],
            &[(
                "TEST_DATABASE_URL",
                "postgres://nutrition:nutrition@127.0.0.1:5432/nutrition",
            )],
        )?;
        Ok::<(), Box<dyn Error>>(())
    })();
    let cleanup = run_owned(
        root,
        "docker",
        &[
            compose[0].clone(),
            compose[1].clone(),
            compose[2].clone(),
            compose[3].clone(),
            compose[4].clone(),
            "down".to_owned(),
            "--remove-orphans".to_owned(),
        ],
        &[],
    );
    result?;
    cleanup?;
    println!("[PASS] staged FDC importer and explicit activation verification");
    Ok(())
}
