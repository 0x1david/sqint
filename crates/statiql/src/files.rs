use crate::config::{Config, DEFAULT_CONFIG_NAME};
use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::WalkBuilder;
use logging::{always_log, debug, error, warn};
use std::{path::PathBuf, process::Command};

/// Returns only files that have changed compared to the baseline branch
pub fn filter_incremental_files(
    files: &[String],
    incr_mode: bool,
    incl_staged: bool,
    base_branch: &str,
) -> Vec<String> {
    if !incr_mode {
        return files.to_vec();
    }

    let changed_files = get_changed_files(base_branch, incl_staged).unwrap_or_else(|| {
        always_log!(
            "Git operations failed. Running in non-incremental mode - processing all files."
        );
        files.to_vec()
    });

    let filtered_files: Vec<String> = files
        .iter()
        .filter(|file| {
            changed_files
                .iter()
                .any(|changed| std::path::Path::new(file).ends_with(changed))
        })
        .cloned()
        .collect();

    if filtered_files.is_empty() && !files.is_empty() {
        always_log!(
            "No changed files found. All files are up-to-date with baseline branch '{}'.",
            base_branch,
        );
    } else if filtered_files.len() < files.len() {
        always_log!(
            "Processing {} changed files out of {} total files.",
            filtered_files.len(),
            files.len()
        );
    }

    filtered_files
}

/// Get files that have changed compared to the baseline branch
fn get_changed_files(base_branch: &str, incl_staged: bool) -> Option<Vec<String>> {
    let mut changed_files = vec![];

    let committed_output = Command::new("git")
        .args(["diff", "--name-only", base_branch])
        .output()
        .map_err(|e| {
            always_log!("Failed to run git diff against '{}': {}. Ensure git is installed and you're in a git repository.", base_branch, e);
        })
        .ok()?;

    if !committed_output.status.success() {
        always_log!(
            "Git diff command failed: {}. Ensure '{}' is a valid branch/commit.",
            String::from_utf8_lossy(&committed_output.stderr).trim(),
            base_branch
        );
        return None;
    }

    let committed_files = String::from_utf8_lossy(&committed_output.stdout);
    for file in committed_files.lines() {
        if !file.trim().is_empty() {
            changed_files.push(file.trim().to_string());
        }
    }

    if incl_staged {
        let staged_output = Command::new("git")
            .args(["diff", "--name-only", "--cached"])
            .output()
            .map_err(|e| {
                always_log!("Failed to run git diff --cached: {}", e);
            })
            .ok()?;

        if !staged_output.status.success() {
            always_log!(
                "Git diff --cached failed: {}",
                String::from_utf8_lossy(&staged_output.stderr).trim()
            );
            return None;
        }

        let staged_files = String::from_utf8_lossy(&staged_output.stdout);
        for file in staged_files.lines() {
            if !file.trim().is_empty() {
                changed_files.push(file.trim().to_string());
            }
        }
    }

    let mut absolute_changed_files = vec![];
    for file in changed_files {
        if let Ok(absolute_path) = std::fs::canonicalize(&file) {
            if let Some(path_str) = absolute_path.to_str() {
                absolute_changed_files.push(path_str.to_string());
            }
        } else {
            // If canonicalize fails, keep the original path - this might happen for deleted files
            absolute_changed_files.push(file);
        }
    }

    Some(absolute_changed_files)
}

pub fn load_config() -> Config {
    let config_path = std::env::current_dir()
        .expect("Unable to read current working directory")
        .join(DEFAULT_CONFIG_NAME);
    let mut config = Config::default();

    Config::from_file(&config_path).map_or_else(
        |e| {
            always_log!(
                "Using default configuration. Couldn't load config from {}: '{e}'.",
                config_path.display(),
            );
        },
        |file_config| config.merge_with(file_config),
    );
    config
}

#[must_use]
#[allow(clippy::fn_params_excessive_bools)]
pub fn collect_files(
    paths: &[PathBuf],
    respect_gitignore: bool,
    respect_global_gitignore: bool,
    respect_git_exclude: bool,
    include_hidden: bool,
) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut files = Vec::new();
    let mut explicits = Vec::new();

    for path in paths {
        if path.is_file() {
            debug!("Found explicit file {}", path.display());
            explicits.push(path.clone());
        } else if path.is_dir() {
            let builder = WalkBuilder::new(path)
                .git_ignore(respect_gitignore)
                .git_global(respect_global_gitignore)
                .git_exclude(respect_git_exclude)
                .hidden(!include_hidden)
                .build();

            for res in builder {
                match res {
                    Ok(entry) => {
                        let entry_path = entry.path();
                        if entry.file_type().is_some_and(|ft| ft.is_file()) {
                            debug!("Found file {}", entry_path.display());
                            files.push(entry_path.to_path_buf());
                        }
                    }
                    Err(e) => {
                        always_log!("Failed to read directory entry: {}", e);
                    }
                }
            }
        }
    }

    (files, explicits)
}

pub fn canonicalize_files(files: Vec<std::path::PathBuf>) -> Vec<String> {
    files
        .into_iter()
        .filter_map(|f| match std::fs::canonicalize(&f) {
            Ok(canonical_path) => {
                debug!(
                    "Canonicalized path: {} -> {}",
                    f.display(),
                    canonical_path.display()
                );
                Some(canonical_path)
            }
            Err(e) => {
                warn!("Failed to canonicalize path '{}': {}", f.display(), e);
                None
            }
        })
        .map(|f| f.to_string_lossy().to_string())
        .collect()
}

pub fn filter_file_pats(
    files: Vec<String>,
    include_pats: &[String],
    exclude_pats: &[String],
) -> Vec<String> {
    let include_pats: GlobSet = slice_to_glob(include_pats, "file_patterns");
    let exclude_pats: GlobSet = slice_to_glob(exclude_pats, "exclude_patterns");

    files
        .into_iter()
        .filter(|f| {
            let matches = include_pats.is_match(f);
            if !matches {
                debug!("File '{f}' filtered out by include patterns");
            }
            matches
        })
        .filter(|f| {
            let excluded = exclude_pats.is_match(f);
            if excluded {
                debug!("File '{f}' filtered out by exclude patterns");
            }
            !excluded
        })
        .collect()
}

fn slice_to_glob(patterns: &[String], log_ctx: &str) -> GlobSet {
    let valid_globs: Vec<Glob> = patterns
        .iter()
        .filter_map(|pattern| match Glob::new(pattern) {
            Ok(glob) => Some(glob),
            Err(e) => {
                always_log!("Failed to parse {log_ctx} glob pattern '{pattern}': {e}");
                None
            }
        })
        .collect();

    if valid_globs.is_empty() {
        debug!(
            "GlobSet for {log_ctx} is empty - no valid patterns found from {} input patterns",
            patterns.len()
        );
    }

    let builder = valid_globs
        .into_iter()
        .fold(GlobSetBuilder::new(), |mut builder, glob| {
            builder.add(glob);
            builder
        });

    builder.build().unwrap_or_else(|e| {
        error!("Failed to build GlobSet for {log_ctx}: {e}");
        GlobSetBuilder::new().build().unwrap()
    })
}
