// TODO: global immutable config with lazy_static

use std::{collections::HashMap, fmt::Display};

use serde::{Deserialize, Serialize};

use anyhow::{Context, Result};

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub sup: Sup,
    pub program: Program,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sup {
    pub socket: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Program {
    pub process: Process,
    pub log: Log,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Process {
    pub path: String,
    pub args: Vec<String>,
    pub work_dir: String,
    pub auto_start: bool,
    pub start_seconds: u64,
    pub restart_strategy: RestartStrategy,
    pub envs: HashMap<String, String>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Log {
    pub path: String,
    pub compress: bool,
    pub merge_compressed: bool,
    pub max_days: u32,
    pub max_backups: u32,
    pub max_size: u64,
}

// TODO: PartialEq、Clone derive 啥意思？？？
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RestartStrategy {
    #[serde(rename = "none")]
    None,
    #[serde(rename = "always")]
    Always,
    #[serde(rename = "on-failure")]
    OnFailure,
}

impl Default for RestartStrategy {
    fn default() -> Self {
        RestartStrategy::Always
    }
}

impl Display for RestartStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RestartStrategy::None => write!(f, "none"),
            RestartStrategy::Always => write!(f, "always"),
            RestartStrategy::OnFailure => write!(f, "on-failure"),
        }
    }
}

#[derive(Debug)]
pub enum Action {
    Serve,
    Start,
    Stop,
    Restart,
    Reload,
    Kill,
    Status,
    Exit,
}

impl Display for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Action::Serve => write!(f, "serve"),
            Action::Start => write!(f, "start"),
            Action::Stop => write!(f, "stop"),
            Action::Restart => write!(f, "restart"),
            Action::Reload => write!(f, "reload"),
            Action::Kill => write!(f, "kill"),
            Action::Status => write!(f, "status"),
            Action::Exit => write!(f, "exit"),
        }
    }
}

impl From<&str> for Action {
    fn from(value: &str) -> Self {
        match value {
            x if x == "serve" => Action::Serve,
            x if x == "start" => Action::Start,
            x if x == "stop" => Action::Stop,
            x if x == "restart" => Action::Restart,
            x if x == "reload" => Action::Reload,
            x if x == "kill" => Action::Kill,
            x if x == "status" => Action::Status,
            x if x == "exit" => Action::Exit,
            _ => panic!("BUG: unknown action '{}'", value),
        }
    }
}

pub fn new() -> Result<(Config, Action)> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 || args.len() > 4 {
        print_help();
        std::process::exit(1);
    }
    if args[1] == "-h" || args[1] == "-v" {
        print_help();
        std::process::exit(0);
    }
    if args[1] != "-c" {
        print_help();
        std::process::exit(1);
    }
    if args.len() == 2 {
        println!("expected config file path after flag '-c'");
        std::process::exit(1);
    }
    let conf_path = &args[2];
    let conf = std::fs::read_to_string(conf_path)
        .context(format!("failed to read config file {}", conf_path))?;
    let conf: Config = toml::from_str(&conf)
        .context(format!("failed to deserialize config file {}", conf_path))?;

    let mut action = String::from("serve");
    if args.len() == 4 {
        action = args[3].clone();
    }
    Ok((conf, Action::from(action.as_str())))
}

fn print_help() {
    println!("Usage:");
    println!("    sup -h                          # show this message");
    println!("    sup -v                          # show this message");
    println!("    sup -c config.toml              # start sup daemon");
    println!("    sup -c config.toml start        # start program asynchronously");
    println!("    sup -c config.toml start-wait   # wait program to start");
    println!("    sup -c config.toml stop         # stop program asynchronously");
    println!("    sup -c config.toml stop-wait    # wait program to stop");
    println!("    sup -c config.toml restart      # restart program asynchronously");
    println!("    sup -c config.toml restart-wait # wait program to restart");
    println!("    sup -c config.toml reload       # reload program");
    println!("    sup -c config.toml kill         # kill program and all child processes");
    println!("    sup -c config.toml status       # print status of program");
    println!(
        "    sup -c config.toml exit         # exit the sup daemon and the process asynchronously"
    );
    println!("    sup -c config.toml exit-wait    # wait the sup daemon and the process to exit");
    println!("");
    println!("Sup version: v{}", env!("CARGO_PKG_VERSION"));
    println!("");
    println!("Check more on: https://github.com/sequix/sup-rust");
}
