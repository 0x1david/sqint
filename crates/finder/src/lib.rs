mod finder_types;
mod format;
pub mod preanalysis;
mod tests;
mod traversal;
pub use crate::finder_types::{FinderConfig, SqlExtract, SqlString};
use logging::{bail_with, error};
use rustpython_parser::{
    Parse,
    ast::{self, Ranged},
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
    pub fn analyze_file(&mut self, file_path: &str, is_raw_sql: bool) -> Option<SqlExtract> {
        if is_raw_sql {
            self.analyze_sql_file(file_path)
        } else {
            self.analyze_python_file(file_path)
        }
    }

    fn analyze_python_file(&mut self, file_path: &str) -> Option<SqlExtract> {
        let source_code = fs::read_to_string(file_path)
            .inspect_err(|e| error!("Failed to read file '{file_path}': {e}"))
            .ok()?;

        let parsed = ast::Suite::parse(&source_code, file_path)
            .inspect_err(|e| {
                error!("Failed to parse Python file '{file_path}': {e}");
            })
            .ok()?;

        let range_file = preanalysis::PreanalyzedFile::from_src(&source_code);
        let strings = self.analyze_stmts(&parsed, &range_file);

        Some(SqlExtract::new(file_path.to_string(), strings))
    }
    fn analyze_sql_file(&mut self, file_path: &str) -> Option<SqlExtract> {
        let source_code = fs::read_to_string(file_path)
            .inspect_err(|e| error!("Failed to read file '{file_path}': {e}"))
            .ok()?;

        let mut strings = Vec::new();
        let mut current_pos = 0;
        let mut line_num = 1;
        let mut col_num = 1;

        for (index, sql_segment) in source_code.split(';').enumerate() {
            let trimmed = sql_segment.trim();
            if !trimmed.is_empty() {
                let start_line = line_num;
                let start_col = col_num;

                let range = crate::preanalysis::Range {
                    start: crate::preanalysis::LineCol::new(start_line, start_col, 0),
                };

                strings.push(SqlString::new(
                    format!("sql_statement_{}", index + 1),
                    trimmed.to_string(),
                    range,
                ));
            }

            // Update position for next segment
            for ch in sql_segment.chars() {
                if ch == '\n' {
                    line_num += 1;
                    col_num = 1;
                } else {
                    col_num += 1;
                }
            }

            // Account for the semicolon delimiter (except for the last segment)
            if current_pos + sql_segment.len() < source_code.len() {
                col_num += 1;
            }

            current_pos += sql_segment.len() + 1;
        }

        Some(SqlExtract::new(file_path.to_string(), strings))
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) fn analyze_stmts(
        &self,
        suite: &ast::Suite,
        rf: &preanalysis::PreanalyzedFile,
    ) -> Vec<SqlString> {
        let mut results = Vec::new();
        for stmt in suite {
            let start_offset = stmt.range().start().to_usize();
            let end_offset = stmt.range().end().to_usize();
            if rf.should_ignore_stmt_at(start_offset) || rf.should_ignore_stmt_at(end_offset) {
                continue;
            }

            let stmt_results = match stmt {
                ast::Stmt::Assign(a) => self.analyze_assignment(a, rf),
                ast::Stmt::AnnAssign(a) => self.analyze_annotated_assignment(a, rf),

                ast::Stmt::For(ast::StmtFor { body, orelse, .. })
                | ast::Stmt::AsyncFor(ast::StmtAsyncFor { body, orelse, .. })
                | ast::Stmt::While(ast::StmtWhile { body, orelse, .. })
                | ast::Stmt::If(ast::StmtIf { body, orelse, .. }) => {
                    self.analyze_body_and_orelse(body, orelse, rf)
                }

                ast::Stmt::FunctionDef(ast::StmtFunctionDef { body, .. })
                | ast::Stmt::AsyncFunctionDef(ast::StmtAsyncFunctionDef { body, .. })
                | ast::Stmt::ClassDef(ast::StmtClassDef { body, .. })
                | ast::Stmt::With(ast::StmtWith { body, .. })
                | ast::Stmt::AsyncWith(ast::StmtAsyncWith { body, .. }) => {
                    self.analyze_stmts(body, rf)
                }

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
        range_file: &preanalysis::PreanalyzedFile,
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
        range_file: &preanalysis::PreanalyzedFile,
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
