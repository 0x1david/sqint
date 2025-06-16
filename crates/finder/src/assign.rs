use std::fmt::Display;

use crate::{SqlFinder, SqlString};
use logging::{always_log, debug, exception};
use regex::Regex;
use rustpython_parser::{
    ast::{self, Identifier, located::ExprBinOp},
    source_code::SourceRange,
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
            ast::Expr::BinOp(b) => Self::extract_from_bin_op(b),
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
    fn extract_from_bin_op(v: &ast::ExprBinOp<TextRange>) -> Option<String> {
        match &v.op {
            ast::Operator::Mod => {
                let expr_string = match &*v.left {
                    ast::Expr::Constant(c) => extract_expr_const(c),
                    otherwise => {
                        exception!("Expected format string on LHS, got: {:?}", otherwise);
                        return None;
                    }
                };

                let rhs = match &*v.right {
                    ast::Expr::Tuple(t) => t.elts.iter().map(extract_expr).collect(),
                    ast::Expr::List(l) => l.elts.iter().map(extract_expr).collect(),
                    ast::Expr::Constant(c) => vec![extract_expr_const(c)],
                    otherwise => {
                        always_log!("Unhandled rhs expr type: {:?}", otherwise);
                        vec![]
                    }
                };
                if let ConstType::Str(fmt_string) = expr_string {
                    return format_python_string(&fmt_string, &rhs);
                }
                None
            }
            otherwise => {
                always_log!("Unhandled binary operator: {:?}", otherwise);
                None
            }
        }
    }
}
fn extract_expr(expr: &ast::Expr<TextRange>) -> ConstType {
    match expr {
        ast::Expr::Constant(v) => extract_expr_const(v),
        _ => {
            always_log!("Unhandled Constant");
            ConstType::Unhandled
        }
    }
}

fn extract_const(c: &ast::Constant) -> ConstType {
    match c {
        ast::Constant::Str(s) => ConstType::Str(s.clone()),
        ast::Constant::Int(i) => ConstType::Int(i.to_string()),
        ast::Constant::Bool(b) => ConstType::Bool(*b),
        ast::Constant::Float(f) => ConstType::Float(*f),
        ast::Constant::Tuple(t) => ConstType::Tuple(t.iter().map(extract_const).collect()),
        _ => ConstType::Unhandled,
    }
}

fn extract_expr_const(c: &ast::ExprConstant<TextRange>) -> ConstType {
    extract_const(&c.value)
}

fn format_python_string(format_str: &str, values: &[ConstType]) -> Option<String> {
    // Regex to match Python format specifiers like %s, %d, %f, etc.
    let re = Regex::new(r"%[sdifgGeEoxXc%]").ok()?;

    let mut result = format_str.to_string();
    let mut value_index = 0;

    // Find all format specifiers
    let matches: Vec<_> = re.find_iter(format_str).collect();

    // Replace from right to left to avoid offset issues
    for m in matches.iter().rev() {
        let specifier = m.as_str();

        // Handle %% (literal %)
        if specifier == "%%" {
            result.replace_range(m.range(), "%");
            continue;
        }

        // Check if we have enough values
        if value_index >= values.len() {
            return None; // Not enough values for format string
        }

        let value = &values[values.len() - 1 - value_index];
        value_index += 1;

        let replacement = match specifier {
            "%s" => Some(value.to_string()),
            "%d" | "%i" => format_value_as_int(value),
            "%f" => format_value_as_float(value),
            "%g" | "%G" => format_value_as_float(value), // General format
            "%e" | "%E" => format_value_as_scientific(value),
            "%o" => format_value_as_octal(value),
            "%x" => format_value_as_hex(value, false),
            "%X" => format_value_as_hex(value, true),
            "%c" => format_value_as_char(value),
            _ => return None, // Unsupported format specifier
        };

        if let Some(replacement_str) = replacement {
            result.replace_range(m.range(), &replacement_str);
        } else {
            return None; // Conversion failed
        }
    }

    Some(result)
}

fn format_value_as_int(value: &ConstType) -> Option<String> {
    match value {
        ConstType::Int(i) => Some(i.clone()),
        ConstType::Float(f) => Some((*f as i64).to_string()),
        ConstType::Bool(b) => Some(if *b { "1".to_string() } else { "0".to_string() }),
        ConstType::Str(s) => s.parse::<i64>().ok().map(|i| i.to_string()),
        _ => None,
    }
}

fn format_value_as_float(value: &ConstType) -> Option<String> {
    match value {
        ConstType::Float(f) => Some(f.to_string()),
        ConstType::Int(i) => i.parse::<f64>().ok().map(|f| f.to_string()),
        ConstType::Bool(b) => Some(if *b {
            "1.0".to_string()
        } else {
            "0.0".to_string()
        }),
        ConstType::Str(s) => s.parse::<f64>().ok().map(|f| f.to_string()),
        _ => None,
    }
}

fn format_value_as_scientific(value: &ConstType) -> Option<String> {
    match value {
        ConstType::Float(f) => Some(format!("{:e}", f)),
        ConstType::Int(i) => i.parse::<f64>().ok().map(|f| format!("{:e}", f)),
        ConstType::Bool(b) => Some(if *b {
            "1.000000e+00".to_string()
        } else {
            "0.000000e+00".to_string()
        }),
        ConstType::Str(s) => s.parse::<f64>().ok().map(|f| format!("{:e}", f)),
        _ => None,
    }
}

fn format_value_as_octal(value: &ConstType) -> Option<String> {
    match value {
        ConstType::Int(i) => i.parse::<i64>().ok().map(|i| format!("{:o}", i)),
        ConstType::Bool(b) => Some(if *b { "1".to_string() } else { "0".to_string() }),
        _ => None,
    }
}

fn format_value_as_hex(value: &ConstType, uppercase: bool) -> Option<String> {
    match value {
        ConstType::Int(i) => i.parse::<i64>().ok().map(|i| {
            if uppercase {
                format!("{:X}", i)
            } else {
                format!("{:x}", i)
            }
        }),
        ConstType::Bool(b) => Some(if *b { "1".to_string() } else { "0".to_string() }),
        _ => None,
    }
}
fn format_value_as_char(value: &ConstType) -> Option<String> {
    match value {
        ConstType::Int(i) => {
            if let Ok(code) = i.parse::<u32>() {
                if let Some(ch) = char::from_u32(code) {
                    return Some(ch.to_string());
                }
            }
            None
        }
        ConstType::Str(s) => {
            if s.len() == 1 {
                Some(s.clone())
            } else {
                None
            }
        }
        _ => None,
    }
}

// Usage example:
// let format_str = "select * from %s where id = %d";
// let values = vec![ConstType::Str("users".to_string()), ConstType::Int("123".to_string())];
// let result = format_python_string(format_str, &values);
// // Result: Some("select * from users where id = 123")

enum ConstType {
    Str(String),
    Int(String),
    Float(f64),
    Bool(bool),
    Tuple(Vec<ConstType>),
    Unhandled,
}
impl std::fmt::Display for ConstType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConstType::Str(s) => write!(f, "{}", s),
            ConstType::Int(i) => write!(f, "{}", i),
            ConstType::Float(fl) => write!(f, "{}", fl),
            // Using numeric booleans for maximum db compatibility
            ConstType::Bool(b) => write!(f, "{}", if *b { "1" } else { "0" }),
            ConstType::Tuple(t) => {
                write!(f, "(")?;
                for (i, item) in t.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", item)?;
                }
                write!(f, ")")
            }
            ConstType::Unhandled => write!(f, "<unhandled>"),
        }
    }
}
