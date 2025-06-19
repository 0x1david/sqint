#![allow(clippy::needless_collect)]
use crate::finder_types::FinderType;

use crate::format::format_python_string;
use crate::{SqlFinder, SqlString};
use logging::{bail, bail_with};
use regex::Regex;
use rustpython_parser::{
    ast::{self, Identifier},
    text_size::TextRange,
};

// Public API
impl SqlFinder {
    pub(super) fn analyze_assignment(&self, assign: &ast::StmtAssign) -> Vec<SqlString> {
        let mut sqls = vec![];
        assign.targets.iter().for_each(|target| {
            let results = self.process_assignment_target(
                target,
                &assign.value,
                assign.range.start().to_usize(),
            );
            sqls.extend(results);
        });
        sqls
    }

    pub(super) fn analyze_annotated_assignment(
        &self,
        assign: &ast::StmtAnnAssign,
    ) -> Vec<SqlString> {
        let mut sqls = vec![];
        if let Some(val) = &assign.value {
            let results = self.process_assignment_target(
                &assign.target,
                val,
                assign.range.start().to_usize(),
            );
            sqls.extend(results);
        }
        sqls
    }
}

// Assignment target processing
impl SqlFinder {
    fn process_assignment_target(
        &self,
        target: &ast::Expr,
        value: &ast::Expr,
        byte_offset: usize,
    ) -> Vec<SqlString> {
        match target {
            ast::Expr::Name(name) => self.process_by_ident(&name.id, value, byte_offset),
            ast::Expr::Attribute(att) => self.process_by_ident(&att.attr, value, byte_offset),
            ast::Expr::Tuple(tuple) => {
                self.handle_tuple_assignment(&tuple.elts, value, byte_offset)
            }
            ast::Expr::List(list) => self.handle_tuple_assignment(&list.elts, value, byte_offset),

            // Other patterns like attribute access (obj.attr = ...) or subscript (arr[0] = ...)
            _ => {
                bail_with!(vec![], "Unhandled assignment target pattern: {:?}", target)
            }
        }
    }

    fn process_by_ident(
        &self,
        name: &Identifier,
        value: &ast::Expr,
        byte_offset: usize,
    ) -> Vec<SqlString> {
        if self.is_sql_variable_name(name) {
            if let Some(sql_content) = Self::extract_string_content(value) {
                return vec![SqlString {
                    byte_offset,
                    variable_name: name.to_string(),
                    sql_content,
                }];
            }
        }
        vec![]
    }

    fn handle_tuple_assignment(
        &self,
        targets: &[ast::Expr],
        value: &ast::Expr,
        byte_offset: usize,
    ) -> Vec<SqlString> {
        let has_sql_target = targets
            .iter()
            .any(|target| self.target_contains_sql_variable(target));

        if !has_sql_target {
            return vec![];
        }
        match value {
            ast::Expr::Tuple(tuple_value) => {
                self.process_paired_assignments(targets, &tuple_value.elts, byte_offset)
            }
            ast::Expr::List(list_value) => {
                self.process_paired_assignments(targets, &list_value.elts, byte_offset)
            }
            _ => {
                bail_with!(vec![], "Unhandled tuple assignment value: {:?}", value)
            }
        }
    }

    fn process_paired_assignments(
        &self,
        targets: &[ast::Expr],
        values: &[ast::Expr],
        byte_offset: usize,
    ) -> Vec<SqlString> {
        let mut results = Vec::new();
        let mut value_idx = 0;

        for target in targets {
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
                let target_results =
                    self.process_assignment_target(starred_target, &new_list_expr, byte_offset);
                results.extend(target_results);
                value_idx += starred_count;
            } else {
                let target_results =
                    self.process_assignment_target(target, &values[value_idx], byte_offset);
                results.extend(target_results);
                value_idx += 1;
            }
        }

