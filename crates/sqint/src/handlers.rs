use logging::{always_log, error, info};
use std::sync::Arc;
use std::thread;

use crate::analyzer::SqlDialect;

#[allow(clippy::too_many_lines)]
pub fn handle_check(config: &Arc<crate::Config>, cli: &crate::Cli) {
    let cfg = Arc::new(finder::FinderConfig::new(
        &config.variable_contexts,
        &config.function_contexts,
    ));
    let (found_files, explicit_files) = crate::files::collect_files(&cli.check_args.paths, config);
    let explicit_files = crate::files::canonicalize_files(explicit_files);
    let found_files = crate::files::canonicalize_files(found_files);
    if found_files.is_empty() && explicit_files.is_empty() {
        always_log!("No target files found in the specified paths.");
        return;
    }
    let target_files: Vec<String> = crate::files::filter_incremental_files(&found_files, config);
    let (target_files, sql_files): (Vec<String>, Vec<String>) =
        crate::files::filter_file_pats(target_files, config);
    let target_files: Vec<String> = target_files.into_iter().chain(explicit_files).collect();
    let sql_files: Vec<String> = sql_files.into_iter().collect();

    let total_files = target_files.len() + sql_files.len();
    if total_files == 0 {
        always_log!("No files to process after filtering.");
        return;
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

        // Process Python files
        if !target_files.is_empty() {
            let chunk_size = std::cmp::max(1, target_files.len() / max_threads);
            target_files
                .chunks(chunk_size)
                .map(|chunk| {
                    let chunk_vec = chunk.to_vec();
                    let cfg = cfg.clone();
                    let app_cfg = config.clone();
                    thread::spawn(move || {
                        for file_path in chunk_vec {
                            process_file(&file_path, cfg.clone(), &app_cfg.clone(), false);
                        }
                    })
                })
                .collect::<Vec<_>>()
                .into_iter()
                .for_each(|handle| handle.join().unwrap());
        }

        // Process SQL files
        if !sql_files.is_empty() {
            let chunk_size = std::cmp::max(1, sql_files.len() / max_threads);
            sql_files
                .chunks(chunk_size)
                .map(|chunk| {
                    let chunk_vec = chunk.to_vec();
                    let cfg = cfg.clone();
                    let app_cfg = config.clone();
                    thread::spawn(move || {
                        for file_path in chunk_vec {
                            process_file(&file_path, cfg.clone(), &app_cfg.clone(), true);
                        }
                    })
                })
                .collect::<Vec<_>>()
                .into_iter()
                .for_each(|handle| handle.join().unwrap());
        }
    } else {
        for file_path in &target_files {
            process_file(file_path, cfg.clone(), &config.clone(), false);
        }

        for file_path in &sql_files {
            process_file(file_path, cfg.clone(), &config.clone(), true);
        }
    }

    always_log!(
        "Analysis complete. Processed {} files ({} Python, {} SQL).",
        total_files,
        target_files.len(),
        sql_files.len()
    );
}

fn process_file(
    file_path: &str,
    cfg: Arc<crate::FinderConfig>,
    app_cfg: &Arc<crate::Config>,
    is_raw_sql: bool,
) {
    let mut sql_finder = finder::SqlFinder::new(cfg);

    let Some(sql_extract) = sql_finder.analyze_file(file_path, is_raw_sql) else {
        return;
    };

    let Some(dialect) = SqlDialect::from_str(&app_cfg.dialect) else {
        error!(
            "Unknown dialect. Supported: {:?}",
            SqlDialect::supported_dialects()
        );
        return;
    };

    let analyzer = crate::analyzer::SqlAnalyzer::new(
        &dialect,
        app_cfg.dialect_mappings.clone(),
        &app_cfg.param_markers,
    );

    analyzer.analyze_sql_extract(&sql_extract);
}

pub fn handle_init() {
    let current_dir = match std::env::current_dir() {
        Ok(dir) => dir,
        Err(e) => {
            always_log!("Failed to get current working directory: {e}");
            return;
        }
    };

    let path = current_dir.join(crate::DEFAULT_CONFIG_NAME);

    if path.exists() {
        always_log!(
            "Configuration file already exists at '{}'. Not overwriting.",
            path.display()
        );
        return;
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
