#![allow(dead_code, unused_variables, clippy::multiple_crate_versions)]
mod assign;
mod finder_types;
mod format;
mod range;
mod tests;
pub use crate::finder_types::{FinderConfig, SqlExtract, SqlString};
use logging::{bail_with, error};
use rustpython_parser::{
    Parse,
    ast::{self},
};
use std::{fs, sync::Arc};

pub struct SqlFinder {
    config: Arc<FinderConfig>,
}

impl SqlFinder {
    #[must_use]
    pub const fn new(config: Arc<FinderConfig>) -> Self {
        Self { config }
    }

    #[must_use]
    pub fn analyze_file(&mut self, file_path: &str) -> Option<SqlExtract> {
        let source_code = fs::read_to_string(file_path)
            .inspect_err(|e| error!("Failed to read file '{file_path}': {e}"))
            .ok()?;

        let parsed = ast::Suite::parse(&source_code, file_path)
            .inspect_err(|e| {
                error!("Failed to parse Python file '{file_path}': {e}");
            })
            .ok()?;

        // Create RangeFile and pass it to analyze_stmts
        let range_file = range::RangeFile::from_src(&source_code);
        let strings = self.analyze_stmts(&parsed, &range_file);

        Some(SqlExtract {
            file_path: file_path.to_string(),
            strings,
        })
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) fn analyze_stmts(
        &self,
        suite: &ast::Suite,
        rf: &range::RangeFile,
    ) -> Vec<SqlString> {
        let mut results = Vec::new();
        for stmt in suite {
            let stmt_results = match stmt {
                ast::Stmt::Assign(a) => self.analyze_assignment(a, rf),
                ast::Stmt::AnnAssign(a) => self.analyze_annotated_assignment(a, rf),
                ast::Stmt::For(f) => self.analyze_body_and_orelse(&f.body, &f.orelse, rf),
                ast::Stmt::AsyncFor(f) => self.analyze_body_and_orelse(&f.body, &f.orelse, rf),
                ast::Stmt::While(f) => self.analyze_body_and_orelse(&f.body, &f.orelse, rf),
                ast::Stmt::If(f) => self.analyze_body_and_orelse(&f.body, &f.orelse, rf),
                ast::Stmt::FunctionDef(f) => self.analyze_stmts(&f.body, rf),
                ast::Stmt::AsyncFunctionDef(f) => self.analyze_stmts(&f.body, rf),
                ast::Stmt::ClassDef(f) => self.analyze_stmts(&f.body, rf),
                ast::Stmt::With(f) => self.analyze_stmts(&f.body, rf),
                ast::Stmt::AsyncWith(f) => self.analyze_stmts(&f.body, rf),

                ast::Stmt::Try(t) => {
                    self.analyze_try(&t.body, &t.orelse, &t.finalbody, &t.handlers, rf)
                }
                ast::Stmt::TryStar(t) => {
                    self.analyze_try(&t.body, &t.orelse, &t.finalbody, &t.handlers, rf)
                }
                ast::Stmt::Match(f) => f
                    .cases
                    .iter()
                    .flat_map(|c| self.analyze_stmts(&c.body, rf))
                    .collect(),

                ast::Stmt::Expr(e) => self.analyze_stmt_expr(e, rf),
                ast::Stmt::Return(_)
                | ast::Stmt::Import(_)
                | ast::Stmt::ImportFrom(_)
                | ast::Stmt::Continue(_)
                | ast::Stmt::Assert(_)
                | ast::Stmt::Delete(_)
                | ast::Stmt::Raise(_) => {
                    vec![]
                }
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
        range_file: &range::RangeFile,
    ) -> Vec<SqlString> {
        let body_results = self.analyze_stmts(body, range_file);
        let orelse_results = self.analyze_stmts(orelse, range_file);
        body_results.into_iter().chain(orelse_results).collect()
    }

    fn analyze_try(
        &self,
        body: &Vec<ast::Stmt>,
        orelse: &Vec<ast::Stmt>,
        finalbody: &Vec<ast::Stmt>,
        handlers: &[ast::ExceptHandler],
        range_file: &range::RangeFile,
    ) -> Vec<SqlString> {
        let body_results = self.analyze_stmts(body, range_file);

        let handler_results: Vec<SqlString> = handlers
            .iter()
            .filter_map(|h| {
                h.as_except_handler()
                    .map_or_else(|| None, |eh| Some(self.analyze_stmts(&eh.body, range_file)))
            })
            .flatten()
            .collect();

        let orelse_results = self.analyze_stmts(orelse, range_file);
        let finally_results = self.analyze_stmts(finalbody, range_file);

        body_results
            .into_iter()
            .chain(handler_results)
            .chain(orelse_results)
            .chain(finally_results)
            .collect()
    }
}
