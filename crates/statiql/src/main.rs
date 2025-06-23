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
    setup_logging(&cli);
    match cli.command {
        None => handle_check(&config.into(), &cli),
        Some(ref comm) => {
            match comm {
                Commands::Check(args) => handle_check(&config.into(), &cli),
                Commands::Init(_) => handle_init(),
                Commands::Config(args) => handle_config(args, &config),
            };
        }
    }
    std::process::exit(Logger::exit_code())
}

fn handle_check(config: &Arc<Config>, cli: &Cli) {
    let cfg = Arc::new(FinderConfig::new(
        &config.variable_contexts,
        &config.function_contexts,
        &config.class_contexts,
        &config.context_match_mode,
    ));

    let python_files: Vec<String> = collect_files(&cli.check_args.paths)
        .iter()
        .filter(|f| finder::is_python_file(f))
        .filter_map(|f| f.to_str())
        .map(std::string::ToString::to_string)
        .collect();

    if !config.parallel_processing {
        for file_path in &python_files {
            process_file(file_path, cfg.clone(), &config.clone());
        }
    } else if config.parallel_processing {
        let max_threads = if config.max_threads == 0 {
            std::cmp::max(
                1,
                std::thread::available_parallelism()
                    .map(std::num::NonZero::get)
                    .unwrap_or(5)
                    - 1,
            )
        } else {
            config.max_threads
        };

        let chunk_size = std::cmp::max(1, python_files.len() / max_threads);

        python_files
            .chunks(chunk_size)
            .map(<[std::string::String]>::to_vec)
            .collect::<Vec<Vec<String>>>()
            .into_iter()
            .map(|chunk| {
                let cfg = cfg.clone();
                let app_cfg = config.clone();
                thread::spawn(move || {
                    for file_path in chunk {
                        process_file(&file_path, cfg.clone(), &app_cfg.clone());
                    }
                })
            })
            .collect::<Vec<thread::JoinHandle<()>>>()
            .into_iter()
            .for_each(|handle| {
                let _ = handle.join();
            });
    }
}

fn process_file(file_path: &str, cfg: Arc<FinderConfig>, app_cfg: &Arc<Config>) {
    let sql_finder = SqlFinder::new(cfg);
    if let Some(sql_extract) = sql_finder.analyze_file(file_path) {
        let analyzer = analyzer::SqlAnalyzer::new(&analyzer::SqlDialect::PostgreSQL);
        dbg!("{}", &sql_extract);
        analyzer.analyze_sql_extract(&sql_extract, app_cfg);
    }
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

fn load_config(cli: &Cli) -> Config {
    let config_path = env::current_dir()
        .expect("Unable to read current working directory")
        .join(DEFAULT_CONFIG_NAME);
    let mut config = Config::default();
    Config::from_file(&config_path).map_or_else(
        |e| {
            println!("Failed reading config file, using default config.");
            println!("{e}")
        },
        |file_config| config.merge_with(file_config),
    );
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

fn handle_config(args: &ConfigArgs, config: &Config) {
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
