use regex::Regex;
use rustpython_parser::{Parse, ast};
use std::fs;

// ANSI color codes
const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const RED: &str = "\x1b[31m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const BLUE: &str = "\x1b[34m";
const MAGENTA: &str = "\x1b[35m";
const CYAN: &str = "\x1b[36m";
const GRAY: &str = "\x1b[90m";
const BRIGHT_RED: &str = "\x1b[91m";
const BRIGHT_GREEN: &str = "\x1b[92m";
const BRIGHT_YELLOW: &str = "\x1b[93m";
const BRIGHT_BLUE: &str = "\x1b[94m";

#[derive(Debug, Clone)]
struct SqlContext {
    file_path: String,
    line_number: usize,
    column: usize,
    variable_name: Option<String>,
    function_name: Option<String>,
    sql_content: String,
    original_sql: String, // Keep original for reference
}

fn main() {
    let path = "test_file.py";
    let python_source = fs::read_to_string(path).unwrap();
    let python_statements = ast::Suite::parse(&python_source, path).unwrap();

    println!(
        "{}{}🔍 Analyzing Python file: {}{}{}",
        BOLD, CYAN, BRIGHT_BLUE, path, RESET
    );
    println!();

    analyze_ast(&python_statements, path, &python_source);
}

fn analyze_ast(suite: &ast::Suite, file_path: &str, source_code: &str) {
    for stmt in suite {
        analyze_statement(stmt, file_path, source_code);
    }
}

fn analyze_statement(stmt: &ast::Stmt, file_path: &str, source_code: &str) {
    match stmt {
        // Variable assignments: query = "SELECT ..."
        ast::Stmt::Assign(assign) => {
            // Check if any target is a simple name
            for target in &assign.targets {
                if let ast::Expr::Name(name) = target {
                    let var_name = &name.id;

                    // Check if variable name matches SQL patterns
                    if is_sql_variable_name(var_name) {
                        // Extract string value if it's a string literal
                        if let ast::Expr::Constant(constant) = &*assign.value {
                            if let ast::Constant::Str(sql_string) = &constant.value {
                                let context = SqlContext {
                                    file_path: file_path.to_string(),
                                    line_number: assign.range.start().into(),
                                    column: assign.range.end().into(),
                                    variable_name: Some(var_name.to_string()),
                                    function_name: None,
                                    sql_content: sql_string.clone(),
                                    original_sql: sql_string.clone(),
                                };

                                println!(
                                    "{} Found SQL variable '{}{}{}': {}{}",
                                    BLUE,
                                    BOLD,
                                    var_name,
                                    RESET,
                                    YELLOW,
                                    sql_string.trim()
                                );
                                validate_sql_with_context(&context, source_code);
                            }
                        }
                    }
                }
            }
        }

        // Function definitions
        ast::Stmt::FunctionDef(func) => {
            for stmt in &func.body {
                analyze_statement(stmt, file_path, source_code);
            }
        }

        // Class definitions
        ast::Stmt::ClassDef(class) => {
            println!(
                "{}  Analyzing class: {}{}{}",
                MAGENTA, BOLD, class.name, RESET
            );
            for stmt in &class.body {
                analyze_statement(stmt, file_path, source_code);
            }
        }

        _ => {
            // Handle other statement types as needed
        }
    }
}

// Check if variable name suggests it contains SQL
fn is_sql_variable_name(name: &str) -> bool {
    matches!(
        name,
        "_sql" | "query" | "sql" | "statement" | "cmd" | "command"
    )
}

// Check if function name suggests it takes SQL parameters
fn is_sql_function_name(name: &str) -> bool {
    matches!(
        name,
        "execute" | "query" | "fetchall" | "fetchone" | "executemany"
    )
}

// Replace various placeholder patterns with valid SQL values
fn normalize_sql_placeholders(sql: &str) -> (String, Vec<String>) {
    let mut normalized = sql.to_string();
    let mut placeholders_found = Vec::new();

    // Define placeholder patterns and their replacements
    let placeholder_patterns = vec![
        // Question mark placeholders: ?
        (Regex::new(r"\?").unwrap(), "1", "? (positional)"),
        // Named placeholders: :param, :name, etc.
        (Regex::new(r":(\w+)").unwrap(), "1", ": (named)"),
        // Python format placeholders: %s, %d, %(name)s, etc.
        (
            Regex::new(r"%\((\w+)\)s").unwrap(),
            "'placeholder'",
            "%(name)s (dict format)",
        ),
        (
            Regex::new(r"%\((\w+)\)d").unwrap(),
            "1",
            "%(name)d (dict format)",
        ),
        (
            Regex::new(r"%s").unwrap(),
            "'placeholder'",
            "%s (string format)",
        ),
        (Regex::new(r"%d").unwrap(), "1", "%d (integer format)"),
        // PostgreSQL/psycopg2 placeholders: %s, %(name)s
        (
            Regex::new(r"%\((\w+)\)s").unwrap(),
            "'placeholder'",
            "psycopg2 %(name)s",
        ),
        // MySQL/pymysql placeholders: %s
        (Regex::new(r"%s").unwrap(), "'placeholder'", "pymysql %s"),
        // SQLite3 named placeholders: :name
        (Regex::new(r":(\w+)").unwrap(), "1", "sqlite3 :name"),
        // Dollar placeholders: $1, $2, etc. (PostgreSQL)
        (Regex::new(r"\$(\d+)").unwrap(), "1", "$n (PostgreSQL)"),
        // Curly brace placeholders: {param}, {0}, {1}
        (
            Regex::new(r"\{(\w+)\}").unwrap(),
            "'placeholder'",
            "{name} (format)",
        ),
        (
            Regex::new(r"\{(\d+)\}").unwrap(),
            "'placeholder'",
            "{n} (positional)",
        ),
    ];

    for (pattern, replacement, description) in placeholder_patterns {
        if pattern.is_match(&normalized) {
            placeholders_found.push(description.to_string());
            normalized = pattern.replace_all(&normalized, replacement).to_string();
        }
    }

    // Remove duplicate placeholder descriptions
    placeholders_found.sort();
    placeholders_found.dedup();

    (normalized, placeholders_found)
}

// Check if SQL looks like a template that shouldn't be parsed
fn is_sql_template(sql: &str) -> bool {
    let sql_trimmed = sql.trim();

    // Check for obvious templates or fragments
    if sql_trimmed.is_empty() {
        return true;
    }

    // Check if it's mostly placeholders
    let placeholder_count = sql.matches('?').count()
        + sql.matches("%s").count()
        + sql.matches(":").count()
        + sql.matches("$").count();

    let word_count = sql.split_whitespace().count();

    // If more than 50% are placeholders, consider it a template
    placeholder_count as f64 / word_count as f64 > 0.5
}

// Validate SQL using sqlparser-rs with detailed error context
fn validate_sql_with_context(context: &SqlContext, source_code: &str) {
    use sqlparser::dialect::PostgreSqlDialect;
    use sqlparser::parser::Parser;

    let dialect = PostgreSqlDialect {};

    // First, check if this looks like a template that shouldn't be parsed
    if is_sql_template(&context.sql_content) {
        println!(
            "  {}{}ℹ{} {}SQL template detected - skipping validation{}",
            BOLD, BLUE, RESET, GRAY, RESET
        );
        return;
    }

    // Normalize placeholders
    let (normalized_sql, placeholders) = normalize_sql_placeholders(&context.sql_content);

    // Show what placeholders were found
    if !placeholders.is_empty() {
        println!(
            "  {}{}🔧{} {}Normalized placeholders: {}{}",
            BOLD,
            CYAN,
            RESET,
            GRAY,
            placeholders.join(", "),
            RESET
        );
    }

    match Parser::parse_sql(&dialect, &normalized_sql) {
        Ok(statements) => {
            println!(
                "  {}{}✓{} {}Valid SQL ({} statements){}",
                BOLD,
                BRIGHT_GREEN,
                RESET,
                GREEN,
                statements.len(),
                RESET
            );

            if !placeholders.is_empty() {
                println!(
                    "    {}Note:{} Original contained placeholders: {}{}{}",
                    BLUE,
                    RESET,
                    YELLOW,
                    placeholders.join(", "),
                    RESET
                );
            }
        }
        Err(e) => {
            // Try alternative approaches if normalization failed
            if !placeholders.is_empty() {
                // Try with different normalization strategies
                if try_alternative_normalization(&context, &e, source_code) {
                    return;
                }
            }

            print_sql_error(context, &e, source_code, &placeholders);
        }
    }
}

// Try alternative normalization strategies
fn try_alternative_normalization(
    context: &SqlContext,
    _original_error: &sqlparser::parser::ParserError,
    source_code: &str,
) -> bool {
    use sqlparser::dialect::PostgreSqlDialect;
    use sqlparser::parser::Parser;

    let dialect = PostgreSqlDialect {};

    // Strategy 1: Replace all placeholders with NULL
    let null_normalized = replace_all_placeholders_with_null(&context.sql_content);
    if let Ok(statements) = Parser::parse_sql(&dialect, &null_normalized) {
        println!(
            "  {}{}✓{} {}Valid SQL with NULL substitution ({} statements){}",
            BOLD,
            BRIGHT_GREEN,
            RESET,
            GREEN,
            statements.len(),
            RESET
        );
        println!(
            "    {}Note:{} Placeholders replaced with NULL for validation",
            BLUE, RESET
        );
        return true;
    }

    // Strategy 2: Try to extract just the SQL structure without placeholders
    let structure_sql = extract_sql_structure(&context.sql_content);
    if !structure_sql.is_empty() {
        if let Ok(statements) = Parser::parse_sql(&dialect, &structure_sql) {
            println!(
                "  {}{}⚠{} {}SQL structure appears valid ({} statements){}",
                BOLD,
                YELLOW,
                RESET,
                YELLOW,
                statements.len(),
                RESET
            );
            println!(
                "    {}Note:{} Validated SQL structure only, placeholders ignored",
                BLUE, RESET
            );
            return true;
        }
    }

    false
}

// Replace all placeholder patterns with NULL
fn replace_all_placeholders_with_null(sql: &str) -> String {
    let mut result = sql.to_string();

    // Replace various placeholder types with NULL
    let patterns = vec![
        r"\?",           // ?
        r":\w+",         // :param
        r"%\(\w+\)[sd]", // %(name)s, %(name)d
        r"%[sd]",        // %s, %d
        r"\$\d+",        // $1, $2
        r"\{\w*\}",      // {name}, {}
    ];

    for pattern in patterns {
        if let Ok(re) = Regex::new(pattern) {
            result = re.replace_all(&result, "NULL").to_string();
        }
    }

    result
}

// Extract SQL structure by removing placeholder-heavy parts
fn extract_sql_structure(sql: &str) -> String {
    // This is a simple approach - you might want to make it more sophisticated
    let lines: Vec<&str> = sql.lines().collect();
    let mut structure_lines = Vec::new();

    for line in lines {
        let trimmed = line.trim();
        if !trimmed.is_empty() && !trimmed.starts_with("--") && !is_mostly_placeholders(trimmed) {
            structure_lines.push(line);
        }
    }

    structure_lines.join("\n")
}

// Check if a line is mostly placeholders
fn is_mostly_placeholders(line: &str) -> bool {
    let placeholder_chars = line
        .chars()
        .filter(|&c| c == '?' || c == '%' || c == ':' || c == '$' || c == '{' || c == '}')
        .count();
    let total_chars = line.chars().filter(|c| !c.is_whitespace()).count();

    if total_chars == 0 {
        return false;
    }

    placeholder_chars as f64 / total_chars as f64 > 0.3
}

fn print_sql_error(
    context: &SqlContext,
    error: &sqlparser::parser::ParserError,
    source_code: &str,
    placeholders: &[String],
) {
    use sqlparser::parser::ParserError;

    // Determine error category with colors
    let (error_type, error_color) = match error {
        ParserError::TokenizerError(_) => ("Tokenization Error", BRIGHT_RED),
        ParserError::ParserError(_) => ("Syntax Error", RED),
        ParserError::RecursionLimitExceeded => ("Complexity Error", YELLOW),
    };

    // Get the source line for context
    let source_lines: Vec<&str> = source_code.lines().collect();
    let line_content = if context.line_number > 0 && context.line_number <= source_lines.len() {
        source_lines[context.line_number - 1]
    } else {
        ""
    };

    // Print formatted error with colors
    println!(
        "  {}{}✗{} {}{}{} in {}{}:{}{}",
        BOLD,
        BRIGHT_RED,
        RESET,
        BOLD,
        error_color,
        error_type,
        RESET,
        CYAN,
        context.file_path,
        context.line_number,
    );

    if let Some(var_name) = &context.variable_name {
        println!(
            "    {}Variable:{} {}{}{}",
            BLUE, RESET, BOLD, var_name, RESET
        );
    }

    if let Some(func_name) = &context.function_name {
        println!(
            "    {}Function:{} {}{}{}",
            YELLOW, RESET, BOLD, func_name, RESET
        );
    }

    println!("    {}Error:{} {}", RED, RESET, error);
    println!(
        "    {}SQL:{} {}{}{}",
        MAGENTA,
        RESET,
        YELLOW,
        context.original_sql.trim(),
        RESET
    );

    if !placeholders.is_empty() {
        println!(
            "    {}Placeholders found:{} {}{}{}",
            CYAN,
            RESET,
            GRAY,
            placeholders.join(", "),
            RESET
        );
    }

    // Show source code context with syntax highlighting
    if !line_content.is_empty() {
        println!("    {}Source:{}", CYAN, RESET);
        println!(
            "      {}{} |{} {}",
            YELLOW,
            context.line_number,
            RESET,
            line_content.trim()
        );

        // Add a pointer to approximate location
        if context.column > 0 {
            let pointer_indent = format!("      {} | ", context.line_number).len() + context.column;
            println!(
                "      {}{}^{}",
                " ".repeat(pointer_indent),
                BRIGHT_RED,
                RESET
            );
        }
    }

    // Suggest common fixes based on error type
    suggest_fix(error, &context.original_sql, !placeholders.is_empty());
    println!();
}

fn suggest_fix(error: &sqlparser::parser::ParserError, sql: &str, has_placeholders: bool) {
    let sql_upper = sql.to_uppercase();

    print!("    {} Hint:{} ", BRIGHT_YELLOW, RESET);

    if has_placeholders {
        println!("SQL contains placeholders - this might be a parameterized query template");
    } else if sql_upper.contains("SELCT") {
        println!(
            "Did you mean '{}SELECT{}' instead of '{}SELCT{}'?",
            GREEN, RESET, RED, RESET
        );
    } else if sql_upper.starts_with("SELEC ") {
        println!(
            "Did you mean '{}SELECT{}' (missing '{}T{})?",
            GREEN, RESET, RED, RESET
        );
    } else if sql.matches('(').count() != sql.matches(')').count() {
        println!("Check for missing or extra {}parentheses{}", YELLOW, RESET);
    } else if sql.matches('\'').count() % 2 != 0 {
        println!("Check for unclosed {}string literals{}", YELLOW, RESET);
    } else if error.to_string().contains("Expected") {
        println!(
            "Check SQL syntax - missing {}keywords{} or {}punctuation{}",
            BLUE, RESET, MAGENTA, RESET
        );
    } else {
        println!("Review the SQL syntax for potential issues");
    }
}
