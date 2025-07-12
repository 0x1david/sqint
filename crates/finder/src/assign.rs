#![allow(clippy::needless_collect, clippy::single_match_else)]
use crate::finder_types::{FinderType, SqlResult};
use crate::format::format_python_string;
use crate::preanalysis::PreanalyzedFile;
use crate::{SqlFinder, SqlString};
use logging::{bail, bail_with};
use regex::Regex;
use rustpython_parser::ast::{Operator, Ranged};
use rustpython_parser::{
    ast::{self, Identifier},
    text_size::TextRange,
};

// Public API
impl SqlFinder {
    pub(super) fn analyze_assignment(
        &self,
        assign: &ast::StmtAssign,
        range_file: &PreanalyzedFile,
    ) -> Vec<SqlString> {
        let mut sql_strings = vec![];

        assign.targets.iter().for_each(|target| {
            sql_strings = self
                .process_assignment_target(target, &assign.value)
                .into_iter()
                .map(|result| sql_result_to_string(result, range_file))
                .collect();
        });
        sql_strings
    }

    pub(super) fn analyze_stmt_expr(
        &self,
        e: &ast::StmtExpr,
        range_file: &PreanalyzedFile,
    ) -> Vec<SqlString> {
        self.process_expr_stmt(&e.value)
            .into_iter()
            .map(|result| sql_result_to_string(result, range_file))
            .collect()
    }

    pub(super) fn analyze_annotated_assignment(
        &self,
        assign: &ast::StmtAnnAssign,
        range_file: &PreanalyzedFile,
    ) -> Vec<SqlString> {
        assign.value.as_ref().map_or_else(Vec::new, |val| {
            self.process_assignment_target(&assign.target, val)
                .into_iter()
                .map(|result| sql_result_to_string(result, range_file))
                .collect()
        })
    }
}

// Internal processing
impl SqlFinder {
    fn process_expr_stmt(&self, value: &ast::Expr) -> Vec<SqlResult> {
        match value {
            ast::Expr::Call(call) => self.process_call_expr(call),
            ast::Expr::Attribute(_) => match value {
                ast::Expr::Call(call) => self.process_call_expr(call),
                _ => bail_with!(vec![], "Unhandled expr_stmt value pattern: {value:?}"),
            },
            ast::Expr::Constant(_) => vec![],
            _ => {
                bail_with!(vec![], "Unhandled expr_stmt value pattern: {value:?}")
            }
        }
    }

    fn process_call_expr(&self, call: &ast::ExprCall) -> Vec<SqlResult> {
        let function_name = Self::extract_function_name(&call.func);

        if !self.config.is_sql_function_name(&function_name) {
            return vec![];
        }

        let process_expr = |expr: &ast::Expr| -> Option<SqlResult> {
            self.extract_content(expr).and_then(|content| {
                content
                    .get_str()
                    .is_some_and(|s| self.config.is_sql_str(s))
                    .then_some(SqlResult {
                        byte_range: call.range.into(),
                        variable_name: function_name.clone(),
                        content,
                    })
            })
        };
        let kwargs = call
            .keywords
            .iter()
            .filter_map(|kw| process_expr(&kw.value));

        call.args
            .iter()
            .filter_map(process_expr)
            .chain(kwargs)
            .collect()
    }

