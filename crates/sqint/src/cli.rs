use crate::config::Config;

use super::config::DEFAULT_CONFIG_NAME;
use clap::{Args, Parser, Subcommand};
use logging::LogLevel;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "staticql")]
#[command(about = "A linter for SQL code embedded in Python files")]
#[command(version = "0.0.1")]
#[command(author = "David Bousi <bousi.david@pm.com>")]
#[allow(clippy::struct_excessive_bools)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    #[arg(short, long, global = true)]
    pub config: Option<PathBuf>,

    /// Use plain output format (default: colored)
    #[arg(long, global = true)]
    pub plain: bool,

    #[arg(short, long, global = true, value_enum)]
    pub loglevel: Option<LogLevel>,

    #[arg(long, global = true)]
    pub incremental: bool,

    #[arg(long, global = true)]
    pub baseline_branch: Option<String>,

    #[arg(long, global = true)]
    pub include_staged: bool,

    #[arg(long, global = true)]
    pub include_hidden_files: bool,

    #[command(flatten)]
    pub check_args: CheckArgs,
}

impl Cli {
    pub fn merge_with_config(&self, cfg: Config) -> Config {
        Config {
            variable_contexts: cfg.variable_contexts,
            baseline_branch: self.baseline_branch.clone().unwrap_or(cfg.baseline_branch),
            dialect: cfg.dialect,
            dialect_mappings: cfg.dialect_mappings,
            exclude_patterns: cfg.exclude_patterns,
            file_patterns: cfg.file_patterns,
            raw_sql_file_patterns: cfg.raw_sql_file_patterns,
            function_contexts: cfg.function_contexts,
            include_hidden_files: self.include_hidden_files || cfg.include_hidden_files,
            include_staged: self.include_staged || cfg.include_staged,
            incremental_mode: self.incremental || cfg.incremental_mode,
            loglevel: self.loglevel.unwrap_or(cfg.loglevel),
            max_threads: self.check_args.max_threads.unwrap_or(cfg.max_threads),
            parallel_processing: self
                .check_args
                .parallel_processing
                .unwrap_or(cfg.parallel_processing),
            param_markers: cfg.param_markers,
            respect_git_exclude: cfg.respect_git_exclude,
            respect_gitignore: self
                .check_args
                .respect_gitignore
                .unwrap_or(cfg.respect_gitignore),
            respect_global_gitignore: cfg.respect_global_gitignore,
        }
    }

    /// Returns true if colored output should be used
    pub const fn use_colored_output(&self) -> bool {
        !self.plain
    }
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// DEFAULT, Check Python files for SQL issues
    Check(CheckArgs),
    /// Initialize a new configuration file
    Init(InitArgs),
}

#[derive(Args, Debug)]
pub struct CheckArgs {
    #[arg(value_name = "PATH", default_value = ".")]
    pub paths: Vec<PathBuf>,

    /// File patterns to exclude (e.g., "test_*.py")
    #[arg(long, value_delimiter = ',')]
    pub exclude: Vec<String>,

    #[arg(long, value_enum)]
    pub dialect: Option<SqlDialect>,

    #[arg(long)]
    pub parallel_processing: Option<bool>,

    /// Maximum number of threads to use (0 = auto-detect)
    #[arg(long)]
    pub max_threads: Option<usize>,

    #[arg(long)]
    pub respect_gitignore: Option<bool>,

    /// File patterns to include (e.g., "*.py,*.pyi")
    #[arg(long, value_delimiter = ',')]
    pub file_patterns: Vec<String>,

    /// File patterns that are raw sql to parse
    #[arg(long, value_delimiter = ',')]
    pub sql_patterns: Vec<PathBuf>,

    /// Variable name patterns to look for (e.g., "*query*,*sql*")
    #[arg(long, value_delimiter = ',')]
    pub variable_contexts: Vec<String>,

    /// Function names with arguments to validate (e.g., "execute,execute_*,fetchall")
    #[arg(long, value_delimiter = ',')]
    pub function_contexts: Vec<String>,
}

#[derive(Args, Debug)]
pub struct InitArgs {
    /// Path where to create the configuration file
    #[arg(short, long, default_value = DEFAULT_CONFIG_NAME)]
    pub output: PathBuf,

    /// Overwrite existing configuration file
    #[arg(long)]
    pub force: bool,
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum SqlDialect {
    Generic,
    PostgreSQL,
    Oracle,
    SQLite,
    Ansi,
    BigQuery,
    ClickHouse,
    DuckDb,
    Hive,
    MsSql,
    MySql,
    RedshiftSql,
    Snowflake,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn verify_cli() {
        Cli::command().debug_assert()
    }
}
