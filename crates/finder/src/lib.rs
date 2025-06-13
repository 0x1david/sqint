use logging::{always_log, debug, error};
use rustpython_parser::{
    Parse,
    ast::{self, Identifier},
};
use std::{
    fmt, fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
struct SearchCtx {
    pub var_assign: bool,
    pub multiple_var_assig: bool,
}

#[derive(Debug, Clone)]
pub struct SqlExtract {
    pub file_path: String,
    pub strings: Vec<SqlString>,
}

/// Represents a detected SQL variable
#[derive(Debug, Clone)]
pub struct SqlString {
    pub byte_offset: usize,
    pub variable_name: String,
    pub sql_content: String,
}

#[derive(Debug, Clone)]
pub struct FinderConfig {
    pub variables: Vec<String>,
    pub min_sql_length: usize,
}

pub struct SqlFinder {
    config: FinderConfig,
    ctx: SearchCtx,
}

impl SqlFinder {
    pub fn new(config: FinderConfig) -> Self {
        Self {
            config,
            ctx: SearchCtx::default(),
        }
    }

    pub fn analyze_file(&self, file_path: &str) -> Option<SqlExtract> {
        let source_code = fs::read_to_string(file_path)
            .inspect_err(|e| error!("Failed to read file '{}': {}", file_path, e))
            .ok()?;

        let parsed = ast::Suite::parse(&source_code, file_path)
            .inspect_err(|e| error!("Failed to parse Python file: {}", e))
            .ok()?;

        let mut contexts = Vec::new();
        self.analyze_stmts(&parsed, file_path, &mut contexts);

        Some(SqlExtract {
            file_path: file_path.to_string(),
            strings: contexts,
        })
    }

    fn analyze_stmts(&self, suite: &ast::Suite, file_path: &str, contexts: &mut Vec<SqlString>) {
        for stmt in suite {
            match stmt {
                ast::Stmt::Assign(assign) if self.ctx.var_assign => {
                    self.analyze_assignment(assign, file_path, contexts);
                }
                _ => {} // TODO: Add more query detection contexts
            }
        }
    }

    fn analyze_assignment(
        &self,
        assign: &ast::StmtAssign,
        file_path: &str,
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
            if let Some(sql_content) = self.extract_string_content(value) {
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
        // Process each target-value pair
        for (target, value) in targets.iter().zip(values.iter()) {
            self.process_assignment_target(target, value, byte_offset, contexts);
        }

        // Handle cases where there are more targets than values or vice versa
        if targets.len() != values.len() {
            always_log!(
                "Mismatched tuple assignment: {} targets, {} values",
                targets.len(),
                values.len()
            );
        }
    }

    /// Extract string content from an expression (only handles string literals)
    fn extract_string_content(&self, expr: &ast::Expr) -> Option<String> {
        match expr {
            ast::Expr::Constant(constant) => match &constant.value {
                ast::Constant::Str(s) => Some(s.clone()),
                _ => None,
            },
            _ => None,
        }
    }

    /// Check if variable name suggests it contains SQL
    fn is_sql_variable_name(&self, name: &str) -> bool {
        let name_lower = name.to_lowercase();
        self.config
            .variables
            .iter()
            .any(|pattern| name_lower.contains(&pattern.to_lowercase()))
    }
}

/// Collects and flattens all files in a list of files/directories
pub fn collect_files(paths: &[PathBuf]) -> Vec<PathBuf> {
    debug!("{:?}", paths);
    paths
        .iter()
        .flat_map(|path| {
            if path.is_file() {
                debug!("Found file {}", path.display());
                vec![path.clone()]
            } else if path.is_dir() {
                read_directory_files(path)
            } else {
                Vec::new()
            }
        })
        .collect()
}

/// Reads all files in a directory
fn read_directory_files(dir_path: &Path) -> Vec<PathBuf> {
    match dir_path.read_dir() {
        Ok(entries) => entries
            .filter_map(|entry| match entry {
                Ok(entry) => Some(entry.path()),
                Err(e) => {
                    error!(
                        "Failed to read directory entry in {}: {}",
                        dir_path.display(),
                        e
                    );
                    None
                }
            })
            .collect(),
        Err(e) => {
            error!("Failed to read directory {}: {}", dir_path.display(), e);
            Vec::new()
        }
    }
}

pub fn is_python_file(file: &Path) -> bool {
    let b = file.extension().and_then(|ext| ext.to_str()) == Some("py");
    if !b {
        debug!("Not a python file {}", file.display());
    } else {
        debug!("Reading a python file {}", file.display());
    }
    b
}

impl fmt::Display for SqlString {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} = {}", self.variable_name, self.sql_content)
    }
}

