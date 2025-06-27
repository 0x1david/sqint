#![allow(clippy::needless_collect)]
use crate::finder_types::{FinderType, SqlResult};
use crate::format::format_python_string;
use crate::{SqlFinder, SqlString};
use logging::{bail, bail_with, debug, warn};
use regex::Regex;
use rustpython_parser::ast::Operator;
use rustpython_parser::{
    ast::{self, Identifier},
    text_size::TextRange,
};

// Public API
impl SqlFinder {
    pub(super) fn analyze_assignment(&self, assign: &ast::StmtAssign) -> Vec<SqlString> {
        debug!("Analyzing assignment with {} targets", assign.targets.len());
        
        let mut results = vec![];
        assign.targets.iter().enumerate().for_each(|(i, target)| {
            debug!("Processing assignment target {}/{}: {:?}", 
                   i + 1, assign.targets.len(), std::mem::discriminant(target));
            
            let sql_results = self.process_assignment_target(
                target,
                &assign.value,
                assign.range.start().to_usize(),
            );
            
            debug!("Assignment target {} yielded {} SQL results", i + 1, sql_results.len());
            
            let sql_strings: Vec<SqlString> = sql_results
                .into_iter()
                .filter_map(|result| {
                    match result.into_sql_string() {
                        Some(sql_string) => {
                            debug!("Converted SQL result to string: variable='{}', content preview='{}'", 
                                   sql_string.variable_name, 
                                   &sql_string.sql_content[..sql_string.sql_content.len().min(50)]);
                            Some(sql_string)
                        }
                        None => {
                            debug!("SQL result did not convert to valid SQL string");
                            None
                        }
                    }
                })
                .collect();
            
            results.extend(sql_strings);
        });
        
        debug!("Assignment analysis complete: found {} SQL strings total", results.len());
        results
    }

    pub(super) fn analyze_stmt_expr(&self, e: &ast::StmtExpr) -> Vec<SqlString> {
        debug!("Analyzing expression statement");
        
        let results = self.process_expr_stmt(&e.value, e.range.start().to_usize());
        debug!("Expression statement yielded {} SQL results", results.len());
        
        let sql_strings: Vec<SqlString> = results
            .into_iter()
            .filter_map(|result| {
                match result.into_sql_string() {
                    Some(sql_string) => {
                        debug!("Expression converted to SQL string: variable='{}', content preview='{}'", 
                               sql_string.variable_name, 
                               &sql_string.sql_content[..sql_string.sql_content.len().min(50)]);
                        Some(sql_string)
                    }
                    None => {
                        debug!("Expression result did not convert to valid SQL string");
                        None
                    }
                }
            })
            .collect();
        
        debug!("Expression statement analysis complete: {} SQL strings", sql_strings.len());
        sql_strings
    }

    pub(super) fn analyze_annotated_assignment(
        &self,
        assign: &ast::StmtAnnAssign,
    ) -> Vec<SqlString> {
        debug!("Analyzing annotated assignment");
        
        let mut results = vec![];
        if let Some(val) = &assign.value {
            debug!("Annotated assignment has value, processing...");
            let sql_results = self.process_assignment_target(
                &assign.target,
                val,
                assign.range.start().to_usize(),
            );
            
            debug!("Annotated assignment yielded {} SQL results", sql_results.len());
            
            let sql_strings: Vec<SqlString> = sql_results
                .into_iter()
                .filter_map(|result| {
                    match result.into_sql_string() {
                        Some(sql_string) => {
                            debug!("Annotated assignment converted to SQL string: variable='{}', content preview='{}'", 
                                   sql_string.variable_name, 
                                   &sql_string.sql_content[..sql_string.sql_content.len().min(50)]);
                            Some(sql_string)
                        }
                        None => {
                            debug!("Annotated assignment result did not convert to valid SQL string");
                            None
                        }
                    }
                })
                .collect();
            
            results.extend(sql_strings);
        } else {
            debug!("Annotated assignment has no value, skipping");
        }
        
        debug!("Annotated assignment analysis complete: {} SQL strings", results.len());
        results
    }
}

