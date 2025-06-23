use std::process::Command;

use logging::always_log;
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
        .filter(|file| changed_files.contains(*file))
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
