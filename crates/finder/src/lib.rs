mod tests;
use logging::{always_log, debug, error};
use rustpython_parser::{
    Parse,
    ast::{self, Identifier},
};
use std::{
    fmt, fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
struct SearchCtx {
    pub var_assign: bool,
    pub multiple_var_assig: bool,
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
    pub fn new(config: FinderConfig) -> Self {
        Self {
            config,
            ctx: SearchCtx::default(),
        }
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

    pub(crate) fn analyze_stmts(
        &self,
        suite: &ast::Suite,
        file_path: &str,
        contexts: &mut Vec<SqlString>,
    ) {
        for stmt in suite {
            match stmt {
                ast::Stmt::Assign(assign) if self.ctx.var_assign => {
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
        for target in &assign.targets {
            self.process_assignment_target(
                target,
                &assign.value,
                assign.range.start().to_usize(),
                contexts,
            );
        }
    }

    fn process_by_ident(
        &self,
        name: &Identifier,
        value: &ast::Expr,
        byte_offset: usize,
        contexts: &mut Vec<SqlString>,
    ) {
        if self.is_sql_variable_name(name) {
            if let Some(sql_content) = self.extract_string_content(value) {
                contexts.push(SqlString {
                    byte_offset,
                    variable_name: name.to_string(),
                    sql_content,
                });
            }
        }
    }

    fn process_assignment_target(
        &self,
        target: &ast::Expr,
        value: &ast::Expr,
        byte_offset: usize,
        contexts: &mut Vec<SqlString>,
    ) {
        match target {
            ast::Expr::Name(name) => {
                self.process_by_ident(&name.id, value, byte_offset, contexts);
            }
            ast::Expr::Attribute(att) => {
                self.process_by_ident(&att.attr, value, byte_offset, contexts)
            }
            ast::Expr::Tuple(tuple) => {
                self.handle_tuple_assignment(&tuple.elts, value, byte_offset, contexts);
            }

            ast::Expr::List(list) => {
                self.handle_tuple_assignment(&list.elts, value, byte_offset, contexts);
            }

            // Other patterns like attribute access (obj.attr = ...) or subscript (arr[0] = ...)
            _ => {
                // Log unhandled patterns for debugging
                debug!("Unhandled assignment target pattern: {:?}", target);
            }
        }
    }
    fn handle_tuple_assignment(
        &self,
        targets: &[ast::Expr],
        value: &ast::Expr,
        byte_offset: usize,
        contexts: &mut Vec<SqlString>,
    ) {
        match value {
            ast::Expr::Tuple(tuple_value) => {
                self.process_paired_assignments(targets, &tuple_value.elts, byte_offset, contexts);
            }
            ast::Expr::List(list_value) => {
                self.process_paired_assignments(targets, &list_value.elts, byte_offset, contexts);
            }
            _ => {}
        }
    }

    fn process_paired_assignments(
        &self,
        targets: &[ast::Expr],
        values: &[ast::Expr],
        byte_offset: usize,
        contexts: &mut Vec<SqlString>,
    ) {
        // Process each target-value pair
        for (target, value) in targets.iter().zip(values.iter()) {
            self.process_assignment_target(target, value, byte_offset, contexts);
        }

        // Handle cases where there are more targets than values or vice versa
        if targets.len() != values.len() {
            always_log!(
                "Mismatched tuple assignment: {} targets, {} values",
                targets.len(),
                values.len()
            );
        }
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

impl Default for SearchCtx {
    fn default() -> Self {
        Self {
            var_assign: true,
            multiple_var_assig: true,
        }
    }
}
