use std::{error::Error, path::Path};

use crate::process::{command_available, output, run as run_command, run_owned};
use std::{thread, time::Duration};

#[allow(clippy::too_many_lines)]
pub fn run(root: &Path) -> Result<(), Box<dyn Error>> {
    if !command_available("docker") {
        return Err("[containers] Docker is required for container verification".into());
    }
    let compose = [
        "compose",
        "-p",
        "xtask-container-smoke",
        "-f",
        "deploy/compose.yaml",
        "-f",
        "deploy/compose.container-smoke.yaml",
    ];
    let images_ready = output(root, "docker", &["image", "inspect", "nutrition-api:ci"]).is_ok()
        && output(root, "docker", &["image", "inspect", "nutrition-worker:ci"]).is_ok();
    let mut up_args = vec![
        compose[0].to_owned(),
        compose[1].to_owned(),
        compose[2].to_owned(),
        compose[3].to_owned(),
        compose[4].to_owned(),
        compose[5].to_owned(),
        compose[6].to_owned(),
        "up".to_owned(),
        "-d".to_owned(),
    ];
    if !images_ready {
        up_args.push("--build".to_owned());
    }
    up_args.push("api-smoke".to_owned());
    run_owned(root, "docker", &up_args, &[])?;
    let result = (|| {
        let mut ready = false;
        for _ in 0..30 {
            if run_command(
                root,
                "curl",
                &[
                    "--fail",
                    "--silent",
                    "--show-error",
                    "http://127.0.0.1:18080/health/ready",
                ],
            )
            .is_ok()
            {
                ready = true;
                break;
            }
            thread::sleep(Duration::from_millis(500));
        }
        if !ready {
            return Err("[containers] containerized API did not become ready".into());
        }
        let api_user = output(
            root,
            "docker",
            &[
                "image",
                "inspect",
                "nutrition-api:ci",
                "--format",
                "{{.Config.User}}",
            ],
        )?;
        let worker_user = output(
            root,
            "docker",
            &[
                "image",
                "inspect",
                "nutrition-worker:ci",
                "--format",
                "{{.Config.User}}",
            ],
        )?;
        if api_user != "10001:10001" || worker_user != "10001:10001" {
            return Err(
                "[containers] production images must run as non-root UID/GID 10001:10001".into(),
            );
        }
        Ok::<(), Box<dyn Error>>(())
    })();
    let cleanup = run_command(
        root,
        "docker",
        &[
            compose[0],
            compose[1],
            compose[2],
            compose[3],
            compose[4],
            compose[5],
            compose[6],
            "down",
            "--remove-orphans",
        ],
    );
    result?;
    cleanup?;
    println!("[PASS] production container readiness and non-root verification");
    Ok(())
}
