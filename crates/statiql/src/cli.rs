use super::config::DEFAULT_CONFIG_NAME;
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "staticql")]
#[command(about = "A linter for SQL code embedded in Python files")]
#[command(version = "0.0.1")]
#[command(author = "David Bousi <bousi.david@pm.com>")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    #[arg(short, long, global = true)]
    pub config: Option<PathBuf>,

    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[arg(short, long, global = true, conflicts_with = "verbose")]
    pub quiet: bool,

    #[arg(short, long, global = true)]
    pub debug: bool,

    #[arg(long, global = true, value_enum, default_value = "colored")]
    pub format: OutputFormat,

    // Flatten CheckArgs so they appear at the top level when no subcommand is used
    #[command(flatten)]
    pub check_args: CheckArgs,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Check Python files for SQL issues
    Check(CheckArgs),

    /// Initialize a new configuration file
    Init(InitArgs),
}

#[derive(Args)]
pub struct CheckArgs {
    /// Python files or directories to check
    #[arg(value_name = "PATH")]
    pub paths: Vec<PathBuf>,

    /// File patterns to exclude (e.g., "test_*.py")
    #[arg(long, value_delimiter = ',')]
    pub exclude: Vec<String>,

    /// Exit with non-zero code if issues are found
    #[arg(long)]
    pub fail_on_issues: bool,

    /// Maximum number of issues to report (0 = unlimited)
    #[arg(long, default_value = "0")]
    pub max_issues: usize,

    /// Only report errors, not warnings
    #[arg(long)]
    pub errors_only: bool,
}

#[derive(Args)]
pub struct InitArgs {
    /// Path where to create the configuration file
    #[arg(short, long, default_value = DEFAULT_CONFIG_NAME)]
    pub output: PathBuf,

    /// Overwrite existing configuration file
    #[arg(long)]
    pub force: bool,
}

#[derive(Args)]
pub struct ConfigArgs {
    /// Show current configuration
    #[arg(long)]
    pub show: bool,

    /// Validate configuration file
    #[arg(long)]
    pub validate: bool,

    /// List all variable names that would be checked
    #[arg(long)]
    pub list_variables: bool,
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum OutputFormat {
    /// Colored terminal output
    Colored,
    /// Plain text output
    Plain,
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