// Internal processing
impl SqlFinder {
    fn process_expr_stmt(&self, value: &ast::Expr, byte_offset: usize) -> Vec<SqlResult> {
        debug!("Processing expression statement at byte offset {}", byte_offset);
        
        match value {
            ast::Expr::Call(call) => {
                debug!("Expression is a function call");
                self.process_call_expr(call, byte_offset)
            },
            ast::Expr::Attribute(_) => match value {
                ast::Expr::Call(call) => {
                    debug!("Expression is an attribute that's also a call");
                    self.process_call_expr(call, byte_offset)
                },
                _ => bail_with!(vec![], "Unhandled expr_stmt value pattern: {:?}", value),
            },
            ast::Expr::Constant(_) => {
                debug!("Expression is a constant (likely code comment), skipping");
                vec![]
            },
            _ => {
                bail_with!(vec![], "Unhandled expr_stmt value pattern: {:?}", value)
            }
        }
    }

    fn process_call_expr(&self, call: &ast::ExprCall, byte_offset: usize) -> Vec<SqlResult> {
        let function_name = Self::extract_function_name(&call.func);
        debug!("Processing call to function: '{}'", function_name);

        if !self.config.is_sql_function_name(&function_name) {
            debug!("Function '{}' is not configured as SQL function, skipping", function_name);
            return vec![];
        }

        debug!("Function '{}' is configured as SQL function, processing {} args and {} keywords", 
               function_name, call.args.len(), call.keywords.len());

        let keyword_results: Vec<SqlResult> = call.keywords.iter().enumerate().flat_map(|(i, keyword)| {
            debug!("Processing keyword argument {}/{}", i + 1, call.keywords.len());
            
            keyword.arg.as_ref().map_or_else(
                || {
                    debug!("Keyword argument {} has no name (unpacked dict), skipping", i + 1);
                    vec![]
                },
                |arg_name| {
                    debug!("Keyword argument {}: name='{}'", i + 1, arg_name);
                    if self.config.is_sql_class_name(arg_name) {
                        debug!("Keyword '{}' matches SQL class name, extracting content", arg_name);
                        self.extract_content_flattened(&keyword.value, &function_name, byte_offset)
                    } else {
                        debug!("Keyword '{}' does not match SQL class name, skipping", arg_name);
                        vec![]
                    }
                }
            )
        }).collect();

        debug!("Keyword arguments yielded {} SQL results", keyword_results.len());

        let arg_results: Vec<SqlResult> = call
            .args
            .iter()
            .enumerate()
            .flat_map(|(i, arg)| {
                debug!("Processing positional argument {}/{}", i + 1, call.args.len());
                self.extract_content_flattened(arg, &function_name, byte_offset)
            })
            .collect();

        debug!("Positional arguments yielded {} SQL results", arg_results.len());

        let total_results: Vec<SqlResult> = arg_results.into_iter().chain(keyword_results).collect();
        debug!("Function call '{}' yielded {} total SQL results", function_name, total_results.len());
        
        total_results
    }

