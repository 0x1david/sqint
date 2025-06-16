use crate::{SqlFinder, SqlString};
use logging::{always_log, debug};
use rustpython_parser::{
    ast::{self, Identifier},
    text_size::TextRange,
};

impl SqlFinder {
    pub(super) fn analyze_assignment(
        &self,
        assign: &ast::StmtAssign,
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
            if let Some(sql_content) = Self::extract_string_content(value) {
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
        let mut value_idx = 0;

        for target in targets.iter() {
            if let ast::Expr::Starred(ast::ExprStarred {
                value: starred_target,
                ..
            }) = target
            {
                let starred_count = values.len() - targets.len() + 1;
                let consumed_values = &values[value_idx..value_idx + starred_count];
                let new_list_expr = ast::Expr::List(ast::ExprList {
                    range: TextRange::default(),
                    elts: consumed_values.to_vec(),
                    ctx: ast::ExprContext::Load,
                });
                self.process_assignment_target(
                    starred_target,
                    &new_list_expr,
                    byte_offset,
                    contexts,
                );
                value_idx += starred_count;
            } else {
                self.process_assignment_target(target, &values[value_idx], byte_offset, contexts);
                value_idx += 1;
            }
        }
    }

    /// Extract string content from an expression (only handles string literals)
    fn extract_string_content(expr: &ast::Expr) -> Option<String> {
        match expr {
            ast::Expr::Constant(c) => match &c.value {
                ast::Constant::Str(s) => Some(s.clone()),
                _ => {
                    always_log!("Not a constant string: {:?}", c);
                    None
                }
            },
            ast::Expr::Name(n) => Some(format!("{{{}}}", n.id)),
            ast::Expr::FormattedValue(f) => Self::extract_string_content(&f.value),
            ast::Expr::JoinedStr(j) => j.values.iter().try_fold(String::new(), |mut acc, val| {
                Self::extract_string_content(val).map(|s| {
                    acc.push_str(&s);
                    acc
                })
            }),

            _ => {
                always_log!("Not a string literal: {:?}", expr);
                None
            }
        }
    }
}