    fn extract_content_flattened(&self, expr: &ast::Expr, variable_name: &str) -> Vec<SqlResult> {
        match expr {
            ast::Expr::List(ast::ExprList { elts, .. }) => elts
                .iter()
                .flat_map(|elem| self.extract_content_flattened(elem, variable_name))
                .collect(),
            ast::Expr::Tuple(ast::ExprTuple { elts, .. }) => elts
                .iter()
                .flat_map(|elem| self.extract_content_flattened(elem, variable_name))
                .collect(),

            ast::Expr::Dict(ast::ExprDict { values, .. }) => values
                .iter()
                .flat_map(|elem| self.extract_content_flattened(elem, variable_name))
                .collect(),
            ast::Expr::BoolOp(ast::ExprBoolOp { values, .. }) => values
                .iter()
                .flat_map(|elem| self.extract_content_flattened(elem, variable_name))
                .collect(),

            ast::Expr::BinOp(bin @ ast::ExprBinOp { op, .. })
                if *op == Operator::Add
                    || *op == Operator::Sub
                    || *op == Operator::Mult
                    || *op == Operator::Div =>
            {
                self.extract_from_bin_op(bin)
                    .map_or_else(Vec::new, |content| {
                        vec![SqlResult {
                            byte_range: expr.range().into(),
                            variable_name: variable_name.to_string(),
                            content,
                        }]
                    })
            }

            _ => self.extract_content(expr).map_or_else(Vec::new, |content| {
                vec![SqlResult {
                    byte_range: expr.range().into(),
                    variable_name: variable_name.to_string(),
                    content,
                }]
            }),
        }
    }

    fn process_assignment_target(&self, target: &ast::Expr, value: &ast::Expr) -> Vec<SqlResult> {
        match target {
            ast::Expr::Name(name) => self.process_by_ident(&name.id, value),
            ast::Expr::Attribute(att) => self.process_by_ident(&att.attr, value),
            ast::Expr::Tuple(tuple) => self.handle_tuple_assignment(&tuple.elts, value),
            ast::Expr::List(list) => self.handle_tuple_assignment(&list.elts, value),
            ast::Expr::Subscript(_) => vec![],
            _ => bail_with!(vec![], "Unhandled assignment target pattern: {:?}", target),
        }
    }

    fn process_by_ident(&self, name: &Identifier, value: &ast::Expr) -> Vec<SqlResult> {
        if self.config.is_sql_variable_name(name) {
            return self.extract_content_flattened(value, name);
        }
        vec![]
    }

    fn handle_tuple_assignment(&self, targets: &[ast::Expr], value: &ast::Expr) -> Vec<SqlResult> {
        let has_sql_target = targets
            .iter()
            .any(|target| self.target_contains_sql_variable(target));

        if !has_sql_target {
            return vec![];
        }

        match value {
            ast::Expr::Tuple(tuple_value) => {
                self.process_paired_assignments(targets, &tuple_value.elts)
            }
            ast::Expr::List(list_value) => {
                self.process_paired_assignments(targets, &list_value.elts)
            }
            _ => bail_with!(vec![], "Unhandled tuple assignment value: {:?}", value),
        }
    }
    // Process assignments with multiple values, including starred destructuring
    fn process_paired_assignments(
        &self,
        targets: &[ast::Expr],
        values: &[ast::Expr],
    ) -> Vec<SqlResult> {
        let mut results = Vec::new();
        let mut value_idx = 0;

        for target in targets {
            match target {
                ast::Expr::Starred(ast::ExprStarred {
                    value: starred_target,
                    ..
                }) => {
                    let starred_count = values.len() - targets.len() + 1;
                    let consumed_values = &values[value_idx..value_idx + starred_count];

                    let new_list_expr = ast::Expr::List(ast::ExprList {
                        range: TextRange::default(),
                        elts: consumed_values.to_vec(),
                        ctx: ast::ExprContext::Load,
                    });

                    let target_results =
                        self.process_assignment_target(starred_target, &new_list_expr);
                    results.extend(target_results);
                    value_idx += starred_count;
                }
                _ => {
                    let target_results = self.process_assignment_target(target, &values[value_idx]);
                    results.extend(target_results);
                    value_idx += 1;
                }
            }
        }

        results
    }

    fn target_contains_sql_variable(&self, target: &ast::Expr) -> bool {
        match target {
            ast::Expr::Name(name) => self.config.is_sql_variable_name(&name.id),
            ast::Expr::Attribute(att) => self.config.is_sql_variable_name(&att.attr),
            ast::Expr::Tuple(tuple) => tuple
                .elts
                .iter()
                .any(|t| self.target_contains_sql_variable(t)),
            ast::Expr::List(list) => list
                .elts
                .iter()
                .any(|t| self.target_contains_sql_variable(t)),
            ast::Expr::Starred(starred) => self.target_contains_sql_variable(&starred.value),
            _ => false,
        }
    }
}