        results
    }
    fn target_contains_sql_variable(&self, target: &ast::Expr) -> bool {
        match target {
            ast::Expr::Name(name) => self.is_sql_variable_name(&name.id),
            ast::Expr::Attribute(att) => self.is_sql_variable_name(&att.attr),
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

// String extraction
impl SqlFinder {
    fn extract_string_content(expr: &ast::Expr) -> Option<String> {
        match expr {
            ast::Expr::Constant(c) => {
                if let ast::Constant::Str(s) = &c.value {
                    Some(s.clone())
                } else {
                    bail_with!(None, "constant string: {:?}", c)
                }
            }
            ast::Expr::Subscript(_) | ast::Expr::Name(_) => Some(format!("{{{}}}", "PLACEHOLDER")),
            ast::Expr::Call(c) => Self::extract_call(c),
            ast::Expr::FormattedValue(f) => Self::extract_string_content(&f.value),
            ast::Expr::BinOp(b) => Self::extract_from_bin_op(b),
            ast::Expr::JoinedStr(j) => j.values.iter().try_fold(String::new(), |mut acc, val| {
                Self::extract_string_content(val).map(|s| {
                    acc.push_str(&s);
                    acc
                })
            }),

            _ => bail_with!(None, "Not a string literal: {:?}", expr),
        }
    }

    fn extract_from_bin_op(v: &ast::ExprBinOp<TextRange>) -> Option<String> {
        dbg!(v);
        match &v.op {
            ast::Operator::Mod => {
                let expr_string = match &*v.left {
                    ast::Expr::Constant(c) => Self::extract_expr_const(c),
                    otherwise => {
                        bail!(None, "Expected format string on LHS, got: {:?}", otherwise);
                    }
                };

                let (args, kwargs) = match &*v.right {
                    ast::Expr::Tuple(t) => {
                        (t.elts.iter().map(Self::extract_expr).collect(), vec![])
                    }
                    ast::Expr::List(l) => (l.elts.iter().map(Self::extract_expr).collect(), vec![]),
                    ast::Expr::Dict(d) => {
                        let keys: Vec<String> = d
                            .keys
                            .iter()
                            .filter_map(|k| k.as_ref())
                            .map(Self::extract_expr)
                            .map(|k| k.to_string())
                            .collect();

                        let values: Vec<FinderType> =
                            d.values.iter().map(Self::extract_expr).collect();
                        let kwargs = keys.into_iter().zip(values).collect();

                        (vec![], kwargs)
                    }
                    ast::Expr::Constant(c) => (vec![Self::extract_expr_const(c)], vec![]),
                    otherwise => {
                        bail_with!((vec![], vec![]), "Unhandled rhs expr type: {:?}", otherwise)
                    }
                };

                if let FinderType::Str(fmt_string) = expr_string {
                    return format_python_string(&fmt_string, &args, &kwargs);
                }
                None
            }
            otherwise => Self::extract_arithmetic(&v.left, &v.right, *otherwise),
        }
    }

    fn extract_arithmetic(lhs: &ast::Expr, rhs: &ast::Expr, op: ast::Operator) -> Option<String> {
        let lhs = match lhs {
            ast::Expr::Constant(c) => Self::extract_expr_const(c),
            ast::Expr::Name(_) => FinderType::Placeholder,
            otherwise => {
                bail!(None, "Expected format string on LHS, got: {:?}", otherwise);
            }
        };

        let rhs = match rhs {
            ast::Expr::Constant(c) => Self::extract_expr_const(c),
            ast::Expr::Name(_) => FinderType::Placeholder,
            otherwise => {
                bail!(None, "Expected format string on LHS, got: {:?}", otherwise);
            }
        };
        dbg!(&lhs, &rhs);
        let result = match op {
            ast::Operator::Add => lhs + rhs,
            ast::Operator::Sub => lhs - rhs,
            ast::Operator::Mult => lhs * rhs,
            ast::Operator::Div => lhs / rhs,
            _ => bail!(None, "Unexpected operator in extraction: {:?}", op),
        };
        Some(result?.to_string())
    }

    fn extract_call(v: &ast::ExprCall<TextRange>) -> Option<String> {
        dbg!(v);
        match &*v.func {
            ast::Expr::Attribute(ast::ExprAttribute { attr, value, .. })
                if attr.as_str() == "format" =>
            {
                Self::extract_format_call(&v.args, &v.keywords, value)
            }
            _ => Some(format!("{{{}}}", "PLACEHOLDER")),
        }
    }

    fn extract_format_call(
        args: &[ast::Expr],
        kwargs: &[ast::Keyword],
        value: &ast::Expr,
    ) -> Option<String> {
        let mut pos_fills = vec![];
        let mut kw_fills = vec![];
        let mut has_unpacked_dict = false;

        for a in args {
            let parsed = match a {
                ast::Expr::Constant(c) => vec![Self::extract_expr_const(c)],
                ast::Expr::List(els) => els.elts.iter().map(Self::extract_expr).collect(),
                ast::Expr::Subscript(_) | ast::Expr::Name(_) | ast::Expr::Call(_) => {
                    vec![FinderType::Placeholder]
                }
                ast::Expr::BinOp(b) => vec![FinderType::Str(Self::extract_from_bin_op(b)?)],
                _ => bail_with!(
                    vec![FinderType::Unhandled],
                    "Unhandled value in args: {:?}",
                    a
                ),
            };
            for p in parsed {
                pos_fills.push(p.to_string());
            }
        }

        for kw in kwargs {
            if let Some(name) = &kw.arg {
                let val = Self::extract_expr(&kw.value);
                kw_fills.push((name.clone(), val));
            } else {
                has_unpacked_dict = true;
            }
        }

        let mut result = Self::extract_expr(value).to_string();

        if has_unpacked_dict {
            let re = Regex::new(r"\{[^}]+\}").unwrap();
            result = re.replace_all(&result, "{PLACEHOLDER}").to_string();
        } else {
            use regex::Regex;
            let numbered_re = Regex::new(r"\{(\d+)\}").unwrap();
            result = numbered_re
                .replace_all(&result, |caps: &regex::Captures| {
                    let index: usize = caps[1].parse().unwrap_or(0);
                    if index < pos_fills.len() {
                        pos_fills[index].clone()
                    } else {
                        "{PLACEHOLDER}".to_string()
                    }
                })
                .to_string();

            for f in &pos_fills {
                result = result.replacen("{}", f, 1);
            }

            for (kw_name, val) in &kw_fills {
                let pat = format!("{{{kw_name}}}");
                result = result.replace(&pat, &val.to_string());
            }
        }

        Some(result)
    }

    fn extract_expr(expr: &ast::Expr<TextRange>) -> FinderType {
        if let ast::Expr::Constant(v) = expr {
            Self::extract_expr_const(v)
        } else {
            bail_with!(
                FinderType::Unhandled,
                "Unhandled Expression for Extraction: {:?}",
                expr
            )
        }
    }

    fn extract_const(c: &ast::Constant) -> FinderType {
        match c {
            ast::Constant::Str(s) => FinderType::Str(s.clone()),
            ast::Constant::Int(i) => FinderType::Int(i.to_string()),
            ast::Constant::Bool(b) => FinderType::Bool(*b),
            ast::Constant::Float(f) => FinderType::Float(*f),
            ast::Constant::Tuple(t) => {
                FinderType::Tuple(t.iter().map(Self::extract_const).collect())
            }
            _ => bail_with!(FinderType::Unhandled, "Unhandled Constant: {:?}", c),
        }
    }

    fn extract_expr_const(c: &ast::ExprConstant<TextRange>) -> FinderType {
        Self::extract_const(&c.value)
    }
}
