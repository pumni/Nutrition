use persistence_postgres::{FdcFoundationValidationRequest, validate_fdc_foundation_json};
use std::{env, fs, process::ExitCode};

fn main() -> ExitCode {
    match run() {
        Ok((report, request)) => {
            match report.to_pretty_json(&request) {
                Ok(json) => println!("{json}"),
                Err(error) => {
                    eprintln!("failed to render validation report: {error}");
                    return ExitCode::from(2);
                }
            }
            if report.validation_status == "passed" {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(error) => {
            eprintln!("{error}");
            eprintln!(
                "usage: cargo run -p persistence-postgres --bin fdc-validation-report -- \
                 --artifact <path> --release <release> --published-date <date> \
                 --object-uri <uri> --expected-sha256 <sha256> [--selection-id <fdc-id>] \
                 [--preprocessing-policy <policy-version>]"
            );
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<
    (
        persistence_postgres::FdcFoundationValidationReport,
        FdcFoundationValidationRequest,
    ),
    String,
> {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    let artifact_path = required_argument(&arguments, "--artifact")?;
    let request = FdcFoundationValidationRequest {
        release_version: required_argument(&arguments, "--release")?,
        source_published_date: required_argument(&arguments, "--published-date")?,
        object_uri: required_argument(&arguments, "--object-uri")?,
        source_payload_filename: optional_argument(&arguments, "--payload-filename"),
        source_archive_sha256: optional_argument(&arguments, "--archive-sha256"),
        expected_sha256: required_argument(&arguments, "--expected-sha256")?,
        reviewed_fdc_ids: repeated_arguments(&arguments, "--selection-id")?,
        preprocessing_policy_version: optional_argument(&arguments, "--preprocessing-policy"),
    };
    let source_bytes = fs::read(&artifact_path)
        .map_err(|error| format!("failed to read artifact {artifact_path}: {error}"))?;
    let report = validate_fdc_foundation_json(&source_bytes, &request);
    Ok((report, request))
}

fn optional_argument(arguments: &[String], flag: &str) -> Option<String> {
    arguments
        .windows(2)
        .find(|window| window[0] == flag)
        .map(|window| window[1].clone())
}

fn required_argument(arguments: &[String], flag: &str) -> Result<String, String> {
    arguments
        .windows(2)
        .find(|window| window[0] == flag)
        .map(|window| window[1].clone())
        .ok_or_else(|| format!("missing required argument {flag}"))
}

fn repeated_arguments(arguments: &[String], flag: &str) -> Result<Vec<u64>, String> {
    let mut values = Vec::new();
    let mut index = 0;
    while index < arguments.len() {
        if arguments[index] == flag {
            let value = arguments
                .get(index + 1)
                .ok_or_else(|| format!("missing value for repeated argument {flag}"))?;
            values.push(
                value
                    .parse::<u64>()
                    .map_err(|error| format!("invalid {flag} value {value}: {error}"))?,
            );
            index += 1;
        }
        index += 1;
    }
    Ok(values)
}
