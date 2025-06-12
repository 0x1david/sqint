#![allow(dead_code, unused_variables)]
mod analyzer;
mod cli;
mod config;
mod finder;
mod logging;

use clap::Parser;
use cli::{CheckArgs, Cli, Commands, ConfigArgs, InitArgs};
use config::{Config, DEFAULT_CONFIG_NAME};
use finder::{FinderConfig, SqlExtract, SqlFinder, collect_files};
use logging::{LogLevel, Logger};
use std::env;

fn main() {
    let cli = Cli::parse();
    let config = load_config(&cli);

    setup_logging(&cli);

    let result = match &cli.command {
        Commands::Check(args) => handle_check(args, &config, &cli),
        Commands::Init(args) => handle_init(args, &cli),
        Commands::Config(args) => handle_config(args, &config, &cli),
    };

    match result {
        Ok(exit_code) => std::process::exit(exit_code),
        Err(error) => {
            eprintln!("Error: {}", error);
            std::process::exit(1);
        }
    }
}

fn handle_check(
    args: &CheckArgs,
    config: &Config,
    cli: &Cli,
) -> Result<i32, Box<dyn std::error::Error>> {
    let cfg = FinderConfig {
        variables: config.variable_names.clone(),
        min_sql_length: config.min_sql_length,
    };
    let sql_finder = SqlFinder::new(cfg);

    let sqls: Vec<SqlExtract> = collect_files(&args.paths)
        .iter()
        .filter(|f| finder::is_python_file(f))
        .filter_map(|f| f.to_str())
        .flat_map(|p| sql_finder.analyze_file(p))
        .collect();
    sqls.iter().for_each(|s| debug!("{}", s));
    Ok(0)
}

fn setup_logging(cli: &Cli) {
    let lvl = match (cli.verbose, cli.quiet) {
        (true, false) => LogLevel::Info,
        (false, true) => LogLevel::Error,
        (false, false) => LogLevel::Debug, //LogLevel::Warn,
        (true, true) => unreachable!(),
    };
    dbg!(lvl);
    Logger::init(lvl);
}

fn load_config(cli: &Cli) -> Config {
    match &cli.config {
        Some(config_path) => Config::from_file(config_path).unwrap_or_default(),
        None => {
            let mut cwd = env::current_dir().expect("Not able to read current working directory.");
            cwd.set_file_name(DEFAULT_CONFIG_NAME);
            let cfg = Config::from_file(cwd);
            cfg.unwrap_or_default()
        }
    }
}

fn handle_init(args: &InitArgs, cli: &Cli) -> Result<i32, Box<dyn std::error::Error>> {
    // TODO: Create configuration file
    unimplemented!();
}

fn handle_config(
    args: &ConfigArgs,
    config: &Config,
    cli: &Cli,
) -> Result<i32, Box<dyn std::error::Error>> {
    if args.validate {
        println!("Validating configuration...");
    }
    if args.list_variables {
        println!("Variables that will be analyzed:");
        for var in &config.variable_names {
            println!("  - {}", var);
        }
    }
    Ok(0)
}
