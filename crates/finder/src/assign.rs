use crate::formatters;
use crate::{SqlFinder, SqlString};
use logging::{except_none, except_ret, exception};
use regex::Regex;
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

    pub(super) fn analyze_annotated_assignment(
        &self,
        assign: &ast::StmtAnnAssign,
        contexts: &mut Vec<SqlString>,
    ) {
        if let Some(val) = &assign.value {
            self.process_assignment_target(
                &assign.target,
                val,
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
                self.process_by_ident(&att.attr, value, byte_offset, contexts);
            }
            ast::Expr::Tuple(tuple) => {
                self.handle_tuple_assignment(&tuple.elts, value, byte_offset, contexts);
            }
            ast::Expr::List(list) => {
                self.handle_tuple_assignment(&list.elts, value, byte_offset, contexts);
            }

            // Other patterns like attribute access (obj.attr = ...) or subscript (arr[0] = ...)
            _ => exception!("Unhandled assignment target pattern: {:?}", target),
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
            _ => exception!("Unhandled tuple assignment value: {:?}", value),
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
            ast::Expr::Constant(c) => {
                if let ast::Constant::Str(s) = &c.value {
                    Some(s.clone())
                } else {
                    except_none!("constant string: {:?}", c)
                }
            }
            ast::Expr::Name(_) => Some(format!("{{{}}}", "PLACEHOLDER")),
            ast::Expr::Call(c) => Self::extract_from_call(c),
            ast::Expr::FormattedValue(f) => Self::extract_string_content(&f.value),
            ast::Expr::BinOp(b) => Self::extract_from_bin_op(b),
            ast::Expr::JoinedStr(j) => j.values.iter().try_fold(String::new(), |mut acc, val| {
                Self::extract_string_content(val).map(|s| {
                    acc.push_str(&s);
                    acc
                })
            }),

            _ => except_none!("Not a string literal: {:?}", expr),
        }
    }
    fn extract_from_bin_op(v: &ast::ExprBinOp<TextRange>) -> Option<String> {
        dbg!(v);
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
                        except_ret!(vec![], "Unhandled rhs expr type: {:?}", otherwise)
                    }
                };
                if let ConstType::Str(fmt_string) = expr_string {
                    return format_python_string(&fmt_string, &rhs);
                }
                None
            }
            otherwise => except_none!("Unhandled binary operator: {:?}", otherwise),
        }
    }

    fn extract_from_call(v: &ast::ExprCall<TextRange>) -> Option<String> {
        dbg!(v);

        match &*v.func {
            ast::Expr::Attribute(ast::ExprAttribute { attr, value, .. })
                if attr.as_str() == "format" =>
            {
                None
                // Self::analyze_assignment(&self, assign, contexts);
            }
            _ => Some(format!("{{{}}}", "PLACEHOLDER")),
        }
    }
}
fn extract_expr(expr: &ast::Expr<TextRange>) -> ConstType {
    if let ast::Expr::Constant(v) = expr {
        extract_expr_const(v)
    } else {
        except_ret!(
            ConstType::Unhandled,
            "Unhandled Expression for Extraction: {:?}",
            expr
        )
    }
}

fn extract_const(c: &ast::Constant) -> ConstType {
    match c {
        ast::Constant::Str(s) => ConstType::Str(s.clone()),
        ast::Constant::Int(i) => ConstType::Int(i.to_string()),
        ast::Constant::Bool(b) => ConstType::Bool(*b),
        ast::Constant::Float(f) => ConstType::Float(*f),
        ast::Constant::Tuple(t) => ConstType::Tuple(t.iter().map(extract_const).collect()),
        _ => except_ret!(ConstType::Unhandled, "Unhandled Constant: {:?}", c),
    }
}

fn extract_expr_const(c: &ast::ExprConstant<TextRange>) -> ConstType {
    extract_const(&c.value)
}

fn format_python_string(format_str: &str, values: &[ConstType]) -> Option<String> {
    let re = Regex::new(r"%[-+0 #]*(?:\*|\d+)?(?:\.(?:\*|\d+))?[hlL]?[sdifgGeEoxXcubp%]").ok()?;
    let mut result = format_str.to_string();
    let mut value_index = 0;
    let matches: Vec<_> = re.find_iter(format_str).collect();

    for m in matches.iter().rev() {
        let specifier = m.as_str();
        if specifier == "%%" {
            result.replace_range(m.range(), "%");
            continue;
        }
        if value_index >= values.len() {
            return None;
        }
        let value = &values[values.len() - 1 - value_index];
        value_index += 1;

        let conversion = specifier.chars().last().unwrap();
        let replacement = match conversion {
            's' => Some(value.to_string()),
            'd' | 'i' => formatters::format_value_as_int(value),
            'u' => formatters::format_value_as_unsigned(value),
            'b' => formatters::format_value_as_binary(value),
            'f' | 'F' => formatters::format_value_as_float(value, specifier),
            'g' | 'G' => formatters::format_value_as_general(value, specifier),
            'e' | 'E' => formatters::format_value_as_scientific(value, specifier),
            'o' => formatters::format_value_as_octal(value),
            'x' => formatters::format_value_as_hex(value, false),
            'X' => formatters::format_value_as_hex(value, true),
            'c' => formatters::format_value_as_char(value),
            'p' => formatters::format_value_as_pointer(value),
            _ => except_none!("Unhandled format conversion specifier: {}", conversion),
        };

        result.replace_range(m.range(), &replacement?);
    }
    Some(result)
}

// Usage example:
// let format_str = "select * from %s where id = %d";
// let values = vec![ConstType::Str("users".to_string()), ConstType::Int("123".to_string())];
// let result = format_python_string(format_str, &values);
// // Result: Some("select * from users where id = 123")

#[derive(Debug)]
pub enum ConstType {
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
            Self::Str(s) => write!(f, "{s}"),
            Self::Int(i) => write!(f, "{i}"),
            Self::Float(fl) => write!(f, "{fl}"),
            // Using numeric booleans for maximum db compatibility
            Self::Bool(b) => write!(f, "{}", if *b { "1" } else { "0" }),
            Self::Tuple(t) => {
                write!(f, "(")?;
                for (i, item) in t.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{item}")?;
                }
                write!(f, ")")
            }
            Self::Unhandled => write!(f, "<unhandled>"),
        }
    }
}