// Content extraction
impl SqlFinder {
    fn extract_function_name(func_expr: &ast::Expr) -> String {
        match func_expr {
            ast::Expr::Name(name) => name.id.to_string(),
            ast::Expr::Attribute(attr) => {
                let full_name =
                    format!("{}.{}", Self::extract_function_name(&attr.value), attr.attr);
                full_name
            }
            _ => bail_with!(
                String::new(),
                "Unknown function expression: {:?}",
                &func_expr
            ),
        }
    }

    fn extract_content(&self, expr: &ast::Expr) -> Option<FinderType> {
        match expr {
            ast::Expr::Constant(c) => Some(Self::extract_expr_const(c)),
            ast::Expr::Call(c) => self.extract_call(c),
            ast::Expr::FormattedValue(f) => self.extract_content(&f.value),
            ast::Expr::BinOp(b) => self.extract_from_bin_op(b),

            ast::Expr::Subscript(_) | ast::Expr::Name(_) | ast::Expr::Attribute(_) => {
                Some(FinderType::Placeholder)
            }

            ast::Expr::JoinedStr(j) => {
                let parts: Option<Vec<FinderType>> = j
                    .values
                    .iter()
                    .map(|val| self.extract_content(val))
                    .collect();

                parts.map(|parts| {
                    let combined = parts.into_iter().map(|p| p.to_string()).collect::<String>();
                    FinderType::Str(combined)
                })
            }
            _ => bail_with!(None, "Not extractable content: {:?}", expr),
        }
    }

    fn extract_from_bin_op(&self, v: &ast::ExprBinOp<TextRange>) -> Option<FinderType> {
        match &v.op {
            ast::Operator::Mod => {
                let expr_content = self.extract_content(&v.left)?;

                let (args, kwargs) = match &*v.right {
                    ast::Expr::Constant(c) => (vec![Self::extract_expr_const(c)], vec![]),

                    ast::Expr::Tuple(ast::ExprTuple { elts, .. })
                    | ast::Expr::List(ast::ExprList { elts, .. }) => {
                        let args = elts
                            .iter()
                            .filter_map(|e| self.extract_content(e))
                            .collect();
                        (args, vec![])
                    }
                    ast::Expr::Dict(d) => {
                        let keys: Vec<String> = d
                            .keys
                            .iter()
                            .filter_map(|k| k.as_ref())
                            .filter_map(|e| self.extract_content(e))
                            .map(|k| k.to_string())
                            .collect();

                        let values: Vec<FinderType> = d
                            .values
                            .iter()
                            .filter_map(|e| self.extract_content(e))
                            .collect();

                        let kwargs: Vec<_> = keys.into_iter().zip(values).collect();
                        (vec![], kwargs)
                    }
                    _ => bail_with!((vec![], vec![]), "Unhandled rhs expr type: {:?}", v.right),
                };

                match expr_content {
                    FinderType::Str(fmt_string) => {
                        format_python_string(&fmt_string, &args, &kwargs).map(FinderType::Str)
                    }
                    other => Some(other),
                }
            }
            _ => self.extract_arithmetic(&v.left, &v.right, v.op),
        }
    }

    fn extract_arithmetic(
        &self,
        lhs: &ast::Expr,
        rhs: &ast::Expr,
        op: ast::Operator,
    ) -> Option<FinderType> {
        let lhs_content = self.extract_content(lhs)?;
        let rhs_content = self.extract_content(rhs)?;

        match op {
            ast::Operator::Add => lhs_content + rhs_content,
            ast::Operator::Sub => lhs_content - rhs_content,
            ast::Operator::Mult => lhs_content * rhs_content,
            ast::Operator::Div => lhs_content / rhs_content,
            _ => bail!(None, "Unexpected operator in extraction: {:?}", op),
        }
    }