    fn extract_content_flattened(
        &self,
        expr: &ast::Expr,
        variable_name: &str,
        byte_offset: usize,
    ) -> Vec<SqlResult> {
        debug!("Extracting content from expression for variable '{}' at offset {}", 
               variable_name, byte_offset);
        
        match expr {
            ast::Expr::List(ast::ExprList { elts, .. }) => {
                debug!("Expression is a list with {} elements", elts.len());
                elts.iter()
                    .enumerate()
                    .flat_map(|(i, elem)| {
                        debug!("Processing list element {}/{}", i + 1, elts.len());
                        self.extract_content_flattened(elem, variable_name, byte_offset)
                    })
                    .collect()
            },
            ast::Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                debug!("Expression is a tuple with {} elements", elts.len());
                elts.iter()
                    .enumerate()
                    .flat_map(|(i, elem)| {
                        debug!("Processing tuple element {}/{}", i + 1, elts.len());
                        self.extract_content_flattened(elem, variable_name, byte_offset)
                    })
                    .collect()
            },

            ast::Expr::Dict(ast::ExprDict { values, .. }) => {
                debug!("Expression is a dictionary with {} values", values.len());
                values.iter()
                    .enumerate()
                    .flat_map(|(i, elem)| {
                        debug!("Processing dict value {}/{}", i + 1, values.len());
                        self.extract_content_flattened(elem, variable_name, byte_offset)
                    })
                    .collect()
            },
            ast::Expr::BoolOp(ast::ExprBoolOp { values, .. }) => {
                debug!("Expression is a boolean operation with {} values", values.len());
                values.iter()
                    .enumerate()
                    .flat_map(|(i, elem)| {
                        debug!("Processing bool op value {}/{}", i + 1, values.len());
                        self.extract_content_flattened(elem, variable_name, byte_offset)
                    })
                    .collect()
            },

            ast::Expr::BinOp(bin @ ast::ExprBinOp { op, .. })
                if *op == Operator::Add
                    || *op == Operator::Sub
                    || *op == Operator::Mult
                    || *op == Operator::Div =>
            {
                debug!("Expression is a binary operation: {:?}", op);
                self.extract_from_bin_op(bin)
                    .map_or_else(
                        || {
                            debug!("Binary operation did not yield extractable content");
                            Vec::new()
                        },
                        |content| {
                            debug!("Binary operation yielded content: '{}'", 
                                   &content.to_string()[..content.to_string().len().min(50)]);
                            vec![SqlResult {
                                byte_offset,
                                variable_name: variable_name.to_string(),
                                content,
                            }]
                        }
                    )
            }

            _ => {
                debug!("Expression is a direct content extraction");
                self.extract_content(expr).map_or_else(
                    || {
                        debug!("Direct content extraction failed");
                        Vec::new()
                    },
                    |content| {
                        debug!("Direct content extraction succeeded: '{}'", 
                               &content.to_string()[..content.to_string().len().min(50)]);
                        vec![SqlResult {
                            byte_offset,
                            variable_name: variable_name.to_string(),
                            content,
                        }]
                    }
                )
            }
        }
    }

    fn process_assignment_target(
        &self,
        target: &ast::Expr,
        value: &ast::Expr,
        byte_offset: usize,
    ) -> Vec<SqlResult> {
        debug!("Processing assignment target at byte offset {}", byte_offset);
        
        match target {
            ast::Expr::Name(name) => {
                debug!("Assignment target is a name: '{}'", name.id);
                self.process_by_ident(&name.id, value, byte_offset)
            },
            ast::Expr::Attribute(att) => {
                debug!("Assignment target is an attribute: '{}'", att.attr);
                self.process_by_ident(&att.attr, value, byte_offset)
            },
            ast::Expr::Tuple(tuple) => {
                debug!("Assignment target is a tuple with {} elements", tuple.elts.len());
                self.handle_tuple_assignment(&tuple.elts, value, byte_offset)
            },
            ast::Expr::List(list) => {
                debug!("Assignment target is a list with {} elements", list.elts.len());
                self.handle_tuple_assignment(&list.elts, value, byte_offset)
            },
            ast::Expr::Subscript(_) => {
                debug!("Assignment target is a subscript (hashmap access), skipping");
                vec![]
            },
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
    ) -> Vec<SqlResult> {
        debug!("Processing identifier assignment: '{}'", name);
        
        if self.config.is_sql_variable_name(name) {
            debug!("Identifier '{}' matches SQL variable name, extracting content", name);
            return self.extract_content_flattened(value, name, byte_offset);
        }
        
        debug!("Identifier '{}' does not match SQL variable name, skipping", name);
        vec![]
    }

    fn handle_tuple_assignment(
        &self,
        targets: &[ast::Expr],
        value: &ast::Expr,
        byte_offset: usize,
    ) -> Vec<SqlResult> {
        debug!("Handling tuple assignment with {} targets", targets.len());
        
        let has_sql_target = targets
            .iter()
            .any(|target| self.target_contains_sql_variable(target));

        if !has_sql_target {
            debug!("No SQL variables found in tuple targets, skipping");
            return vec![];
        }
        
        debug!("Found SQL variables in tuple targets, processing...");
        
        match value {
            ast::Expr::Tuple(tuple_value) => {
                debug!("Tuple assignment value is also a tuple with {} elements", tuple_value.elts.len());
                self.process_paired_assignments(targets, &tuple_value.elts, byte_offset)
            },
            ast::Expr::List(list_value) => {
                debug!("Tuple assignment value is a list with {} elements", list_value.elts.len());
                self.process_paired_assignments(targets, &list_value.elts, byte_offset)
            },
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
    ) -> Vec<SqlResult> {
        debug!("Processing paired assignments: {} targets, {} values", targets.len(), values.len());
        
        let mut results = Vec::new();
        let mut value_idx = 0;

        for (target_idx, target) in targets.iter().enumerate() {
            debug!("Processing paired assignment {}/{}", target_idx + 1, targets.len());
            
            if let ast::Expr::Starred(ast::ExprStarred {
                value: starred_target,
                ..
            }) = target
            {
                let starred_count = values.len() - targets.len() + 1;
                debug!("Found starred target, consuming {} values", starred_count);
                
                let consumed_values = &values[value_idx..value_idx + starred_count];
                let new_list_expr = ast::Expr::List(ast::ExprList {
                    range: TextRange::default(),
                    elts: consumed_values.to_vec(),
                    ctx: ast::ExprContext::Load,
                });
                let target_results =
                    self.process_assignment_target(starred_target, &new_list_expr, byte_offset);
                    
                debug!("Starred target yielded {} results", target_results.len());
                results.extend(target_results);
                value_idx += starred_count;
            } else {
                debug!("Processing regular target with value at index {}", value_idx);
                let target_results =
                    self.process_assignment_target(target, &values[value_idx], byte_offset);
                    
                debug!("Regular target yielded {} results", target_results.len());
                results.extend(target_results);
                value_idx += 1;
            }
        }

        debug!("Paired assignments complete: {} total results", results.len());
        results
    }

    fn target_contains_sql_variable(&self, target: &ast::Expr) -> bool {
        let contains_sql = match target {
            ast::Expr::Name(name) => {
                let is_sql = self.config.is_sql_variable_name(&name.id);
                debug!("Checking if name '{}' is SQL variable: {}", name.id, is_sql);
                is_sql
            },
            ast::Expr::Attribute(att) => {
                let is_sql = self.config.is_sql_variable_name(&att.attr);
                debug!("Checking if attribute '{}' is SQL variable: {}", att.attr, is_sql);
                is_sql
            },
            ast::Expr::Tuple(tuple) => {
                debug!("Checking tuple with {} elements for SQL variables", tuple.elts.len());
                tuple.elts.iter().any(|t| self.target_contains_sql_variable(t))
            },
            ast::Expr::List(list) => {
                debug!("Checking list with {} elements for SQL variables", list.elts.len());
                list.elts.iter().any(|t| self.target_contains_sql_variable(t))
            },
            ast::Expr::Starred(starred) => {
                debug!("Checking starred expression for SQL variables");
                self.target_contains_sql_variable(&starred.value)
            },
            _ => {
                debug!("Target type does not contain SQL variables: {:?}", std::mem::discriminant(target));
                false
            }
        };
        
        debug!("Target contains SQL variable: {}", contains_sql);
        contains_sql
    }
}

// Content extraction
impl SqlFinder {
    fn extract_function_name(func_expr: &ast::Expr) -> String {
        let name = match func_expr {
            ast::Expr::Name(name) => {
                debug!("Function name is simple identifier: '{}'", name.id);
                name.id.to_string()
            },
            ast::Expr::Attribute(attr) => {
                let full_name = format!("{}.{}", Self::extract_function_name(&attr.value), attr.attr);
                debug!("Function name is attribute access: '{}'", full_name);
                full_name
            },
            _ => bail_with!(
                String::new(),
                "Unknown function expression: {:?}",
                &func_expr
            ),
        };
        
        debug!("Extracted function name: '{}'", name);
        name
    }

    fn extract_content(&self, expr: &ast::Expr) -> Option<FinderType> {
        debug!("Extracting content from expression type: {:?}", std::mem::discriminant(expr));
        
        let result = match expr {
            ast::Expr::Constant(c) => {
                debug!("Expression is a constant");
                Some(Self::extract_expr_const(c))
            },
            ast::Expr::Subscript(_) | ast::Expr::Name(_) | ast::Expr::Attribute(_) => {
                debug!("Expression is a placeholder (subscript/name/attribute)");
                Some(FinderType::Placeholder)
            },
            ast::Expr::Call(c) => {
                debug!("Expression is a function call");
                self.extract_call(c)
            },
            ast::Expr::FormattedValue(f) => {
                debug!("Expression is a formatted value (f-string component)");
                self.extract_content(&f.value)
            },
            ast::Expr::BinOp(b) => {
                debug!("Expression is a binary operation: {:?}", b.op);
                self.extract_from_bin_op(b)
            },
            ast::Expr::JoinedStr(j) => {
                debug!("Expression is a joined string (f-string) with {} parts", j.values.len());
                let parts: Option<Vec<FinderType>> = j
                    .values
                    .iter()
                    .enumerate()
                    .map(|(i, val)| {
                        debug!("Processing f-string part {}/{}", i + 1, j.values.len());
                        self.extract_content(val)
                    })
                    .collect();

                parts.map(|parts| {
                    let combined = parts.into_iter().map(|p| p.to_string()).collect::<String>();
                    debug!("F-string combined result: '{}'", &combined[..combined.len().min(50)]);
                    FinderType::Str(combined)
                })
            },
            _ => bail_with!(None, "Not extractable content: {:?}", expr),
        };
        
        if let Some(ref content) = result {
            debug!("Content extraction successful: '{}'", &content.to_string()[..content.to_string().len().min(50)]);
        } else {
            debug!("Content extraction failed");
        }
        
        result
    }
    
    fn extract_from_bin_op(&self, v: &ast::ExprBinOp<TextRange>) -> Option<FinderType> {
        debug!("Extracting from binary operation: {:?}", v.op);
        
        match &v.op {
            ast::Operator::Mod => {
                debug!("Binary operation is modulo (string formatting)");
                let expr_content = self.extract_content(&v.left)?;
                debug!("Left side extracted successfully");

                let (args, kwargs) = match &*v.right {
                    ast::Expr::Constant(c) => {
                        debug!("Right side is a constant");
                        (vec![Self::extract_expr_const(c)], vec![])
                    },

                    ast::Expr::Tuple(ast::ExprTuple { elts, .. })
                    | ast::Expr::List(ast::ExprList { elts, .. }) => {
                        debug!("Right side is a tuple/list with {} elements", elts.len());
                        let args = elts.iter()
                            .enumerate()
                            .filter_map(|(i, e)| {
                                debug!("Processing format arg {}/{}", i + 1, elts.len());
                                self.extract_content(e)
                            })
                            .collect();
                        (args, vec![])
                    },
                    ast::Expr::Dict(d) => {
                        debug!("Right side is a dictionary with {} key-value pairs", d.keys.len());
                        let keys: Vec<String> = d
                            .keys
                            .iter()
                            .enumerate()
                            .filter_map(|(i, k)| {
                                debug!("Processing dict key {}/{}", i + 1, d.keys.len());
                                k.as_ref()
                            })
                            .filter_map(|e| self.extract_content(e))
                            .map(|k| k.to_string())
                            .collect();

                        let values: Vec<FinderType> = d
                            .values
                            .iter()
                            .enumerate()
                            .filter_map(|(i, e)| {
                                debug!("Processing dict value {}/{}", i + 1, d.values.len());
                                self.extract_content(e)
                            })
                            .collect();

                        let kwargs: Vec<_> = keys.into_iter().zip(values).collect();
                        debug!("Extracted {} keyword arguments", kwargs.len());
                        (vec![], kwargs)
                    },
                    _ => bail_with!((vec![], vec![]), "Unhandled rhs expr type: {:?}", v.right),
                };

                match expr_content {
                    FinderType::Str(fmt_string) => {
                        debug!("Formatting string with {} positional and {} keyword args", 
                               args.len(), kwargs.len());
                        let result = format_python_string(&fmt_string, &args, &kwargs).map(FinderType::Str);
                        if let Some(ref formatted) = result {
                            debug!("String formatting successful: '{}'", 
                                   &formatted.to_string()[..formatted.to_string().len().min(50)]);
                        } else {
                            debug!("String formatting failed");
                        }
                        result
                    },
                    other => {
                        debug!("Left side is not a string, returning as-is");
                        Some(other)
                    },
                }
            },
            _ => {
                debug!("Binary operation is arithmetic");
                self.extract_arithmetic(&v.left, &v.right, v.op)
            },
        }
    }

    fn extract_arithmetic(
        &self,
        lhs: &ast::Expr,
        rhs: &ast::Expr,
        op: ast::Operator,
    ) -> Option<FinderType> {
        debug!("Extracting arithmetic operation: {:?}", op);
        
        let lhs_content = self.extract_content(lhs)?;
        debug!("Left operand extracted successfully");
        
        let rhs_content = self.extract_content(rhs)?;
        debug!("Right operand extracted successfully");

        let result = match op {
            ast::Operator::Add => {
                debug!("Performing addition");
                lhs_content + rhs_content
            },
            ast::Operator::Sub => {
                debug!("Performing subtraction");
                lhs_content - rhs_content
            },
            ast::Operator::Mult => {
                debug!("Performing multiplication");
                lhs_content * rhs_content
            },
            ast::Operator::Div => {
                debug!("Performing division");
                lhs_content / rhs_content
            },
            _ => bail!(None, "Unexpected operator in extraction: {:?}", op),
        };
        
        if let Some(ref res) = result {
            debug!("Arithmetic operation successful: '{}'", 
                   &res.to_string()[..res.to_string().len().min(50)]);
        } else {
            debug!("Arithmetic operation failed");
        }
        
        result
    }

    fn extract_call(&self, v: &ast::ExprCall<TextRange>) -> Option<FinderType> {
        debug!("Extracting from function call");
        
        match &*v.func {
            ast::Expr::Call(nested_call) => {
                debug!("Function call is nested, recursing");
                self.extract_call(nested_call)
            },
            ast::Expr::Attribute(ast::ExprAttribute { attr, value, .. }) => {
                debug!("Function call is method call: '{}'", attr);
                if attr.as_str() == "format" {
                    debug!("Method call is string.format(), processing");
                    self.extract_format_call(&v.args, &v.keywords, value)
                } else {
                    debug!("Method call is not format(), extracting from base object");
                    self.extract_content(value)
                }
            },
            ast::Expr::Name(name) => {
                debug!("Function call is simple function: '{}'", name.id);
                if self.config.is_sql_function_name(&name.id) {
                    debug!("Function '{}' is configured as SQL function", name.id);
                    v.args.iter().find_map(|arg| {
                        debug!("Trying to extract content from function argument");
                        self.extract_content(arg)
                    })
                } else {
                    debug!("Function '{}' is not configured as SQL function", name.id);
                    None
                }
            },
            _ => bail_with!(None, "Unhandled function call type: {:?}", v.func),
        }
    }

    fn extract_format_call(
        &self,
        args: &[ast::Expr],
        kwargs: &[ast::Keyword],
        value: &ast::Expr,
    ) -> Option<FinderType> {
        debug!("Processing string.format() call with {} args and {} kwargs", 
               args.len(), kwargs.len());
        
        let mut pos_fills = vec![];
        let mut kw_fills = vec![];
        let mut has_unpacked_dict = false;

        for (i, a) in args.iter().enumerate() {
            debug!("Processing format() positional argument {}/{}", i + 1, args.len());
            
            let parsed = match a {
                ast::Expr::Constant(c) => {
                    debug!("Format arg is constant");
                    vec![Self::extract_expr_const(c)]
                },
                ast::Expr::List(els) => {
                    debug!("Format arg is list with {} elements", els.elts.len());
                    els.elts
                        .iter()
                        .filter_map(|e| self.extract_content(e))
                        .collect()
                },
                ast::Expr::Subscript(_) | ast::Expr::Name(_) | ast::Expr::Call(_) => {
                    debug!("Format arg is placeholder");
                    vec![FinderType::Placeholder]
                },
                ast::Expr::BinOp(b) => {
                    debug!("Format arg is binary operation");
                    self.extract_from_bin_op(b)
                        .map_or_else(|| vec![FinderType::Unhandled], |content| vec![content])
                },
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

        debug!("Extracted {} positional format arguments", pos_fills.len());

        for (i, kw) in kwargs.iter().enumerate() {
            debug!("Processing format() keyword argument {}/{}", i + 1, kwargs.len());
            
            if let Some(name) = &kw.arg {
                debug!("Keyword argument name: '{}'", name);
                if let Some(val) = self.extract_content(&kw.value) {
                    debug!("Keyword argument value extracted successfully");
                    kw_fills.push((name.clone(), val));
                } else {
                    debug!("Failed to extract keyword argument value");
                }
            } else {
                debug!("Keyword argument has no name (unpacked dict)");
                has_unpacked_dict = true;
            }
        }

        debug!("Extracted {} keyword format arguments, has_unpacked_dict: {}", 
               kw_fills.len(), has_unpacked_dict);

        let base_content = self.extract_content(value)?;
        debug!("Base format string extracted successfully");
        
        let mut result = base_content.to_string();

        if has_unpacked_dict {
            debug!("Has unpacked dict, replacing all format placeholders with PLACEHOLDER");
            let re = Regex::new(r"\{[^}]+\}").expect("Broke the regex format call finder.");
            result = re.replace_all(&result, "{PLACEHOLDER}").to_string();
        } else {
            debug!("Processing numbered format placeholders");
            let numbered_re =
                Regex::new(r"\{(\d+)\}").expect("Broke the regex format call finder.");
            result = numbered_re
                .replace_all(&result, |caps: &regex::Captures| {
                    let index: usize = caps[1].parse().unwrap_or(0);
                    debug!("Replacing numbered placeholder {} with positional arg", index);
                    if index < pos_fills.len() {
                        pos_fills[index].clone()
                    } else {
                        warn!("Format placeholder index {} out of range (have {} args)", 
                              index, pos_fills.len());
                        "{PLACEHOLDER}".to_string()
                    }
                })
                .to_string();

            debug!("Processing unnamed format placeholders");
            for (i, f) in pos_fills.iter().enumerate() {
                debug!("Replacing unnamed placeholder with positional arg {}", i);
                result = result.replacen("{}", f, 1);
            }

            debug!("Processing named format placeholders");
            for (kw_name, val) in &kw_fills {
                let pat = format!("{{{kw_name}}}");
                debug!("Replacing named placeholder '{}' with value", pat);
                result = result.replace(&pat, &val.to_string());
            }
        }

        debug!("String formatting complete: '{}'", &result[..result.len().min(50)]);
        Some(FinderType::Str(result))
    }

    fn extract_expr_const(c: &ast::ExprConstant<TextRange>) -> FinderType {
        debug!("Extracting expression constant");
        Self::extract_const(&c.value)
    }

    fn extract_const(c: &ast::Constant) -> FinderType {
        let result = match c {
            ast::Constant::Str(s) => {
                debug!("Constant is string: '{}'", &s[..s.len().min(50)]);
                FinderType::Str(s.clone())
            },
            ast::Constant::Int(i) => {
                debug!("Constant is integer: {}", i);
                FinderType::Int(i.to_string())
            },
            ast::Constant::Bool(b) => {
                debug!("Constant is boolean: {}", b);
                FinderType::Bool(*b)
            },
            ast::Constant::Float(f) => {
                debug!("Constant is float: {}", f);
                FinderType::Float(*f)
            },
            ast::Constant::None => {
                debug!("Constant is None (unhandled)");
                FinderType::Unhandled
            },
            ast::Constant::Tuple(t) => {
                debug!("Constant is tuple with {} elements", t.len());
                FinderType::Tuple(t.iter().map(Self::extract_const).collect())
            },
            _ => bail_with!(FinderType::Unhandled, "Unhandled Constant: {:?}", c),
        };
        
        debug!("Constant extraction complete");
        result
    }
}
