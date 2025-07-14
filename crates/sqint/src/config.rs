use logging::LogLevel;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub const DEFAULT_CONFIG_NAME: &str = "sqint.toml";
pub const PYPROJECT_CONFIG_NAME: &str = "pyproject.toml";
pub const DEFAULT_CONFIG: &str = include_str!("./assets/default.toml");

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    // Detection Settings
    pub variable_contexts: Vec<String>,
    pub function_contexts: Vec<String>,

    // File Processing
    pub file_patterns: Vec<String>,
    pub raw_sql_file_patterns: Vec<String>,
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
    pub dialect: String,
    pub param_markers: Vec<String>,
    pub dialect_mappings: HashMap<String, String>,
}

/// Wrapper for pyproject.toml structure
#[derive(Debug, Deserialize)]
struct PyprojectToml {
    tool: Option<ToolConfig>,
}

/// Tool configuration section in pyproject.toml
#[derive(Debug, Deserialize)]
struct ToolConfig {
    sqint: Option<Config>,
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

            // File Processing
            file_patterns: vec![
                "*.py".to_string(),
                "*.pyi".to_string(),
                "*.ipynb".to_string(),
            ],
            raw_sql_file_patterns: vec!["*.sql".to_string()],
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
            dialect: "generic".to_string(),
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
    /// Load configuration from a file, supporting both sqint.toml and pyproject.toml formats
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let content = fs::read_to_string(path)
            .map_err(|e| ConfigError::Io(format!("Failed to read config file: {e}")))?;

        if path.file_name().and_then(|name| name.to_str()) == Some(PYPROJECT_CONFIG_NAME) {
            Self::from_pyproject_toml(&content)
        } else {
            Self::from_toml(&content)
        }
    }

    pub fn from_toml(toml_content: &str) -> Result<Self, ConfigError> {
        toml::from_str(toml_content)
            .map_err(|e| ConfigError::Parse(format!("Failed to parse TOML: {e}")))
    }

    /// Parse configuration from pyproject.toml file
    pub fn from_pyproject_toml(toml_content: &str) -> Result<Self, ConfigError> {
        let pyproject: PyprojectToml = toml::from_str(toml_content)
            .map_err(|e| ConfigError::Parse(format!("Failed to parse pyproject.toml: {e}")))?;

        match pyproject.tool.and_then(|tool| tool.sqint) {
            Some(config) => Ok(config),
            None => Err(ConfigError::Parse(
                "No [tool.sqint] section found in pyproject.toml".to_string(),
            )),
        }
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

        // File Processing
        if !other.file_patterns.is_empty() {
            self.file_patterns = other.file_patterns;
        }
        if !other.raw_sql_file_patterns.is_empty() {
            self.raw_sql_file_patterns = other.raw_sql_file_patterns;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pyproject_toml_parsing() {
        let pyproject_content = r#"
[build-system]
requires = ["setuptools", "wheel"]

[tool.sqint]
variable_contexts = ["*query*", "*sql*", "*custom*"]
file_patterns = ["*.py", "*.sql"]
parallel_processing = true
max_threads = 4
loglevel = "info"

[tool.sqint.dialect_mappings]
NOTNULL = "NOT NULL"
ISNULL = "IS NULL"
"#;

        let config = Config::from_pyproject_toml(pyproject_content).unwrap();
        assert_eq!(
            config.variable_contexts,
            vec!["*query*", "*sql*", "*custom*"]
        );
        assert_eq!(config.file_patterns, vec!["*.py", "*.sql"]);
        assert_eq!(config.max_threads, 4);
        assert!(config.parallel_processing);
    }

    #[test]
    fn test_pyproject_toml_missing_section() {
        let pyproject_content = r#"
[build-system]
requires = ["setuptools", "wheel"]

[tool.other]
some_config = "value"
"#;

        let result = Config::from_pyproject_toml(pyproject_content);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("No [tool.sqint] section found")
        );
    }

    #[test]
    fn test_standalone_toml_parsing() {
        let toml_content = r#"
variable_contexts = ["*query*", "*sql*"]
file_patterns = ["*.py"]
parallel_processing = false
"#;

        let config = Config::from_toml(toml_content).unwrap();
        assert_eq!(config.variable_contexts, vec!["*query*", "*sql*"]);
        assert_eq!(config.file_patterns, vec!["*.py"]);
        assert!(!config.parallel_processing);
    }
}
