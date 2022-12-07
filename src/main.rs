mod client;
mod config;
mod process;
mod rotate;
mod server;

use anyhow::Result;
use config::Action;
use std::{io::Write, str::FromStr};

fn main() -> Result<()> {
    init_logger();

    let action = Action::from_args()?;

    if matches!(action, Action::Serve) {
        let rotater = rotate::Rotater::new()?;
        let process = process::Process::new(rotater)?;
        server::run(process)?;
    } else {
        client::request(action)?;
    }

    Ok(())
}

fn init_logger() {
    let level = std::env::var("RUST_LOG").unwrap_or(String::from("info"));
    let level = log::LevelFilter::from_str(&level)
        .expect(&format!("invalid log level: RUST_LOG={}", level));

    env_logger::Builder::new()
        .format(|buf, record| {
            writeln!(
                buf,
                "{} {} {}:{} {}",
                chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
                record.level(),
                record
                    .file()
                    .unwrap_or("<unknown>")
                    .trim_start_matches("src/"),
                record.line().unwrap_or(0),
                record.args(),
            )
        })
        .filter(None, level)
        .target(env_logger::Target::Stdout)
        .init();
}
