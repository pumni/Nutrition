//! `PostgreSQL` verification lifecycle and integration-test ownership.

use std::{error::Error, path::Path};

use crate::process::{command_available, run_owned};

const DATABASE_URL: &str = "postgres://nutrition:nutrition@127.0.0.1:5432/nutrition";

pub fn run(root: &Path) -> Result<(), Box<dyn Error>> {
    if !command_available("docker") {
        return Err("[postgres] Docker is required for PostgreSQL verification".into());
    }

    let project = format!("xtask-postgres-{}", std::process::id());
    run_compose(root, &project, &["up", "-d", "--wait", "postgres"])?;

    let result = (|| {
        run_owned(
            root,
            "cargo",
            &["run", "-p", "worker"],
            &[
                ("APP_ENV", "ci"),
                ("DATABASE_URL", DATABASE_URL),
                ("RUN_MIGRATIONS", "true"),
                ("RUN_FOUNDATION_SEED", "true"),
                ("WORKER_MODE", "idle"),
                ("WORKER_ID", "xtask-postgres-bootstrap"),
            ],
        )?;
        run_owned(
            root,
            "cargo",
            &[
                "test",
                "-p",
                "persistence-postgres",
                "--test",
                "postgres_integration",
                "--",
                "--ignored",
                "--test-threads=1",
            ],
            &[("TEST_DATABASE_URL", DATABASE_URL)],
        )?;
        run_owned(
            root,
            "cargo",
            &[
                "test",
                "-p",
                "api-http",
                "--test",
                "postgres_integration",
                "--",
                "--ignored",
                "--test-threads=1",
            ],
            &[("TEST_DATABASE_URL", DATABASE_URL)],
        )?;
        run_owned(
            root,
            "cargo",
            &["run", "-p", "worker"],
            &[
                ("APP_ENV", "ci"),
                ("DATABASE_URL", DATABASE_URL),
                ("RUN_MIGRATIONS", "false"),
                ("RUN_FOUNDATION_SEED", "false"),
                ("WORKER_MODE", "run-once"),
                ("WORKER_ID", "xtask-postgres-verification"),
            ],
        )?;
        run_compose(
            root,
            &project,
            &[
                "exec",
                "-T",
                "postgres",
                "psql",
                "-U",
                "nutrition",
                "-d",
                "nutrition",
                "-f",
                "/migrations/tests/immutability.sql",
            ],
        )?;
        Ok::<(), Box<dyn Error>>(())
    })();

    let cleanup = run_compose(root, &project, &["down", "--remove-orphans"]);
    result?;
    cleanup?;
    println!(
        "[PASS] PostgreSQL lifecycle, Rust integration, worker, and immutability verification"
    );
    Ok(())
}

fn run_compose(root: &Path, project: &str, command: &[&str]) -> Result<(), Box<dyn Error>> {
    let mut args = vec![
        "compose".to_owned(),
        "-p".to_owned(),
        project.to_owned(),
        "-f".to_owned(),
        "deploy/compose.yaml".to_owned(),
    ];
    args.extend(command.iter().map(|value| (*value).to_owned()));
    run_owned(root, "docker", &args, &[])
}
