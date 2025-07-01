#![allow(dead_code, unused_variables, clippy::multiple_crate_versions)]
mod assign;
mod finder_types;
mod format;
mod range;
mod tests;
pub use crate::finder_types::{FinderConfig, SqlExtract, SqlString};
use logging::{bail_with, debug, error, info, warn};
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
    pub fn new(config: Arc<FinderConfig>) -> Self {
        debug!("Creating new SqlFinder instance");
        Self { config }
    }

    #[must_use]
    pub fn analyze_file(&mut self, file_path: &str) -> Option<SqlExtract> {
        debug!("Starting analysis of file: {file_path}");

        let source_code = fs::read_to_string(file_path)
            .inspect_err(|e| error!("Failed to read file '{file_path}': {e}"))
            .ok()?;

        debug!(
            "Successfully read {} bytes from {}",
            source_code.len(),
            file_path
        );

        let parsed = ast::Suite::parse(&source_code, file_path)
            .inspect_err(|e| {
                error!("Failed to parse Python file '{file_path}': {e}");
                debug!("Parse error details for {file_path}: {e:?}");
            })
            .ok()?;

        // Create RangeFile and pass it to analyze_stmts
        let range_file = range::RangeFile::from_src(&source_code);

        debug!(
            "Successfully parsed AST for {file_path}, found {} top-level statements",
            parsed.len()
        );

        let strings = self.analyze_stmts(&parsed, &range_file);

        if strings.is_empty() {
            debug!("No SQL strings found in {file_path}");
        } else {
            info!("Found {} SQL string(s) in {}", strings.len(), file_path);
            debug!(
                "SQL strings found in {}: {:?}",
                file_path,
                strings
                    .iter()
                    .map(|s| &s.sql_content[..s.sql_content.len().min(50)])
                    .collect::<Vec<_>>()
            );
        }

        Some(SqlExtract {
            file_path: file_path.to_string(),
            strings,
        })
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) fn analyze_stmts(
        &self,
        suite: &ast::Suite,
        range_file: &range::RangeFile,
    ) -> Vec<SqlString> {
        debug!("Analyzing {} statements", suite.len());
        let mut results = Vec::new();

        for (i, stmt) in suite.iter().enumerate() {
            debug!(
                "Processing statement {}/{}: {:?}",
                i + 1,
                suite.len(),
                std::mem::discriminant(stmt)
            );

            let stmt_results = match stmt {
                ast::Stmt::Assign(a) => {
                    debug!("Processing assignment statement");
                    self.analyze_assignment(a, range_file)
                }
                ast::Stmt::AnnAssign(a) => {
                    debug!("Processing annotated assignment statement");
                    self.analyze_annotated_assignment(a, range_file)
                }
                ast::Stmt::For(f) => {
                    debug!(
                        "Processing for loop with {} body statements, {} orelse statements",
                        f.body.len(),
                        f.orelse.len()
                    );
                    self.analyze_body_and_orelse(&f.body, &f.orelse, range_file)
                }
                ast::Stmt::AsyncFor(f) => {
                    debug!(
                        "Processing async for loop with {} body statements, {} orelse statements",
                        f.body.len(),
                        f.orelse.len()
                    );
                    self.analyze_body_and_orelse(&f.body, &f.orelse, range_file)
                }
                ast::Stmt::While(f) => {
                    debug!(
                        "Processing while loop with {} body statements, {} orelse statements",
                        f.body.len(),
                        f.orelse.len()
                    );
                    self.analyze_body_and_orelse(&f.body, &f.orelse, range_file)
                }
                ast::Stmt::If(f) => {
                    debug!(
                        "Processing if statement with {} body statements, {} orelse statements",
                        f.body.len(),
                        f.orelse.len()
                    );
                    self.analyze_body_and_orelse(&f.body, &f.orelse, range_file)
                }

                ast::Stmt::FunctionDef(f) => {
                    debug!(
                        "Processing function definition '{}' with {} statements",
                        f.name,
                        f.body.len()
                    );
                    self.analyze_stmts(&f.body, range_file)
                }
                ast::Stmt::AsyncFunctionDef(f) => {
                    debug!(
                        "Processing async function definition '{}' with {} statements",
                        f.name,
                        f.body.len()
                    );
                    self.analyze_stmts(&f.body, range_file)
                }
                ast::Stmt::ClassDef(f) => {
                    debug!(
                        "Processing class definition '{}' with {} statements",
                        f.name,
                        f.body.len()
                    );
                    self.analyze_stmts(&f.body, range_file)
                }
                ast::Stmt::With(f) => {
                    debug!(
                        "Processing with statement with {} body statements",
                        f.body.len()
                    );
                    self.analyze_stmts(&f.body, range_file)
                }
                ast::Stmt::AsyncWith(f) => {
                    debug!(
                        "Processing async with statement with {} body statements",
                        f.body.len()
                    );
                    self.analyze_stmts(&f.body, range_file)
                }

                ast::Stmt::Try(t) => {
                    debug!(
                        "Processing try statement with {} body, {} handlers, {} orelse, {} finally statements",
                        t.body.len(),
                        t.handlers.len(),
                        t.orelse.len(),
                        t.finalbody.len()
                    );
                    self.analyze_try(&t.body, &t.orelse, &t.finalbody, &t.handlers, range_file)
                }
                ast::Stmt::TryStar(t) => {
                    debug!(
                        "Processing try* statement with {} body, {} handlers, {} orelse, {} finally statements",
                        t.body.len(),
                        t.handlers.len(),
                        t.orelse.len(),
                        t.finalbody.len()
                    );
                    self.analyze_try(&t.body, &t.orelse, &t.finalbody, &t.handlers, range_file)
                }

                ast::Stmt::Match(f) => {
                    debug!("Processing match statement with {} cases", f.cases.len());
                    f.cases
                        .iter()
                        .enumerate()
                        .flat_map(|(i, c)| {
                            debug!(
                                "Processing match case {}/{} with {} statements",
                                i + 1,
                                f.cases.len(),
                                c.body.len()
                            );
                            self.analyze_stmts(&c.body, range_file)
                        })
                        .collect()
                }

                ast::Stmt::Expr(e) => {
                    debug!("Processing expression statement");
                    self.analyze_stmt_expr(e, range_file)
                }

                ast::Stmt::Return(_) => {
                    debug!("Skipping return statement");
                    vec![]
                }
                ast::Stmt::Import(_) => {
                    debug!("Skipping import statement");
                    vec![]
                }
                ast::Stmt::ImportFrom(_) => {
                    debug!("Skipping import from statement");
                    vec![]
                }
                ast::Stmt::Continue(_) => {
                    debug!("Skipping continue statement");
                    vec![]
                }
                ast::Stmt::Assert(_) => {
                    debug!("Skipping assert statement");
                    vec![]
                }
                ast::Stmt::Delete(_) => {
                    debug!("Skipping delete statement");
                    vec![]
                }
                ast::Stmt::Raise(_) => {
                    debug!("Skipping raise statement");
                    vec![]
                }
                _ => {
                    bail_with!(vec![], "Unimplemented stmt: {:?}", stmt)
                }
            };

            debug!(
                "Statement {}/{} yielded {} SQL strings",
                i + 1,
                suite.len(),
                stmt_results.len()
            );
            results.extend(stmt_results);
        }

        debug!(
            "Completed analyzing {} statements, found {} total SQL strings",
            suite.len(),
            results.len()
        );
        results
    }

    fn analyze_body_and_orelse(
        &self,
        body: &Vec<ast::Stmt>,
        orelse: &Vec<ast::Stmt>,
        range_file: &range::RangeFile,
    ) -> Vec<SqlString> {
        debug!(
            "Analyzing body ({} stmts) and orelse ({} stmts)",
            body.len(),
            orelse.len()
        );

        let body_results = self.analyze_stmts(body, range_file);
        let orelse_results = self.analyze_stmts(orelse, range_file);

        debug!(
            "Body yielded {} SQL strings, orelse yielded {} SQL strings",
            body_results.len(),
            orelse_results.len()
        );

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
        debug!(
            "Analyzing try block: {} body, {} handlers, {} orelse, {} finally statements",
            body.len(),
            handlers.len(),
            orelse.len(),
            finalbody.len()
        );

        let body_results = self.analyze_stmts(body, range_file);
        debug!("Try body yielded {} SQL strings", body_results.len());

        let handler_results: Vec<SqlString> = handlers
            .iter()
            .enumerate()
            .filter_map(|(i, h)| {
                h.as_except_handler().map_or_else(
                    || {
                        warn!("Encountered non-ExceptHandler in handlers list at index {i}");
                        None
                    },
                    |eh| {
                        debug!(
                            "Processing exception handler {}/{} with {} statements",
                            i + 1,
                            handlers.len(),
                            eh.body.len()
                        );
                        Some(self.analyze_stmts(&eh.body, range_file))
                    },
                )
            })
            .flatten()
            .collect();
        debug!(
            "Exception handlers yielded {} SQL strings",
            handler_results.len()
        );

        let orelse_results = self.analyze_stmts(orelse, range_file);
        debug!("Try orelse yielded {} SQL strings", orelse_results.len());

        let finally_results = self.analyze_stmts(finalbody, range_file);
        debug!("Try finally yielded {} SQL strings", finally_results.len());

        body_results
            .into_iter()
            .chain(handler_results)
            .chain(orelse_results)
            .chain(finally_results)
            .collect()
    }
}
