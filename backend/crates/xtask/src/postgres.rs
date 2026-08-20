use std::{error::Error, path::Path};

use crate::process::run_powershell_script_with_env;

pub fn run(root: &Path) -> Result<(), Box<dyn Error>> {
    run_powershell_script_with_env(
        root,
        "scripts/verify-postgres.ps1",
        &[("COMPOSE_PROJECT_NAME", "xtask-postgres")],
    )
}
