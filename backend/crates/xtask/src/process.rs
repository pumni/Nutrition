use std::{error::Error, path::Path, process::Command};

pub fn run(root: &Path, program: &str, args: &[&str]) -> Result<(), Box<dyn Error>> {
    let owned = args
        .iter()
        .map(|argument| (*argument).to_owned())
        .collect::<Vec<_>>();
    run_owned(root, program, &owned, &[])
}

pub fn run_owned<S: AsRef<str>>(
    root: &Path,
    program: &str,
    args: &[S],
    environment: &[(&str, &str)],
) -> Result<(), Box<dyn Error>> {
    let args = args
        .iter()
        .map(|argument| argument.as_ref().to_owned())
        .collect::<Vec<_>>();
    let rendered = render(program, &args);
    println!("$ {rendered}");
    let mut command = Command::new(program);
    command.current_dir(root).args(&args);
    for (key, value) in environment {
        command.env(key, value);
    }
    let status = command
        .status()
        .map_err(|error| format!("could not execute `{rendered}`: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("command failed with {status}: `{rendered}`").into())
    }
}

pub fn output(root: &Path, program: &str, args: &[&str]) -> Result<String, Box<dyn Error>> {
    let rendered = render(
        program,
        &args
            .iter()
            .map(|argument| (*argument).to_owned())
            .collect::<Vec<_>>(),
    );
    println!("$ {rendered}");
    let result = Command::new(program)
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|error| format!("could not execute `{rendered}`: {error}"))?;
    if !result.status.success() {
        return Err(format!(
            "command failed with {}: `{rendered}`\n{}",
            result.status,
            String::from_utf8_lossy(&result.stderr).trim()
        )
        .into());
    }
    Ok(String::from_utf8(result.stdout)?.trim().to_owned())
}

pub fn run_powershell_script(root: &Path, script: &str) -> Result<(), Box<dyn Error>> {
    run_powershell_script_with_env(root, script, &[])
}

pub fn run_powershell_script_with_env(
    root: &Path,
    script: &str,
    environment: &[(&str, &str)],
) -> Result<(), Box<dyn Error>> {
    let script_path = root.join(script);
    if !script_path.is_file() {
        return Err(format!("verification script is missing: {}", script_path.display()).into());
    }
    let path = script_path.to_string_lossy().into_owned();
    let args = ["-NoLogo", "-NoProfile", "-File", path.as_str()];
    let owned = args
        .iter()
        .map(|value| (*value).to_owned())
        .collect::<Vec<_>>();
    run_owned(root, "pwsh", &owned, environment)
}

pub fn command_available(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
}

fn render(program: &str, args: &[String]) -> String {
    let mut parts = vec![program.to_owned()];
    parts.extend(args.iter().map(|argument| quote(argument)));
    parts.join(" ")
}

fn quote(value: &str) -> String {
    if value.is_empty() || value.chars().any(char::is_whitespace) {
        format!("\"{value}\"")
    } else {
        value.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::render;

    #[test]
    fn renders_command_arguments() {
        assert_eq!(
            render("cargo", &["test".to_owned(), "hello world".to_owned()]),
            "cargo test \"hello world\""
        );
    }
}
