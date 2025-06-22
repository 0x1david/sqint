#![allow(dead_code, unused_variables)]
mod analyzer;
mod cli;
mod config;

use clap::Parser;
use cli::{Cli, Commands, ConfigArgs};
use config::{Config, DEFAULT_CONFIG, DEFAULT_CONFIG_NAME};
use finder::{FinderConfig, SqlFinder, collect_files};
use logging::{LogLevel, Logger, always_log};
use std::env;
use std::sync::Arc;
use std::thread;

fn main() {
    let cli = Cli::parse();
    let config = load_config(&cli);

    setup_logging(&cli, config.debug);

    match cli.command {
        None => handle_check(config, cli),
        Some(ref comm) => {
            match comm {
                Commands::Check(args) => handle_check(config, cli),
                Commands::Init(_) => handle_init(),
                Commands::Config(args) => handle_config(args, config),
            };
        }
    }

    std::process::exit(Logger::exit_code())
}

fn handle_check(config: Config, cli: Cli) {
    let config = Arc::new(config);
    let cfg = Arc::new(FinderConfig {
        variable_ctx: config
            .variable_contexts
            .iter()
            .map(|f| f.to_lowercase())
            .collect(),
        func_ctx: config
            .function_contexts
            .iter()
            .map(|f| f.to_lowercase())
            .collect(),
        class_ctx: config
            .class_contexts
            .iter()
            .map(|f| f.to_lowercase())
            .collect(),
        min_sql_length: config.min_sql_length,
    });

    let python_files: Vec<String> = collect_files(&cli.check_args.paths)
        .iter()
        .filter(|f| finder::is_python_file(f))
        .filter_map(|f| f.to_str())
        .map(|s| s.to_string())
        .collect();

    python_files
        .into_iter()
        .map(|file_path| {
            let cfg = cfg.clone();
            let app_cfg = config.clone();
            thread::spawn(move || {
                let sql_finder = SqlFinder::new(cfg);

                if let Some(sql_extract) = sql_finder.analyze_file(&file_path) {
                    let analyzer = analyzer::SqlAnalyzer::new(analyzer::SqlDialect::PostgreSQL);
                    println!("{}", sql_extract);
                    analyzer.analyze_sql_extract(&sql_extract, app_cfg);
                }
            })
        })
        .collect::<Vec<thread::JoinHandle<()>>>()
        .into_iter()
        .for_each(|handle| {
            let _ = handle.join();
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
    let config_path = env::current_dir()
        .expect("Unable to read current working directory")
        .join(DEFAULT_CONFIG_NAME);

    let mut config = Config::default();

    if let Ok(file_config) = Config::from_file(&config_path) {
        config.merge_with(file_config);
    }

    config
}

fn handle_init() {
    let path = env::current_dir()
        .expect("Failed fetching CWD.")
        .join(DEFAULT_CONFIG_NAME);

    std::fs::write(&path, DEFAULT_CONFIG)
        .expect("Can't write to {path.display()}, likely due to permission issues");
    always_log!("Created default config at {}", path.display());
}

fn handle_config(args: &ConfigArgs, config: Config) {
    if args.validate {
        println!("Validating configuration...");
    }
    if args.list_variables {
        println!("Variables that will be analyzed:");
        for var in &config.variable_contexts {
            println!("  - {var}");
        }
    }
}
