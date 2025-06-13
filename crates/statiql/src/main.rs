#![allow(dead_code, unused_variables)]
mod analyzer;
mod cli;
mod config;

use clap::Parser;
use cli::{CheckArgs, Cli, Commands, ConfigArgs};
use config::{Config, DEFAULT_CONFIG, DEFAULT_CONFIG_NAME};
use finder::{FinderConfig, SqlExtract, SqlFinder, collect_files};
use logging::{LogLevel, Logger, always_log, debug};
use std::env;

fn main() {
    let cli = Cli::parse();
    let config = load_config(&cli);

    setup_logging(&cli, config.debug);

    match &cli.command {
        None => handle_check(&cli.check_args, &config, &cli),
        Some(comm) => {
            match comm {
                Commands::Check(args) => handle_check(args, &config, &cli),
                Commands::Init(_) => handle_init(),
                Commands::Config(args) => handle_config(args, &config),
            };
        }
    }

    std::process::exit(Logger::exit_code())
}

fn handle_check(args: &CheckArgs, config: &Config, cli: &Cli) {
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

    let anlyzer = analyzer::SqlAnalyzer::new(analyzer::SqlDialect::Generic);
    sqls.iter().for_each(|e| {
        debug!("{}", e);
        anlyzer.analyze_sql_extract(e);
    });
}

fn setup_logging(cli: &Cli, debug: bool) {
    let lvl = if debug {
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

fn load_config(cli: &Cli) -> Config {
    match &cli.config {
        Some(config_path) => Config::from_file(config_path).unwrap_or_default(),
        None => {
            let mut cwd = env::current_dir().expect("Not able to read current working directory.");
            cwd.push(DEFAULT_CONFIG_NAME);
            let cfg = Config::from_file(cwd);
            cfg.unwrap_or_default()
        }
    }
}

fn handle_init() {
    let path = env::current_dir()
        .expect("Failed fetching CWD.")
        .join(DEFAULT_CONFIG_NAME);

    std::fs::write(&path, DEFAULT_CONFIG)
        .expect("Can't write to {path.display()}, likely due to permission issues");
    always_log!("Created default config at {}", path.display());
}

fn handle_config(args: &ConfigArgs, config: &Config) {
    if args.validate {
        println!("Validating configuration...");
    }
    if args.list_variables {
        println!("Variables that will be analyzed:");
        for var in &config.variable_names {
            println!("  - {}", var);
        }
    }
}
