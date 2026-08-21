#![deny(unsafe_code)]

mod architecture;
mod benchmark;
mod check;
mod command;
mod containers;
mod docs;
mod fdc;
mod json;
mod migrations;
mod postgres;
mod privacy;
mod process;

use std::{env, error::Error, process::exit};

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let root = command::repository_root()?;
    match command::parse(env::args().skip(1))? {
        command::Task::Check => check::run(&root)?,
        command::Task::Architecture => architecture::run(&root)?,
        command::Task::Privacy => privacy::run(&root)?,
        command::Task::Migrations { record_new } => migrations::run(&root, record_new)?,
        command::Task::Json => json::run(&root)?,
        command::Task::Postgres => postgres::run(&root)?,
        command::Task::Fdc => fdc::run(&root)?,
        command::Task::Containers => containers::run(&root)?,
        command::Task::Benchmark => benchmark::run(&root)?,
        command::Task::All => {
            check::run(&root)?;
            postgres::run(&root)?;
            fdc::run(&root)?;
            containers::run(&root)?;
            benchmark::run(&root)?;
        }
    }
    Ok(())
}
