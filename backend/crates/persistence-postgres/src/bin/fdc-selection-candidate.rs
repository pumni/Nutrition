use persistence_postgres::{
    FDC_FOUNDATION_V1_SELECTION_CAP, FdcFoundationValidationRequest,
    build_fdc_selection_candidate_manifest,
};
use std::{env, fs, process::ExitCode};

fn main() -> ExitCode {
    match run() {
        Ok(manifest) => {
            match serde_json::to_string_pretty(&manifest) {
                Ok(rendered) => println!("{rendered}"),
                Err(error) => {
                    eprintln!("failed to render candidate manifest: {error}");
                    return ExitCode::from(2);
                }
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            eprintln!(
                "usage: cargo run -p persistence-postgres --bin fdc-selection-candidate -- \
                 --artifact <path> --release <release> --published-date <date> \
                 --object-uri <uri> --expected-sha256 <sha256> \
                 [--payload-filename <name>] [--archive-sha256 <sha256>] \
                 [--preprocessing-policy <policy-version>] [--cap <1..20>]"
            );
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<serde_json::Value, String> {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    let artifact_path = required_argument(&arguments, "--artifact")?;
    let request = FdcFoundationValidationRequest {
        release_version: required_argument(&arguments, "--release")?,
        source_published_date: required_argument(&arguments, "--published-date")?,
        object_uri: required_argument(&arguments, "--object-uri")?,
        source_payload_filename: optional_argument(&arguments, "--payload-filename"),
        source_archive_sha256: optional_argument(&arguments, "--archive-sha256"),
        expected_sha256: required_argument(&arguments, "--expected-sha256")?,
        reviewed_fdc_ids: Vec::new(),
        preprocessing_policy_version: optional_argument(&arguments, "--preprocessing-policy"),
    };
    let candidate_cap = optional_argument(&arguments, "--cap")
        .map(|value| {
            value
                .parse::<usize>()
                .map_err(|error| format!("invalid --cap value {value}: {error}"))
        })
        .transpose()?
        .unwrap_or(FDC_FOUNDATION_V1_SELECTION_CAP);
    let source_bytes = fs::read(&artifact_path)
        .map_err(|error| format!("failed to read artifact {artifact_path}: {error}"))?;
    build_fdc_selection_candidate_manifest(&source_bytes, &request, candidate_cap)
        .map_err(|error| error.to_string())
}

fn optional_argument(arguments: &[String], flag: &str) -> Option<String> {
    arguments
        .windows(2)
        .find(|window| window[0] == flag)
        .map(|window| window[1].clone())
}

fn required_argument(arguments: &[String], flag: &str) -> Result<String, String> {
    optional_argument(arguments, flag).ok_or_else(|| format!("missing required argument {flag}"))
}
