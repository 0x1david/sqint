mod assign;
mod formatters;
mod tests;
use logging::{debug, error};
use rustpython_parser::{
    Parse,
    ast::{self},
};
use std::{
    fmt, fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
struct SearchCtx {
    pub var_assign: bool,
}

#[derive(Debug, Clone)]
pub struct SqlExtract {
    pub file_path: String,
    pub strings: Vec<SqlString>,
}

/// Represents a detected SQL variable
#[derive(Debug, Clone)]
pub struct SqlString {
    pub byte_offset: usize,
    pub variable_name: String,
    pub sql_content: String,
}

#[derive(Debug, Clone)]
pub struct FinderConfig {
    pub variables: Vec<String>,
    pub min_sql_length: usize,
}

pub struct SqlFinder {
    config: FinderConfig,
    ctx: SearchCtx,
}

impl SqlFinder {
    #[must_use]
    pub fn new(config: FinderConfig) -> Self {
        Self {
            config,
            ctx: SearchCtx::default(),
        }
    }

    #[must_use]
    pub fn analyze_file(&self, file_path: &str) -> Option<SqlExtract> {
        let source_code = fs::read_to_string(file_path)
            .inspect_err(|e| error!("Failed to read file '{}': {}", file_path, e))
            .ok()?;

        let parsed = ast::Suite::parse(&source_code, file_path)
            .inspect_err(|e| error!("Failed to parse Python file: {}", e))
            .ok()?;

        let mut contexts = Vec::new();
        self.analyze_stmts(&parsed, &mut contexts);

        Some(SqlExtract {
            file_path: file_path.to_string(),
            strings: contexts,
        })
    }

    pub(crate) fn analyze_stmts(&self, suite: &ast::Suite, contexts: &mut Vec<SqlString>) {
        for stmt in suite {
            match stmt {
                ast::Stmt::Assign(assign) if self.ctx.var_assign => {
                    self.analyze_assignment(assign, contexts);
                }
                _ => {} // TODO: Add more query detection contexts
            }
        }
    }

    /// Check if variable name suggests it contains SQL
    fn is_sql_variable_name(&self, name: &str) -> bool {
        let name_lower = name.to_lowercase();
        self.config
            .variables
            .iter()
            .any(|pattern| name_lower.contains(&pattern.to_lowercase()))
    }
}

/// Collects and flattens all files in a list of files/directories
#[must_use]
pub fn collect_files(paths: &[PathBuf]) -> Vec<PathBuf> {
    debug!("{:?}", paths);
    paths
        .iter()
        .flat_map(|path| {
            if path.is_file() {
                debug!("Found file {}", path.display());
                vec![path.clone()]
            } else if path.is_dir() {
                read_directory_files(path)
            } else {
                Vec::new()
            }
        })
        .collect()
}

/// Reads all files in a directory
fn read_directory_files(dir_path: &Path) -> Vec<PathBuf> {
    match dir_path.read_dir() {
        Ok(entries) => entries
            .filter_map(|entry| match entry {
                Ok(entry) => Some(entry.path()),
                Err(e) => {
                    error!(
                        "Failed to read directory entry in {}: {}",
                        dir_path.display(),
                        e
                    );
                    None
                }
            })
            .collect(),
        Err(e) => {
            error!("Failed to read directory {}: {}", dir_path.display(), e);
            Vec::new()
        }
    }
}

#[must_use]
pub fn is_python_file(file: &Path) -> bool {
    let b = file.extension().and_then(|ext| ext.to_str()) == Some("py");
    if b {
        debug!("Reading a python file {}", file.display());
    } else {
        debug!("Not a python file {}", file.display());
    }
    b
}

impl fmt::Display for SqlString {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} = {}", self.variable_name, self.sql_content)
    }
}

impl fmt::Display for SqlExtract {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "{}", self.file_path)?;
        for sql_string in &self.strings {
            writeln!(f, "{sql_string}")?;
        }
        Ok(())
    }
}

impl Default for SearchCtx {
    fn default() -> Self {
        Self { var_assign: true }
    }
}
