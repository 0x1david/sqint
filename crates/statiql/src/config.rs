use logging::LogLevel;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub const DEFAULT_CONFIG_NAME: &str = "statiql-config.toml";
pub const DEFAULT_CONFIG: &str = include_str!("./assets/default.toml");

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    // Detection Settings
    pub variable_contexts: Vec<String>,
    pub function_contexts: Vec<String>,
    pub class_contexts: Vec<String>,
    pub min_sql_length: usize,
    pub case_sensitive: bool,

    // File Processing
    pub file_patterns: Vec<String>,
    pub exclude_patterns: Vec<String>,
    pub respect_gitignore: bool,
    pub respect_global_gitignore: bool,
    pub respect_git_exclude: bool,
    pub include_hidden_files: bool,

    // Threading Settings
    pub parallel_processing: bool,
    pub max_threads: usize,

    // Incremental Mode
    pub incremental_mode: bool,
    pub baseline_branch: String,
    pub include_staged: bool,

    // Output Settings
    pub loglevel: LogLevel,

    // SQL Parsing Settings
    pub param_markers: Vec<String>,
    pub dialect_mappings: HashMap<String, String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            // Detection Settings
            variable_contexts: vec![
                "*query*".to_string(),
                "*sql*".to_string(),
                "*statement*".to_string(),
                "*stmt*".to_string(),
            ],
            function_contexts: vec![],
            class_contexts: vec![],
            min_sql_length: 10,
            case_sensitive: false,

            // File Processing
            file_patterns: vec![
                "*.py".to_string(),
                "*.pyi".to_string(),
                "*.ipynb".to_string(),
            ],
            exclude_patterns: vec![],
            respect_gitignore: true,
            respect_global_gitignore: false,
            respect_git_exclude: true,
            include_hidden_files: false,

            // Performance Settings
            parallel_processing: true,
            max_threads: 0,

            // Incremental Mode
            incremental_mode: false,
            baseline_branch: "main".to_string(),
            include_staged: true,

            // Output Settings
            loglevel: LogLevel::default(),

            // SQL Parsing Settings
            param_markers: vec!["?".to_string()],
            dialect_mappings: {
                let mut map = HashMap::new();
                map.insert("NOTNULL".to_string(), "NOT NULL".to_string());
                map.insert("ISNULL".to_string(), "IS NULL".to_string());
                map
            },
        }
    }
}

impl Config {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let content = fs::read_to_string(path)
            .map_err(|e| ConfigError::Io(format!("Failed to read config file: {e}")))?;
        Self::from_toml(&content)
    }

    pub fn from_toml(toml_content: &str) -> Result<Self, ConfigError> {
        toml::from_str(toml_content)
            .map_err(|e| ConfigError::Parse(format!("Failed to parse TOML: {e}")))
    }

    /// Merge this config with another, preferring values from the other config
    pub fn merge_with(&mut self, other: Self) {
        // Detection Settings
        if !other.variable_contexts.is_empty() {
            self.variable_contexts = other.variable_contexts;
        }
        if !other.function_contexts.is_empty() {
            self.function_contexts = other.function_contexts;
        }
        if !other.class_contexts.is_empty() {
            self.class_contexts = other.class_contexts;
        }
        if other.min_sql_length != 10 {
            self.min_sql_length = other.min_sql_length;
        }
        if other.case_sensitive {
            self.case_sensitive = other.case_sensitive;
        }

        // File Processing
        if !other.file_patterns.is_empty() {
            self.file_patterns = other.file_patterns;
        }
        if !other.exclude_patterns.is_empty() {
            self.exclude_patterns = other.exclude_patterns;
        }
        if other.respect_gitignore {
            self.respect_gitignore = other.respect_gitignore;
        }
        if other.respect_global_gitignore {
            self.respect_global_gitignore = other.respect_global_gitignore;
        }
        if other.respect_git_exclude {
            self.respect_git_exclude = other.respect_git_exclude;
        }
        if other.include_hidden_files {
            self.include_hidden_files = other.include_hidden_files;
        }

        // Threading Settings
        if other.parallel_processing {
            self.parallel_processing = other.parallel_processing;
        }
        if other.max_threads != 0 {
            self.max_threads = other.max_threads;
        }

        self.loglevel = other.loglevel;

        // Incremental Mode
        if other.incremental_mode {
            self.incremental_mode = other.incremental_mode;
        }
        if other.baseline_branch != "main" {
            self.baseline_branch = other.baseline_branch;
        }
        if other.include_staged {
            self.include_staged = other.include_staged;
        }

        // SQL Parsing Settings
        if !other.param_markers.is_empty() {
            self.param_markers = other.param_markers;
        }
        if !other.dialect_mappings.is_empty() {
            self.dialect_mappings = other.dialect_mappings;
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(String),
    #[error("Parse error: {0}")]
    Parse(String),
}
