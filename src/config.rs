use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

pub const DEFAULT_CONFIG_NAME: &str = "statiql-config.toml";

/// Configuration for SQL detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Exact variable names to check for SQL content
    pub variable_names: Vec<String>,
    /// Minimum SQL content length to consider
    pub min_sql_length: usize,
    /// Case sensitive variable name matching
    pub case_sensitive: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            variable_names: vec![
                "query".to_string(),
                "sql".to_string(),
                "statement".to_string(),
                "cmd".to_string(),
                "command".to_string(),
                "sql_query".to_string(),
                "db_query".to_string(),
                "select_query".to_string(),
                "insert_query".to_string(),
                "update_query".to_string(),
                "delete_query".to_string(),
            ],
            min_sql_length: 10,
            case_sensitive: false,
        }
    }
}

impl Config {
    /// Load configuration from a TOML file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let content = fs::read_to_string(path)
            .map_err(|e| ConfigError::Io(format!("Failed to read config file: {}", e)))?;

        Self::from_toml(&content)
    }

    /// Parse configuration from TOML string
    pub fn from_toml(toml_content: &str) -> Result<Self, ConfigError> {
        toml::from_str(toml_content)
            .map_err(|e| ConfigError::Parse(format!("Failed to parse TOML: {}", e)))
    }

    /// Save configuration to a TOML file
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), ConfigError> {
        let toml_content = toml::to_string_pretty(self)
            .map_err(|e| ConfigError::Serialize(format!("Failed to serialize config: {}", e)))?;

        fs::write(path, toml_content)
            .map_err(|e| ConfigError::Io(format!("Failed to write config file: {}", e)))?;

        Ok(())
    }

    /// Check if a variable name should be analyzed for SQL
    pub fn should_analyze_variable(&self, variable_name: &str) -> bool {
        if self.case_sensitive {
            self.variable_names.contains(&variable_name.to_string())
        } else {
            let name_lower = variable_name.to_lowercase();
            self.variable_names
                .iter()
                .any(|pattern| pattern.to_lowercase() == name_lower)
        }
    }

    /// Check if content meets minimum length requirement
    pub fn meets_min_length(&self, content: &str) -> bool {
        content.trim().len() >= self.min_sql_length
    }

    /// Generate a default configuration file
    pub fn generate_default_config_file<P: AsRef<Path>>(path: P) -> Result<(), ConfigError> {
        let default_config = Self::default();
        default_config.save_to_file(path)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(String),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Serialize error: {0}")]
    Serialize(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.min_sql_length, 10);
        assert!(!config.case_sensitive);
        assert!(config.variable_names.contains(&"query".to_string()));
    }

    #[test]
    fn test_should_analyze_variable() {
        let config = Config::default();

        assert!(config.should_analyze_variable("query"));
        assert!(config.should_analyze_variable("sql"));
        assert!(config.should_analyze_variable("QUERY"));
        assert!(!config.should_analyze_variable("username"));
        assert!(!config.should_analyze_variable("data"));
    }

    #[test]
    fn test_case_sensitivity() {
        let case_sensitive_config = Config {
            variable_names: vec!["Query".to_string()],
            case_sensitive: true,
            min_sql_length: 1,
        };

        let case_insensitive_config = Config {
            variable_names: vec!["Query".to_string()],
            case_sensitive: false,
            min_sql_length: 1,
        };

        // Case sensitive should only match exact case
        assert!(case_sensitive_config.should_analyze_variable("Query"));
        assert!(!case_sensitive_config.should_analyze_variable("query"));
        assert!(!case_sensitive_config.should_analyze_variable("QUERY"));

        // Case insensitive should match all cases
        assert!(case_insensitive_config.should_analyze_variable("Query"));
        assert!(case_insensitive_config.should_analyze_variable("query"));
        assert!(case_insensitive_config.should_analyze_variable("QUERY"));
    }

    #[test]
    fn test_min_length() {
        let config = Config::default();

        assert!(config.meets_min_length("SELECT * FROM users"));
        assert!(!config.meets_min_length("test"));
        assert!(!config.meets_min_length("   "));
    }
}
