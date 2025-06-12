use crate::{debug, error, log};
use rustpython_parser::{Parse, ast};
use std::{
    fmt, fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct SqlExtract {
    file_path: String,
    strings: Vec<SqlString>,
}

/// Represents a detected SQL variable
#[derive(Debug, Clone)]
pub struct SqlString {
    byte_offset: usize,
    variable_name: String,
    sql_content: String,
}

#[derive(Debug, Clone)]
pub struct FinderConfig {
    pub variables: Vec<String>,
    pub min_sql_length: usize,
}

pub struct SqlFinder {
    config: FinderConfig,
}

impl SqlFinder {
    pub fn new(config: FinderConfig) -> Self {
        Self { config }
    }

    pub fn analyze_file(&self, file_path: &str) -> Option<SqlExtract> {
        let source_code = fs::read_to_string(file_path)
            .inspect_err(|e| error!("Failed to read file '{}': {}", file_path, e))
            .ok()?;

        let parsed = ast::Suite::parse(&source_code, file_path)
            .inspect_err(|e| error!("Failed to parse Python file: {}", e))
            .ok()?;

        let mut contexts = Vec::new();
        self.analyze_stmts(&parsed, file_path, &mut contexts);

        Some(SqlExtract {
            file_path: file_path.to_string(),
            strings: contexts,
        })
    }

    fn analyze_stmts(&self, suite: &ast::Suite, file_path: &str, contexts: &mut Vec<SqlString>) {
        for stmt in suite {
            match stmt {
                ast::Stmt::Assign(assign) => {
                    self.analyze_assignment(assign, file_path, contexts);
                }
                _ => {} // TODO: Add more query detection contexts
            }
        }
    }

    fn analyze_assignment(
        &self,
        assign: &ast::StmtAssign,
        file_path: &str,
        contexts: &mut Vec<SqlString>,
    ) {
        // TODO: Add multi-assignment support
        if assign.targets.len() != 1 {
            return;
        }

        let ast::Expr::Name(name) = &assign.targets[0] else {
            return;
        };

        if !self.is_sql_variable_name(&name.id) {
            return;
        }

        let Some(sql_content) = self.extract_string_content(&assign.value) else {
            return;
        };

        contexts.push(SqlString {
            byte_offset: assign.range.start().to_usize(),
            variable_name: name.id.to_string(),
            sql_content,
        });
    }

    /// Extract string content from an expression (only handles string literals)
    fn extract_string_content(&self, expr: &ast::Expr) -> Option<String> {
        match expr {
            ast::Expr::Constant(constant) => match &constant.value {
                ast::Constant::Str(s) => Some(s.clone()),
                _ => None,
            },
            _ => None,
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

pub fn is_python_file(file: &Path) -> bool {
    let b = file.extension().and_then(|ext| ext.to_str()) == Some("py");
    if !b {
        debug!("Not a python file {}", file.display());
    } else {
        debug!("Reading a python file {}", file.display());
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
            writeln!(f, "{}", sql_string)?;
        }
        Ok(())
    }
}
