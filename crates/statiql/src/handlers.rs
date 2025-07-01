use logging::{always_log, debug, error, info, warn};
use std::sync::Arc;
use std::thread;

#[allow(clippy::too_many_lines)]
pub fn handle_check(config: &Arc<crate::Config>, cli: &crate::Cli) {
    debug!(
        "Starting check handler with paths: {:?}",
        cli.check_args.paths
    );

    let cfg = Arc::new(finder::FinderConfig::new(
        &config.variable_contexts,
        &config.function_contexts,
        &config.class_contexts,
    ));

    let (found_files, explicit_files) = crate::files::collect_files(
        &cli.check_args.paths,
        config.respect_gitignore,
        config.respect_global_gitignore,
        config.respect_git_exclude,
        config.include_hidden_files,
    );

    let explicit_files: Vec<String> = crate::files::canonicalize_files(explicit_files);
    let found_files = crate::files::canonicalize_files(found_files);
    let no_of_files = found_files.len() + explicit_files.len();
    debug!("Found {no_of_files} files total");

    if found_files.is_empty() && explicit_files.is_empty() {
        always_log!("No target files found in the specified paths.");
        return;
    }

    let target_files: Vec<String> = crate::files::filter_incremental_files(
        &found_files,
        config.incremental_mode,
        config.include_staged,
        &config.baseline_branch,
    );
    let target_files: Vec<String> = crate::files::filter_file_pats(
        target_files,
        &config.file_patterns,
        &config.exclude_patterns,
    )
    .into_iter()
    .chain(explicit_files)
    .collect();

    debug!(
        "After incremental filtering: {} files remain",
        target_files.len()
    );

    if target_files.is_empty() {
        always_log!("No files to process after filtering.");
        return;
    }

    if config.incremental_mode {
        info!(
            "Running in incremental mode against baseline branch '{}'.",
            config.baseline_branch
        );
        debug!("Include staged files: {}", config.include_staged);
    }

    if config.parallel_processing {
        let max_threads = if config.max_threads == 0 {
            std::thread::available_parallelism()
                .map(std::num::NonZero::get)
                .unwrap_or(5)
                - 1
        } else {
            debug!("Using configured thread count: {}", config.max_threads);
            config.max_threads
        };

        info!(
            "Processing {} files in parallel using {max_threads} threads...",
            target_files.len(),
        );

        let chunk_size = std::cmp::max(1, target_files.len() / max_threads);
        debug!("Chunk size per thread: {chunk_size}");

        let handles: Vec<thread::JoinHandle<()>> = target_files
            .chunks(chunk_size)
            .enumerate()
            .map(|(i, chunk)| {
                let chunk_vec = chunk.to_vec();
                let cfg = cfg.clone();
                let app_cfg = config.clone();
                debug!("Starting thread {i} with {} files", chunk_vec.len());

                thread::spawn(move || {
                    debug!("Thread {i} processing files: {chunk_vec:?}");
                    for file_path in chunk_vec {
                        process_file(&file_path, cfg.clone(), &app_cfg.clone());
                    }
                    debug!("Thread {i} completed");
                })
            })
            .collect();

        let mut failed_threads = 0;
        for (i, handle) in handles.into_iter().enumerate() {
            if let Err(e) = handle.join() {
                error!("Worker thread {i} panicked during processing: {e:?}");
                failed_threads += 1;
            }
        }

        if failed_threads > 0 {
            warn!("{failed_threads} worker threads failed during parallel processing",);
        }
    } else {
        info!("Processing {} files sequentially...", target_files.len());
        for (i, file_path) in target_files.iter().enumerate() {
            debug!(
                "Processing file {}/{}: {file_path}",
                i + 1,
                target_files.len(),
            );
            process_file(file_path, cfg.clone(), &config.clone());
        }
    }

    always_log!("Analysis complete. Processed {} files.", target_files.len());
}

fn process_file(file_path: &str, cfg: Arc<crate::FinderConfig>, app_cfg: &Arc<crate::Config>) {
    debug!("Starting analysis of file: {file_path}");

    let mut sql_finder = finder::SqlFinder::new(cfg);
    if let Some(sql_extract) = sql_finder.analyze_file(file_path) {
        debug!("Found SQL extracts in {file_path}");
        let analyzer = crate::analyzer::SqlAnalyzer::new(
            &crate::analyzer::SqlDialect::PostgreSQL,
            app_cfg.dialect_mappings.clone(),
            &app_cfg.param_markers,
        );
        analyzer.analyze_sql_extract(&sql_extract);
    } else {
        debug!("No SQL found in file: {file_path}");
    }
}

pub fn handle_init() {
    debug!("Initializing configuration file");

    let current_dir = match std::env::current_dir() {
        Ok(dir) => {
            debug!("Current working directory: {}", dir.display());
            dir
        }
        Err(e) => {
            error!("Failed to get current working directory: {e}");
            return;
        }
    };

    let path = current_dir.join(crate::DEFAULT_CONFIG_NAME);
    debug!("Configuration file path: {}", path.display());

    if path.exists() {
        always_log!(
            "Configuration file already exists at '{}'. Not overwriting.",
            path.display()
        );
        info!("Use --force/-f flag to overwrite existing configuration.");
        return;
    }

    debug!(
        "Writing default configuration of {} bytes",
        crate::DEFAULT_CONFIG.len()
    );

    match std::fs::write(&path, crate::DEFAULT_CONFIG) {
        Ok(()) => {
            always_log!(
                "Created default configuration file at '{}'.",
                path.display()
            );
        }
        Err(e) => {
            error!(
                "Failed to create configuration file at '{}': {e}. Check file permissions.",
                path.display(),
            );
        }
    }
}
