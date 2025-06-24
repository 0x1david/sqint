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
use logging::{LogLevel, Logger};

fn main() {
    let cli = Cli::parse();
    let config = files::load_config(&cli);
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

fn setup_logging(cli: &Cli) {
    let lvl = match (cli.debug, cli.verbose, cli.quiet) {
        (true, _, _) => LogLevel::Debug,
        (false, true, false) => LogLevel::Info,
        (false, false, true) => LogLevel::Error,
        (false, false, false) => LogLevel::Warn,
        (false, true, true) => unreachable!(),
    };
    Logger::init(lvl);
}
