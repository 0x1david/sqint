use crate::config::{Config, DEFAULT_CONFIG_NAME};
use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::WalkBuilder;
use logging::{always_log, debug, error, warn};
use std::{path::PathBuf, process::Command};

/// Returns only files that have changed compared to the baseline branch
pub fn filter_incremental_files(files: &[String], cfg: &Config) -> Vec<String> {
    if !cfg.incremental_mode {
        return files.to_vec();
    }

    let changed_files =
        get_changed_files(&cfg.baseline_branch, cfg.include_staged).unwrap_or_else(|_| {
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
            cfg.baseline_branch
        );
    }

    filtered_files
}

/// Get files that have changed compared to the baseline branch
fn get_changed_files(base_branch: &str, incl_staged: bool) -> Result<Vec<String>, String> {
    let committed_output = Command::new("git")
        .args(["diff", "--name-only", base_branch])
        .output()
        .map_err(|e| {
            format!("Failed to run git diff against '{base_branch}': {e}. Ensure git is installed and you're in a git repository.")
        })?;
    if !committed_output.status.success() {
        Err(format!(
            "Git diff command failed: {}. Ensure '{base_branch}' is a valid branch/commit.",
            String::from_utf8_lossy(&committed_output.stderr).trim(),
        ))?;
    }

    let mut all_files: Vec<String> = String::from_utf8_lossy(&committed_output.stdout)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(std::string::ToString::to_string)
        .collect();

    if incl_staged {
        let staged_output = Command::new("git")
            .args(["diff", "--name-only", "--cached"])
            .output()
            .map_err(|e| format!("Failed to run git diff --cached: {e}"))?;
        if !staged_output.status.success() {
            Err(format!(
                "Git diff --cached failed: {}",
                String::from_utf8_lossy(&staged_output.stderr).trim()
            ))?;
        }
        let staged_files: Vec<String> = String::from_utf8_lossy(&staged_output.stdout)
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(std::string::ToString::to_string)
            .collect();
        all_files.extend(staged_files);
    }

    Ok(all_files
        .into_iter()
        .map(|file| {
            std::fs::canonicalize(&file)
                .ok()
                .and_then(|path| path.to_str().map(std::string::ToString::to_string))
                .unwrap_or(file)
        })
        .collect())
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
pub fn collect_files(paths: &[PathBuf], cfg: &Config) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut files = Vec::new();
    let mut explicits = Vec::new();

    for path in paths {
        if path.is_file() {
            debug!("Found explicit file {}", path.display());
            explicits.push(path.clone());
        } else if path.is_dir() {
            WalkBuilder::new(path)
                .git_ignore(cfg.respect_gitignore)
                .git_global(cfg.respect_global_gitignore)
                .git_exclude(cfg.respect_git_exclude)
                .hidden(!cfg.include_hidden_files)
                .build()
                .filter_map(|found_path| {
                    found_path
                        .map_err(|e| always_log!("Failed to read directory entry: {}", e))
                        .ok()
                })
                .filter(|entry| entry.file_type().is_some_and(|ft| ft.is_file()))
                .for_each(|entry| {
                    files.push(entry.path().to_path_buf());
                });
        }
    }

    (files, explicits)
}

pub fn canonicalize_files(files: Vec<std::path::PathBuf>) -> Vec<String> {
    files
        .into_iter()
        .filter_map(|f| {
            std::fs::canonicalize(&f)
                .map_err(|e| warn!("Failed to canonicalize path '{}': {e}", f.display()))
                .ok()
        })
        .map(|f| f.to_string_lossy().to_string())
        .collect()
}

pub fn filter_file_pats(files: Vec<String>, cfg: &Config) -> Vec<String> {
    let include_pats: GlobSet = slice_to_glob(&cfg.file_patterns, "file_patterns");
    let exclude_pats: GlobSet = slice_to_glob(&cfg.exclude_patterns, "exclude_patterns");

    files
        .into_iter()
        .filter(|f| {
            if !include_pats.is_match(f) || exclude_pats.is_match(f) {
                debug!("File '{f}' filtered out by include/exclude patterns");
                return false;
            }
            true
        })
        .collect()
}

fn slice_to_glob(patterns: &[String], log_ctx: &str) -> GlobSet {
    patterns
        .iter()
        .filter_map(|p| {
            Glob::new(p)
                .map_err(|e| always_log!("Failed to parse {log_ctx} glob pattern '{p}': {e}"))
                .ok()
        })
        .fold(GlobSetBuilder::new(), |mut b, g| {
            b.add(g);
            b
        })
        .build()
        .unwrap_or_else(|e| {
            error!("Failed to build GlobSet for {log_ctx}: {e}");
            GlobSetBuilder::new().build().unwrap()
        })
}
