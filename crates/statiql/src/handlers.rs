use std::sync::Arc;
use std::thread;

use finder::collect_files;
use logging::{always_log, info};

pub fn handle_check(config: &Arc<crate::Config>, cli: &crate::Cli) {
    let cfg = Arc::new(finder::FinderConfig::new(
        &config.variable_contexts,
        &config.function_contexts,
        &config.class_contexts,
        &config.context_match_mode,
    ));

    let all_python_files: Vec<String> = collect_files(&cli.check_args.paths)
        .iter()
        .filter(|f| finder::is_python_file(f))
        .filter_map(|f| match std::fs::canonicalize(f) {
            Ok(canonical_path) => Some(canonical_path),
            Err(e) => {
                always_log!("Failed to canonicalize path '{}': {}", f.display(), e);
                None
            }
        })
        .map(|f| f.to_string_lossy().to_string())
        .collect();

    if all_python_files.is_empty() {
        always_log!("No Python files found in the specified paths.");
        return;
    }

    let python_files = crate::files::filter_incremental_files(
        &all_python_files,
        config.incremental_mode,
        config.include_staged,
        &config.baseline_branch,
    );

    if python_files.is_empty() {
        always_log!("No files to process after filtering.");
        return;
    }

    if config.incremental_mode {
        always_log!(
            "Running in incremental mode against baseline branch '{}'.",
            config.baseline_branch
        );
    }

    if config.parallel_processing {
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

        info!(
            "Processing {} files in parallel using {} threads...",
            python_files.len(),
            max_threads
        );

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
                if handle.join().is_err() {
                    always_log!("Warning: A worker thread panicked during processing.");
                }
            });
    } else {
        info!("Processing {} files sequentially...", python_files.len());
        for file_path in &python_files {
            process_file(file_path, cfg.clone(), &config.clone());
        }
    }
    info!("Analysis complete.");
}

fn process_file(file_path: &str, cfg: Arc<crate::FinderConfig>, app_cfg: &Arc<crate::Config>) {
    let sql_finder = finder::SqlFinder::new(cfg);
    if let Some(sql_extract) = sql_finder.analyze_file(file_path) {
        let analyzer = crate::analyzer::SqlAnalyzer::new(&crate::analyzer::SqlDialect::PostgreSQL);
        analyzer.analyze_sql_extract(&sql_extract, app_cfg);
    }
}

pub fn handle_init() {
    let path = std::env::current_dir()
        .expect("Failed fetching current working directory.")
        .join(crate::DEFAULT_CONFIG_NAME);

    if path.exists() {
        always_log!(
            "Configuration file already exists at '{}'. Not overwriting.",
            path.display()
        );
        return;
    }

    match std::fs::write(&path, crate::DEFAULT_CONFIG) {
        Ok(()) => always_log!(
            "Created default configuration file at '{}'.",
            path.display()
        ),
        Err(e) => always_log!(
            "Failed to create configuration file at '{}': {}. Check file permissions.",
            path.display(),
            e
        ),
    }
}