    fn extract_call(&self, v: &ast::ExprCall<TextRange>) -> Option<FinderType> {
        match &*v.func {
            ast::Expr::Call(nested_call) => self.extract_call(nested_call),
            ast::Expr::Attribute(ast::ExprAttribute { attr, value, .. }) => match attr.as_str() {
                "format" => self.extract_format_call(&v.args, &v.keywords, value),
                _ => self.extract_content(value),
            },
            ast::Expr::Name(name) => {
                if self.config.is_sql_function_name(&name.id) {
                    v.args.iter().find_map(|arg| self.extract_content(arg))
                } else {
                    None
                }
            }
            _ => bail_with!(None, "Unhandled function call type: {:?}", v.func),
        }
    }

    fn extract_format_call(
        &self,
        args: &[ast::Expr],
        kwargs: &[ast::Keyword],
        value: &ast::Expr,
    ) -> Option<FinderType> {
        let mut pos_fills = vec![];
        let mut kw_fills = vec![];
        let mut has_unpacked_dict = false;

        for a in args {
            let parsed = match a {
                ast::Expr::Constant(c) => vec![Self::extract_expr_const(c)],
                ast::Expr::Subscript(_) | ast::Expr::Name(_) | ast::Expr::Call(_) => {
                    vec![FinderType::Placeholder]
                }

                ast::Expr::List(els) => els
                    .elts
                    .iter()
                    .filter_map(|e| self.extract_content(e))
                    .collect(),

                ast::Expr::BinOp(b) => self
                    .extract_from_bin_op(b)
                    .map_or_else(|| vec![FinderType::Unhandled], |content| vec![content]),

                _ => bail_with!(
                    vec![FinderType::Unhandled],
                    "Unhandled value in args: {a:?}"
                ),
            };
            pos_fills.extend(parsed.iter().map(std::string::ToString::to_string));
        }

        for kw in kwargs {
            match &kw.arg {
                Some(name) => {
                    if let Some(val) = self.extract_content(&kw.value) {
                        kw_fills.push((name.clone(), val));
                    }
                }
                None => has_unpacked_dict = true,
            }
        }

        let base_content = self.extract_content(value)?;

        let mut result = base_content.to_string();

        if has_unpacked_dict {
            let re = Regex::new(r"\{[^}]+\}")
                .expect("Broke the regex format call finder.")
                .replace_all(&result, "PLACEHOLDER")
                .to_string();
        } else {
            let numbered_re = Regex::new(r"\{(\d+)\}")
                .expect("Broke the regex format call finder.")
                .replace_all(&result, |caps: &regex::Captures| {
                    let index: usize = caps[1].parse().unwrap_or(0);

                    if index < pos_fills.len() {
                        pos_fills[index].clone()
                    } else {
                        "PLACEHOLDER".to_string()
                    }
                })
                .to_string();

            for f in pos_fills {
                result = result.replacen("{}", &f, 1);
            }

            for (kw_name, val) in &kw_fills {
                let pat = format!("{{{kw_name}}}");
                result = result.replace(&pat, &val.to_string());
            }
        }

        Some(FinderType::Str(result))
    }

    fn extract_expr_const(c: &ast::ExprConstant<TextRange>) -> FinderType {
        Self::extract_const(&c.value)
    }

    fn extract_const(c: &ast::Constant) -> FinderType {
        let result = match c {
            ast::Constant::Str(s) => FinderType::Str(s.clone()),
            ast::Constant::Int(i) => FinderType::Int(i.to_string()),
            ast::Constant::Bool(b) => FinderType::Bool(*b),
            ast::Constant::Float(f) => FinderType::Float(*f),
            ast::Constant::None => FinderType::Unhandled,
            ast::Constant::Tuple(t) => {
                FinderType::Tuple(t.iter().map(Self::extract_const).collect())
            }
            _ => bail_with!(FinderType::Unhandled, "Unhandled Constant: {:?}", c),
        };

        result
    }
}

fn sql_result_to_string(res: SqlResult, range_file: &PreanalyzedFile) -> SqlString {
    SqlString {
        variable_name: res.variable_name,
        range: range_file.byterange_to_range(res.byte_range),
        sql_content: res.content.to_string(),
    }
}
