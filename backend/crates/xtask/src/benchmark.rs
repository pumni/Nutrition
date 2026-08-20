use std::{error::Error, path::Path};

use crate::process::run_powershell_script;

pub fn run(root: &Path) -> Result<(), Box<dyn Error>> {
    run_powershell_script(root, "scripts/verify-vietnamese-meal-bench.ps1")?;
    run_powershell_script(root, "scripts/test-vietnamese-meal-bench-evaluation.ps1")?;
    println!("[PASS] local VietnameseMealBench verification");
    Ok(())
}