impl fmt::Display for SqlExtract {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "{}", self.file_path)?;
        for sql_string in &self.strings {
            writeln!(f, "{}", sql_string)?;
        }
        Ok(())
    }
}

impl Default for SearchCtx {
    fn default() -> Self {
        Self {
            var_assign: true,
            multiple_var_assig: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_finder() -> SqlFinder {
        SqlFinder::new(FinderConfig {
            variables: vec!["query".to_string(), "sql".to_string()],
            min_sql_length: 1,
        })
    }

    fn test_python_code_with_expectations(
        code: &str,
        expected: Vec<(&str, &str)>,
        test_name: &str,
    ) {
        let parsed = ast::Suite::parse(code, "test.py").expect("Failed to parse");
        let finder = create_test_finder();
        let mut contexts = Vec::new();
        finder.analyze_stmts(&parsed, "test.py", &mut contexts);

        assert_eq!(
            contexts.len(),
            expected.len(),
            "{}: Expected {} SQL strings, found {}",
            test_name,
            expected.len(),
            contexts.len()
        );

        for (i, (expected_var, expected_sql)) in expected.iter().enumerate() {
            assert!(
                i < contexts.len(),
                "{}: Missing context at index {}",
                test_name,
                i
            );

            assert_eq!(
                contexts[i].variable_name, *expected_var,
                "{}: Expected variable '{}', found '{}'",
                test_name, expected_var, contexts[i].variable_name
            );

            assert_eq!(
                contexts[i].sql_content, *expected_sql,
                "{}: Expected SQL '{}', found '{}'",
                test_name, expected_sql, contexts[i].sql_content
            );
        }
    }

    #[test]
    fn test_simple_assignment() {
        test_python_code_with_expectations(
            r#"query = "SELECT id, name FROM users WHERE active = 1""#,
            vec![("query", "SELECT id, name FROM users WHERE active = 1")],
            "simple assignment",
        );
    }

    #[test]
    fn test_multiple_assignment() {
        test_python_code_with_expectations(
            r#"query = sql = "UPDATE users SET last_login = NOW()""#,
            vec![
                ("query", "UPDATE users SET last_login = NOW()"),
                ("sql", "UPDATE users SET last_login = NOW()"),
            ],
            "multiple assignment",
        );
    }

    #[test]
    fn test_chained_multiple_assignment() {
        test_python_code_with_expectations(
            r#"query = sql = query = "DELETE FROM sessions WHERE expires_at < NOW()""#,
            vec![
                ("query", "DELETE FROM sessions WHERE expires_at < NOW()"),
                ("sql", "DELETE FROM sessions WHERE expires_at < NOW()"),
                ("query", "DELETE FROM sessions WHERE expires_at < NOW()"),
            ],
            "chained multiple assignment",
        );
    }

    #[test]
    fn test_tuple_assignment() {
        test_python_code_with_expectations(
            r#"(query, sql) = ("SELECT * FROM users", "SELECT * FROM orders WHERE status = 'pending'")"#,
            vec![
                ("query", "SELECT * FROM users"),
                ("sql", "SELECT * FROM orders WHERE status = 'pending'"),
            ],
            "tuple assignment",
        );
    }

    #[test]
    fn test_list_assignment() {
        test_python_code_with_expectations(
            r#"[query, sql] = ["SELECT COUNT(*) FROM products", "INSERT INTO audit_log (action, timestamp) VALUES ('login', NOW())"]"#,
            vec![
                ("query", "SELECT COUNT(*) FROM products"),
                (
                    "sql",
                    "INSERT INTO audit_log (action, timestamp) VALUES ('login', NOW())",
                ),
            ],
            "list assignment",
        );
    }

    #[test]
    fn test_mixed_tuple_list() {
        test_python_code_with_expectations(
            r#"(query, sql) = ["SELECT * FROM cache WHERE key = ?", "UPDATE cache SET value = ?, updated_at = NOW() WHERE key = ?"]"#,
            vec![
                ("query", "SELECT * FROM cache WHERE key = ?"),
                (
                    "sql",
                    "UPDATE cache SET value = ?, updated_at = NOW() WHERE key = ?",
                ),
            ],
            "mixed tuple/list assignment",
        );
    }

    #[test]
    fn test_nested_tuple_assignment() {
        test_python_code_with_expectations(
            r#"((query, sql), query) = (("SELECT u.* FROM users u", "SELECT r.* FROM roles r"), "SELECT * FROM admins WHERE permissions LIKE '%super%'")"#,
            vec![
                ("query", "SELECT u.* FROM users u"),
                ("sql", "SELECT r.* FROM roles r"),
                (
                    "query",
                    "SELECT * FROM admins WHERE permissions LIKE '%super%'",
                ),
            ],
            "nested tuple assignment",
        );
    }

    #[test]
    fn test_deep_nested_assignment() {
        test_python_code_with_expectations(
            r#"(((query, sql), query), sql) = ((("SELECT 1", "SELECT 2"), "SELECT 3"), "SELECT 4")"#,
            vec![
                ("query", "SELECT 1"),
                ("sql", "SELECT 2"),
                ("query", "SELECT 3"),
                ("sql", "SELECT 4"),
            ],
            "deep nested assignment",
        );
    }

    #[test]
    fn test_attribute_assignment() {
        test_python_code_with_expectations(
            r#"database.query = "SELECT u.id, u.email, p.name FROM users u JOIN profiles p ON u.id = p.user_id""#,
            vec![(
                "query",
                "SELECT u.id, u.email, p.name FROM users u JOIN profiles p ON u.id = p.user_id",
            )],
            "attribute assignment",
        );
    }

    #[test]
    fn test_class_attribute_assignment() {
        test_python_code_with_expectations(
            r#"UserModel.sql = "SELECT id, created_at, updated_at FROM users""#,
            vec![("sql", "SELECT id, created_at, updated_at FROM users")],
            "class attribute assignment",
        );
    }

    #[test]
    fn test_nested_attribute_assignment() {
        test_python_code_with_expectations(
            r#"app.db.queries.sql = "SELECT * FROM users WHERE deleted_at IS NULL""#,
            vec![("sql", "SELECT * FROM users WHERE deleted_at IS NULL")],
            "nested attribute assignment",
        );
    }

    #[test]
    fn test_subscript_assignment() {
        test_python_code_with_expectations(
            r#"queries["query"] = "SELECT * FROM users WHERE username = ? OR email = ?""#,
            vec![], // Subscripts not currently handled
            "subscript assignment",
        );
    }

    #[test]
    fn test_starred_assignment_beginning() {
        test_python_code_with_expectations(
            r#"*rest, query = ["SELECT 1", "SELECT 2", "SELECT * FROM users ORDER BY created_at DESC"]"#,
            vec![("query", "SELECT * FROM users ORDER BY created_at DESC")],
            "starred assignment at beginning",
        );
    }

    #[test]
    fn test_starred_assignment_middle() {
        test_python_code_with_expectations(
            r#"query, *middle, sql = ["SELECT 1", "SELECT 2", "SELECT 3", "SELECT * FROM orders"]"#,
            vec![("query", "SELECT 1"), ("sql", "SELECT * FROM orders")],
            "starred assignment in middle",
        );
    }

    #[test]
    fn test_starred_assignment_end() {
        test_python_code_with_expectations(
            r#"query, *rest = ["SELECT u.*, COUNT(o.id) as order_count FROM users u LEFT JOIN orders o ON u.id = o.user_id GROUP BY u.id", "SELECT 1", "SELECT 2"]"#,
            vec![(
                "query",
                "SELECT u.*, COUNT(o.id) as order_count FROM users u LEFT JOIN orders o ON u.id = o.user_id GROUP BY u.id",
            )],
            "starred assignment at end",
        );
    }

    #[test]
    fn test_mixed_names_and_attributes() {
        test_python_code_with_expectations(
            r#"query, obj.sql = ("SELECT * FROM local_users", "SELECT * FROM remote_users WHERE sync_status = 'pending'")"#,
            vec![
                ("query", "SELECT * FROM local_users"),
                (
                    "sql",
                    "SELECT * FROM remote_users WHERE sync_status = 'pending'",
                ),
            ],
            "mixed names and attributes",
        );
    }

    #[test]
    fn test_mixed_starred_and_regular() {
        test_python_code_with_expectations(
            r#"query, *middle, sql = ("SELECT * FROM primary_table", "SELECT * FROM secondary1", "SELECT * FROM secondary2", "SELECT * FROM fallback_table")"#,
            vec![
                ("query", "SELECT * FROM primary_table"),
                ("sql", "SELECT * FROM fallback_table"),
            ],
            "mixed starred and regular",
        );
    }

    #[test]
    fn test_multiline_string_assignment() {
        let expected_sql = r#"
            SELECT 
                u.id,
                u.username,
                u.email,
                COUNT(o.id) as order_count,
                SUM(o.total) as total_spent
            FROM users u
            LEFT JOIN orders o ON u.id = o.user_id
            WHERE u.created_at >= '2023-01-01'
            GROUP BY u.id, u.username, u.email
            HAVING COUNT(o.id) > 0
            ORDER BY total_spent DESC
            LIMIT 100
            "#;

        test_python_code_with_expectations(
            &format!(r#"query = """{}""""#, expected_sql),
            vec![("query", expected_sql)],
            "multiline string assignment",
        );
    }

    #[test]
    fn test_single_quoted_sql() {
        test_python_code_with_expectations(
            r#"sql = 'SELECT * FROM products WHERE category = "electronics" AND price > 100'"#,
            vec![(
                "sql",
                r#"SELECT * FROM products WHERE category = "electronics" AND price > 100"#,
            )],
            "single quoted SQL",
        );
    }

    #[test]
    fn test_raw_string_with_escapes() {
        test_python_code_with_expectations(
            r#"query = r"SELECT * FROM logs WHERE message REGEXP '^Error.*\d{4}-\d{2}-\d{2}'""#,
            vec![(
                "query",
                r#"SELECT * FROM logs WHERE message REGEXP '^Error.*\d{4}-\d{2}-\d{2}'"#,
            )],
            "raw string with regex",
        );
    }

    #[test]
    fn test_complex_sql_statements() {
        test_python_code_with_expectations(
            r#"
                query = "INSERT INTO users (name, email, password_hash) VALUES (?, ?, ?)"
                sql = "UPDATE users SET last_login = CURRENT_TIMESTAMP WHERE id = ?"
                query = "DELETE FROM sessions WHERE expires_at < NOW() - INTERVAL '24 hours'"
                sql = "SELECT u.*, p.bio FROM users u LEFT JOIN profiles p ON u.id = p.user_id"
            "#,
            vec![
                (
                    "query",
                    "INSERT INTO users (name, email, password_hash) VALUES (?, ?, ?)",
                ),
                (
                    "sql",
                    "UPDATE users SET last_login = CURRENT_TIMESTAMP WHERE id = ?",
                ),
                (
                    "query",
                    "DELETE FROM sessions WHERE expires_at < NOW() - INTERVAL '24 hours'",
                ),
                (
                    "sql",
                    "SELECT u.*, p.bio FROM users u LEFT JOIN profiles p ON u.id = p.user_id",
                ),
            ],
            "complex SQL statements",
        );
    }

    #[test]
    fn test_sql_with_comments() {
        let expected_sql = r#"
            -- Get active users with their order counts
            SELECT 
                u.id,
                u.username,
                COUNT(o.id) as order_count
            FROM users u  -- Main users table
            LEFT JOIN orders o ON u.id = o.user_id  -- Join with orders
            WHERE u.status = 'active'  -- Only active users
            GROUP BY u.id, u.username
            "#;

        test_python_code_with_expectations(
            &format!(r#"sql = """{}""""#, expected_sql),
            vec![("sql", expected_sql)],
            "SQL with comments",
        );
    }

    #[test]
    fn test_stored_procedure_calls() {
        test_python_code_with_expectations(
            r#"query = "CALL get_user_analytics(?, ?, @result)""#,
            vec![("query", "CALL get_user_analytics(?, ?, @result)")],
            "stored procedure call",
        );
    }

    #[test]
    fn test_transaction_statements() {
        test_python_code_with_expectations(
            r#"
                query = "BEGIN TRANSACTION"
                sql = "COMMIT"
                query = "ROLLBACK"
            "#,
            vec![
                ("query", "BEGIN TRANSACTION"),
                ("sql", "COMMIT"),
                ("query", "ROLLBACK"),
            ],
            "transaction statements",
        );
    }

    #[test]
    fn test_ddl_statements() {
        test_python_code_with_expectations(
            r#"
                query = "CREATE TABLE temp_analytics (id INT PRIMARY KEY, data JSON)"
                sql = "ALTER TABLE users ADD COLUMN last_activity TIMESTAMP"
                query = "DROP TABLE IF EXISTS temp_results"
            "#,
            vec![
                (
                    "query",
                    "CREATE TABLE temp_analytics (id INT PRIMARY KEY, data JSON)",
                ),
                (
                    "sql",
                    "ALTER TABLE users ADD COLUMN last_activity TIMESTAMP",
                ),
                ("query", "DROP TABLE IF EXISTS temp_results"),
            ],
            "DDL statements",
        );
    }

    #[test]
    fn test_empty_sql_string() {
        test_python_code_with_expectations(r#"sql = """#, vec![("sql", "")], "empty SQL string");
    }

    #[test]
    fn test_annotation_assignment() {
        test_python_code_with_expectations(
            r#"query: str = "SELECT * FROM users WHERE age > 18""#,
            vec![("query", "SELECT * FROM users WHERE age > 18")],
            "annotated assignment",
        );
    }

    #[test]
    fn test_multiple_annotations() {
        test_python_code_with_expectations(
            r#"
            query: str = "SELECT * FROM products"
            sql: Optional[str] = "SELECT COUNT(*) FROM products"
            "#,
            vec![
                ("query", "SELECT * FROM products"),
                ("sql", "SELECT COUNT(*) FROM products"),
            ],
            "multiple annotated assignments",
        );
    }

    #[test]
    fn test_tuple_with_annotations() {
        test_python_code_with_expectations(
            r#"query: str, sql: str = ("SELECT * FROM cache", "UPDATE cache SET value = ?")"#,
            vec![
                ("query", "SELECT * FROM cache"),
                ("sql", "UPDATE cache SET value = ?"),
            ],
            "tuple assignment with annotations",
        );
    }

    #[test]
    fn test_class_method_assignment() {
        test_python_code_with_expectations(
            r#"
                class UserDAO:
                    def __init__(self):
                        self.query = "SELECT * FROM users"
                        self.sql = "INSERT INTO users (name, email) VALUES (?, ?)"
            "#,
            vec![
                ("query", "SELECT * FROM users"),
                ("sql", "INSERT INTO users (name, email) VALUES (?, ?)"),
            ],
            "class method assignments",
        );
    }

    #[test]
    fn test_function_local_assignment() {
        test_python_code_with_expectations(
            r#"
                def get_users():
                    query = "SELECT * FROM users"
                    sql = "SELECT * FROM users WHERE active = 1"
                    return query
            "#,
            vec![
                ("query", "SELECT * FROM users"),
                ("sql", "SELECT * FROM users WHERE active = 1"),
            ],
            "function local assignments",
        );
    }

    #[test]
    fn test_conditional_assignment() {
        test_python_code_with_expectations(
            r#"
                if condition:
                    query = "SELECT * FROM users WHERE role = 'admin'"
                else:
                    sql = "SELECT * FROM users WHERE role = 'user'"
            "#,
            vec![
                ("query", "SELECT * FROM users WHERE role = 'admin'"),
                ("sql", "SELECT * FROM users WHERE role = 'user'"),
            ],
            "conditional assignments",
        );
    }

    #[test]
    fn test_loop_assignment() {
        test_python_code_with_expectations(
            r#"
                for table in tables:
                    # This will be detected:
                    sql = "SELECT COUNT(*) FROM table_name"
            "#,
            vec![("sql", "SELECT COUNT(*) FROM table_name")],
            "loop assignments",
        );
    }

    #[test]
    fn test_exception_handling_assignment() {
        test_python_code_with_expectations(
            r#"
                try:
                    query = "SELECT * FROM users WHERE complex_condition = true"
                except Exception:
                    sql = "SELECT * FROM users LIMIT 10"
            "#,
            vec![
                (
                    "query",
                    "SELECT * FROM users WHERE complex_condition = true",
                ),
                ("sql", "SELECT * FROM users LIMIT 10"),
            ],
            "exception handling assignments",
        );
    }

    #[test]
    fn test_global_assignment() {
        test_python_code_with_expectations(
            r#"
                global query
                query = "SELECT * FROM global_config"
            "#,
            vec![("query", "SELECT * FROM global_config")],
            "global assignment",
        );
    }

    #[test]
    fn test_nonlocal_assignment() {
        test_python_code_with_expectations(
            r#"
                def outer():
                    def inner():
                        nonlocal sql
                        sql = "SELECT * FROM nested_table"
            "#,
            vec![("sql", "SELECT * FROM nested_table")],
            "nonlocal assignment",
        );
    }

    #[test]
    fn test_mixed_query_and_sql() {
        test_python_code_with_expectations(
            r#"
                query = "SELECT * FROM users"
                sql = "INSERT INTO logs (message) VALUES (?)"
                query = "UPDATE users SET active = 1"
                sql = "DELETE FROM temp_data"
            "#,
            vec![
                ("query", "SELECT * FROM users"),
                ("sql", "INSERT INTO logs (message) VALUES (?)"),
                ("query", "UPDATE users SET active = 1"),
                ("sql", "DELETE FROM temp_data"),
            ],
            "mixed query and sql variables",
        );
    }

    #[test]
    fn test_case_sensitive_patterns() {
        test_python_code_with_expectations(
            r#"
                QUERY = "SELECT * FROM users"
                SQL = "INSERT INTO logs VALUES (?)"
                Query = "UPDATE users SET status = 'active'"
                Sql = "DELETE FROM cache"
            "#,
            vec![
                ("QUERY", "SELECT * FROM users"),
                ("SQL", "INSERT INTO logs VALUES (?)"),
                ("Query", "UPDATE users SET status = 'active'"),
                ("Sql", "DELETE FROM cache"),
            ],
            "case variations of query/sql",
        );
    }

    #[test]
    fn test_complex_nesting_patterns() {
        test_python_code_with_expectations(
            r#"
                ((query, sql), (query, sql)) = (("SELECT 1", "SELECT 2"), ("SELECT 3", "SELECT 4"))
            "#,
            vec![
                ("query", "SELECT 1"),
                ("sql", "SELECT 2"),
                ("query", "SELECT 3"),
                ("sql", "SELECT 4"),
            ],
            "complex nested tuple patterns",
        );
    }

    #[test]
    fn test_asymmetric_tuple_assignment() {
        test_python_code_with_expectations(
            r#"
                (query, sql, query) = ("SELECT users", "SELECT orders")
            "#,
            vec![
                ("query", "SELECT users"),
                ("sql", "SELECT orders"), // Third query won't have a matching value, current implementation should handle gracefully
            ],
            "asymmetric tuple assignment",
        );
    }

    #[test]
    fn test_starred_with_sql_variables() {
        test_python_code_with_expectations(
            r#"
                query, *sql, query = ["SELECT 1", "SELECT 2", "SELECT 3", "SELECT 4", "SELECT 5"]
            "#,
            vec![
                ("query", "SELECT 1"),
                ("query", "SELECT 5"), // The starred *sql in the middle should get the middle values but may not be handled yet
            ],
            "starred assignment with sql variables",
        );
    }
}
