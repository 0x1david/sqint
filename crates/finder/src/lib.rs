mod assign;
mod finder_types;
mod format;
mod tests;
pub use crate::finder_types::{FinderConfig, SqlExtract, SqlString};
use logging::{bail_with, debug, error};
use rustpython_parser::{
    Parse,
    ast::{self},
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

pub struct SqlFinder {
    config: Arc<FinderConfig>,
}

impl SqlFinder {
    #[must_use]
    pub fn new(config: Arc<FinderConfig>) -> Self {
        Self { config }
    }

    #[must_use]
    pub fn analyze_file(&self, file_path: &str) -> Option<SqlExtract> {
        let source_code = fs::read_to_string(file_path)
            .inspect_err(|e| error!("Failed to read file '{}': {}", file_path, e))
            .ok()?;

        let parsed = ast::Suite::parse(&source_code, file_path)
            .inspect_err(|e| error!("Failed to parse Python file: {}", e))
            .ok()?;

        let strings = self.analyze_stmts(&parsed);

        Some(SqlExtract {
            file_path: file_path.to_string(),
            strings,
        })
    }

    pub(crate) fn analyze_stmts(&self, suite: &ast::Suite) -> Vec<SqlString> {
        let mut results = Vec::new();
        for stmt in suite {
            let stmt_results = match stmt {
                ast::Stmt::Assign(a) => self.analyze_assignment(a),
                ast::Stmt::AnnAssign(a) => self.analyze_annotated_assignment(a),
                ast::Stmt::For(f) => self.analyze_body_and_orelse(&f.body, &f.orelse),
                ast::Stmt::AsyncFor(f) => self.analyze_body_and_orelse(&f.body, &f.orelse),
                ast::Stmt::While(f) => self.analyze_body_and_orelse(&f.body, &f.orelse),
                ast::Stmt::If(f) => self.analyze_body_and_orelse(&f.body, &f.orelse),

                ast::Stmt::FunctionDef(f) => self.analyze_stmts(&f.body),
                ast::Stmt::AsyncFunctionDef(f) => self.analyze_stmts(&f.body),
                ast::Stmt::ClassDef(f) => self.analyze_stmts(&f.body),
                ast::Stmt::With(f) => self.analyze_stmts(&f.body),
                ast::Stmt::AsyncWith(f) => self.analyze_stmts(&f.body),

                ast::Stmt::Try(t) => {
                    self.analyze_try(&t.body, &t.orelse, &t.finalbody, &t.handlers)
                }
                ast::Stmt::TryStar(t) => {
                    self.analyze_try(&t.body, &t.orelse, &t.finalbody, &t.handlers)
                }

                ast::Stmt::Match(f) => f
                    .cases
                    .iter()
                    .flat_map(|c| self.analyze_stmts(&c.body))
                    .collect(),

                ast::Stmt::Expr(e) => self.analyze_stmt_expr(e),

                ast::Stmt::Return(_)
                | ast::Stmt::Import(_)
                | ast::Stmt::ImportFrom(_)
                | ast::Stmt::Continue(_)
                | ast::Stmt::Assert(_)
                | ast::Stmt::Delete(_)
                | ast::Stmt::Raise(_) => vec![],
                _ => {
                    bail_with!(vec![], "Unimplemented stmt: {:?}", stmt)
                }
            };

            results.extend(stmt_results);
        }

        results
    }

    fn analyze_body_and_orelse(
        &self,
        body: &Vec<ast::Stmt>,
        orelse: &Vec<ast::Stmt>,
    ) -> Vec<SqlString> {
        self.analyze_stmts(body)
            .into_iter()
            .chain(self.analyze_stmts(orelse))
            .collect()
    }

    fn analyze_try(
        &self,
        body: &Vec<ast::Stmt>,
        orelse: &Vec<ast::Stmt>,
        finalbody: &Vec<ast::Stmt>,
        handlers: &[ast::ExceptHandler],
    ) -> Vec<SqlString> {
        self.analyze_stmts(body)
            .into_iter()
            .chain(
                handlers
                    .iter()
                    .filter_map(|h| h.as_except_handler())
                    .flat_map(|eh| self.analyze_stmts(&eh.body)),
            )
            .chain(self.analyze_stmts(orelse))
            .chain(self.analyze_stmts(finalbody))
            .collect()
    }

    /// Check if variable name suggests it contains SQL
    fn is_sql_variable_name(&self, name: &str) -> bool {
        let name_lower = name.to_lowercase();
        self.config.variable_ctx.contains(&name_lower)
    }

    fn is_sql_function_name(&self, name: &str) -> bool {
        let name_lower = name.to_lowercase();
        self.config.func_ctx.contains(&name_lower)
    }

    fn is_sql_parameter_name(&self, name: &str) -> bool {
        let sql_params = ["sql", "query", "statement", "command"]; // TODO: Configurable
        sql_params.contains(&name)
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
