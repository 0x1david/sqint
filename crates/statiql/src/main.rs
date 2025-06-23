#![allow(dead_code, unused_variables, clippy::multiple_crate_versions)]
mod analyzer;
mod cli;
mod config;
mod files;
mod handlers;
use clap::Parser;
use cli::{Cli, Commands};
use config::{Config, DEFAULT_CONFIG, DEFAULT_CONFIG_NAME};
use finder::FinderConfig;
use logging::{LogLevel, Logger, info};
use std::env;

fn main() {
    let cli = Cli::parse();
    let config = load_config(&cli);
    setup_logging(&cli);
    match cli.command {
        None => handlers::handle_check(&config.into(), &cli),
        Some(ref comm) => {
            match comm {
                Commands::Check(args) => handlers::handle_check(&config.into(), &cli),
                Commands::Init(_) => handlers::handle_init(),
            };
        }
    }
    std::process::exit(Logger::exit_code())
}

fn load_config(cli: &Cli) -> Config {
    let config_path = env::current_dir()
        .expect("Unable to read current working directory")
        .join(DEFAULT_CONFIG_NAME);
    let mut config = Config::default();

    Config::from_file(&config_path).map_or_else(
        |e| {
            info!(
                "No configuration file found at '{}'. Using default configuration.",
                config_path.display()
            );
            info!("Config load error: {}", e);
        },
        |file_config| {
            info!("Loaded configuration from '{}'.", config_path.display());
            config.merge_with(file_config);
        },
    );
    config
}

fn setup_logging(cli: &Cli) {
    let lvl = if cli.debug {
        LogLevel::Debug
    } else {
        match (cli.verbose, cli.quiet) {
            (true, false) => LogLevel::Info,
            (false, true) => LogLevel::Error,
            (false, false) => LogLevel::Warn,
            (true, true) => unreachable!(),
        }
    };
    Logger::init(lvl);
}
