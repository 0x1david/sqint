use logging::{always_log, info, return_log};
use std::sync::Arc;
use std::thread;

#[allow(clippy::too_many_lines)]
pub fn handle_check(config: &Arc<crate::Config>, cli: &crate::Cli) {
    let cfg = Arc::new(finder::FinderConfig::new(
        &config.variable_contexts,
        &config.function_contexts,
        &config.class_contexts,
    ));

    let (found_files, explicit_files) = crate::files::collect_files(&cli.check_args.paths, config);

    let explicit_files = crate::files::canonicalize_files(explicit_files);
    let found_files = crate::files::canonicalize_files(found_files);
    let no_of_files = found_files.len() + explicit_files.len();

    if found_files.is_empty() && explicit_files.is_empty() {
        return_log!("No target files found in the specified paths.");
    }

    let target_files: Vec<String> = crate::files::filter_incremental_files(&found_files, config);
    let target_files: Vec<String> = crate::files::filter_file_pats(target_files, config)
        .into_iter()
        .chain(explicit_files)
        .collect();

    let no_remaining = target_files.len();

    if no_remaining == 0 {
        return_log!("No files to process after filtering.");
    }

    if config.parallel_processing {
        let max_threads = if config.max_threads == 0 {
            std::thread::available_parallelism()
                .map(std::num::NonZero::get)
                .unwrap_or(5)
                - 1
        } else {
            info!("Using configured thread count: {}", config.max_threads);
            config.max_threads
        };

        let chunk_size = std::cmp::max(1, target_files.len() / max_threads);
        target_files
            .chunks(chunk_size)
            .map(|chunk| {
                let chunk_vec = chunk.to_vec();
                let cfg = cfg.clone();
                let app_cfg = config.clone();
                thread::spawn(move || {
                    for file_path in chunk_vec {
                        process_file(&file_path, cfg.clone(), &app_cfg.clone());
                    }
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .for_each(|handle| handle.join().unwrap());
    } else {
        for file_path in &target_files {
            process_file(file_path, cfg.clone(), &config.clone());
        }
    }

    always_log!("Analysis complete. Processed {} files.", target_files.len());
}

fn process_file(file_path: &str, cfg: Arc<crate::FinderConfig>, app_cfg: &Arc<crate::Config>) {
    let mut sql_finder = finder::SqlFinder::new(cfg);

    let Some(sql_extract) = sql_finder.analyze_file(file_path) else {
        return;
    };

    let analyzer = crate::analyzer::SqlAnalyzer::new(
        &crate::analyzer::SqlDialect::Generic, // TODO
        app_cfg.dialect_mappings.clone(),
        &app_cfg.param_markers,
    );

    analyzer.analyze_sql_extract(&sql_extract);
}

pub fn handle_init() {
    let current_dir = match std::env::current_dir() {
        Ok(dir) => dir,
        Err(e) => {
            return_log!("Failed to get current working directory: {e}");
        }
    };

    let path = current_dir.join(crate::DEFAULT_CONFIG_NAME);

    if path.exists() {
        return_log!(
            "Configuration file already exists at '{}'. Not overwriting.",
            path.display()
        );
    }

    match std::fs::write(&path, crate::DEFAULT_CONFIG) {
        Ok(()) => {
            always_log!(
                "Created default configuration file at '{}'.",
                path.display()
            );
        }
        Err(e) => {
            always_log!(
                "Failed to create configuration file at '{}': {e}. Check file permissions.",
                path.display(),
            );
        }
    }
}
