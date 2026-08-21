use std::{env, error::Error, path::PathBuf};

#[derive(Debug, Eq, PartialEq)]
pub enum Task {
    Check,
    Architecture,
    Privacy,
    Migrations { record_new: bool },
    Json,
    Postgres,
    Fdc,
    Containers,
    Benchmark,
    All,
}

pub fn parse(mut args: impl Iterator<Item = String>) -> Result<Task, Box<dyn Error>> {
    let task = args.next().unwrap_or_else(|| "check".to_owned());
    let parsed = match task.as_str() {
        "check" => Task::Check,
        "architecture" => Task::Architecture,
        "privacy" => Task::Privacy,
        "migrations" => Task::Migrations {
            record_new: matches!(args.next().as_deref(), Some("--record-new")),
        },
        "json" => Task::Json,
        "postgres" => Task::Postgres,
        "fdc" => Task::Fdc,
        "containers" => Task::Containers,
        "benchmark" => Task::Benchmark,
        "all" => Task::All,
        "help" | "--help" | "-h" => {
            print_help();
            return Err("help requested".into());
        }
        other => return Err(format!("unknown xtask command: {other}\n{HELP}").into()),
    };
    if args.next().is_some() {
        return Err(format!("unexpected xtask argument\n{HELP}").into());
    }
    Ok(parsed)
}

pub fn repository_root() -> Result<PathBuf, Box<dyn Error>> {
    let mut current = env::current_dir()?;
    loop {
        if current.join("Cargo.toml").is_file() && current.join("crates").is_dir() {
            return Ok(current);
        }
        if !current.pop() {
            return Err("could not locate repository root from current directory".into());
        }
    }
}

fn print_help() {
    println!("{HELP}");
}

const HELP: &str = "cargo xtask <command>\n\nCommands: check, architecture, privacy, migrations [--record-new], json, postgres, fdc, containers, benchmark, all";

#[cfg(test)]
mod tests {
    use super::{Task, parse};

    #[test]
    fn parses_default_check() {
        assert_eq!(parse([].into_iter()).expect("check"), Task::Check);
    }

    #[test]
    fn parses_explicit_migration_recording() {
        assert_eq!(
            parse(["migrations".to_owned(), "--record-new".to_owned()].into_iter())
                .expect("migrations"),
            Task::Migrations { record_new: true }
        );
    }

    #[test]
    fn rejects_unknown_commands() {
        assert!(parse(["unknown".to_owned()].into_iter()).is_err());
    }
}
