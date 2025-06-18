#[cfg(test)]
mod tests {
    use crate::*;
    use rustpython_parser::{
        Parse,
        ast::{self},
    };

    fn harness_create_test_finder() -> SqlFinder {
        SqlFinder::new(FinderConfig {
            variables: vec![
                "query".to_string(),
                "sql".to_string(),
                "also_query".to_string(),
            ],
            min_sql_length: 1,
        })
    }

    fn harness_find(code: &str, expected: Vec<(&str, &str)>, name: &str) {
        let parsed = ast::Suite::parse(code, "test.py").expect("Failed to parse");
        let finder = harness_create_test_finder();
        let mut contexts = Vec::new();
        finder.analyze_stmts(&parsed, &mut contexts);

        assert_eq!(
            contexts.len(),
            expected.len(),
            "{}: Expected {} SQL strings, found {}",
            name,
            expected.len(),
            contexts.len()
        );

        for (i, (expected_var, expected_sql)) in expected.iter().enumerate() {
            assert!(
                i < contexts.len(),
                "{}: Missing context at index {}",
                name,
                i
            );

            assert_eq!(
                contexts[i].variable_name, *expected_var,
                "{}: Expected variable '{}', found '{}'",
                name, expected_var, contexts[i].variable_name
            );

            assert_eq!(
                contexts[i].sql_content, *expected_sql,
                "{}: Expected SQL '{}', found '{}'",
                name, expected_sql, contexts[i].sql_content
            );
        }
    }

    #[test]
    fn simple_assignment() {
        harness_find(
            r#"query = "SELECT id, name FROM users WHERE active = 1""#,
            vec![("query", "SELECT id, name FROM users WHERE active = 1")],
            "simple assignment",
        );
    }

    #[test]
    fn multiple_assignment() {
        harness_find(
            r#"query = sql = "UPDATE users SET last_login = NOW()""#,
            vec![
                ("query", "UPDATE users SET last_login = NOW()"),
                ("sql", "UPDATE users SET last_login = NOW()"),
            ],
            "multiple assignment",
        );
    }

    #[test]
    fn chained_multiple_assignment() {
        harness_find(
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
    fn tuple_assignment() {
        harness_find(
            r#"(query, sql) = ("SELECT * FROM users", "SELECT * FROM orders WHERE status = 'pending'")"#,
            vec![
                ("query", "SELECT * FROM users"),
                ("sql", "SELECT * FROM orders WHERE status = 'pending'"),
            ],
            "tuple assignment",
        );
    }

    #[test]
    fn list_assignment() {
        harness_find(
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
    fn mixed_tuple_list() {
        harness_find(
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
    fn nested_tuple_assignment() {
        harness_find(
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
    fn deep_nested_assignment() {
        harness_find(
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
    fn attribute_assignment() {
        harness_find(
            r#"database.query = "SELECT u.id, u.email, p.name FROM users u JOIN profiles p ON u.id = p.user_id""#,
            vec![(
                "query",
                "SELECT u.id, u.email, p.name FROM users u JOIN profiles p ON u.id = p.user_id",
            )],
            "attribute assignment",
        );
    }

    #[test]
    fn class_attribute_assignment() {
        harness_find(
            r#"UserModel.sql = "SELECT id, created_at, updated_at FROM users""#,
            vec![("sql", "SELECT id, created_at, updated_at FROM users")],
            "class attribute assignment",
        );
    }

    #[test]
    fn nested_attribute_assignment() {
        harness_find(
            r#"app.db.queries.sql = "SELECT * FROM users WHERE deleted_at IS NULL""#,
            vec![("sql", "SELECT * FROM users WHERE deleted_at IS NULL")],
            "nested attribute assignment",
        );
    }

    #[test]
    fn subscript_assignment() {
        harness_find(
            r#"queries["query"] = "SELECT * FROM users WHERE username = ? OR email = ?""#,
            vec![], // Subscripts not currently handled
            "subscript assignment",
        );
    }

    #[test]
    fn starred_assignment_beginning() {
        harness_find(
            r#"*rest, query = ["SELECT 1", "SELECT 2", "SELECT * FROM users ORDER BY created_at DESC"]"#,
            vec![("query", "SELECT * FROM users ORDER BY created_at DESC")],
            "starred assignment at beginning",
        );
    }

    #[test]
    fn starred_assignment_middle() {
        harness_find(
            r#"query, *middle, sql = ["SELECT 1", "SELECT 2", "SELECT 3", "SELECT * FROM orders"]"#,
            vec![("query", "SELECT 1"), ("sql", "SELECT * FROM orders")],
            "starred assignment in middle",
        );
    }

    #[test]
    fn starred_assignment_end() {
        harness_find(
            r#"query, *rest = ["SELECT u.*, COUNT(o.id) as order_count FROM users u LEFT JOIN orders o ON u.id = o.user_id GROUP BY u.id", "SELECT 1", "SELECT 2"]"#,
            vec![(
                "query",
                "SELECT u.*, COUNT(o.id) as order_count FROM users u LEFT JOIN orders o ON u.id = o.user_id GROUP BY u.id",
            )],
            "starred assignment at end",
        );
    }

    #[test]
    fn mixed_names_and_attributes() {
        harness_find(
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
    fn mixed_starred_and_regular() {
        harness_find(
            r#"query, *middle, sql = ("SELECT * FROM primary_table", "SELECT * FROM secondary1", "SELECT * FROM secondary2", "SELECT * FROM fallback_table")"#,
            vec![
                ("query", "SELECT * FROM primary_table"),
                ("sql", "SELECT * FROM fallback_table"),
            ],
            "mixed starred and regular",
        );
    }

    #[test]
    fn multiline_string_assignment() {
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

        harness_find(
            &format!(r#"query = """{}""""#, expected_sql),
            vec![("query", expected_sql)],
            "multiline string assignment",
        );
    }

    #[test]
    fn single_quoted_sql() {
        harness_find(
            r#"sql = 'SELECT * FROM products WHERE category = "electronics" AND price > 100'"#,
            vec![(
                "sql",
                r#"SELECT * FROM products WHERE category = "electronics" AND price > 100"#,
            )],
            "single quoted SQL",
        );
    }

    #[test]
    fn raw_string_with_escapes() {
        harness_find(
            r#"query = r"SELECT * FROM logs WHERE message REGEXP '^Error.*\d{4}-\d{2}-\d{2}'""#,
            vec![(
                "query",
                r#"SELECT * FROM logs WHERE message REGEXP '^Error.*\d{4}-\d{2}-\d{2}'"#,
            )],
            "raw string with regex",
        );
    }

    #[test]
    fn sql_with_comments() {
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

        harness_find(
            &format!(r#"sql = """{}""""#, expected_sql),
            vec![("sql", expected_sql)],
            "SQL with comments",
        );
    }

    #[test]
    fn stored_procedure_calls() {
        harness_find(
            r#"query = "CALL get_user_analytics(?, ?, @result)""#,
            vec![("query", "CALL get_user_analytics(?, ?, @result)")],
            "stored procedure call",
        );
    }

    #[test]
    fn ddl_statements() {
        harness_find(
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
    fn empty_sql_string() {
        harness_find(r#"sql = """#, vec![("sql", "")], "empty SQL string");
    }

    #[test]
    fn annotation_assignment() {
        harness_find(
            r#"query: str = "SELECT * FROM users WHERE age > 18""#,
            vec![("query", "SELECT * FROM users WHERE age > 18")],
            "annotated assignment",
        );
    }

    #[test]
    fn class_method_assignment() {
        harness_find(
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
    fn function_local_assignment() {
        harness_find(
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
    fn conditional_assignment() {
        harness_find(
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
    fn loop_assignment() {
        harness_find(
            r#"
for table in tables:
    sql = "SELECT COUNT(*) FROM table_name"
            "#,
            vec![("sql", "SELECT COUNT(*) FROM table_name")],
            "loop assignments",
        );
    }

    #[test]
    fn exception_handling_assignment() {
        harness_find(
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
    fn global_assignment() {
        harness_find(
            r#"
global query
query = "SELECT * FROM global_config"
            "#,
            vec![("query", "SELECT * FROM global_config")],
            "global assignment",
        );
    }

    #[test]
    fn mixed_query_and_sql() {
        harness_find(
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
    fn case_sensitive_patterns() {
        harness_find(
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
    fn complex_nesting_patterns() {
        harness_find(
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
    fn f_string_simple() {
        harness_find(
            r#"
table = "users"
query = f"select * from {table}"
            "#,
            vec![("query", "select * from {PLACEHOLDER}")],
            "f-string simple variable substitution",
        );
    }

    #[test]
    fn f_string_multiple_vars() {
        harness_find(
            r#"
table = "users"
status = "active"
query = f"select * from {table} where status = '{status}'"
            "#,
            vec![(
                "query",
                "select * from {PLACEHOLDER} where status = '{PLACEHOLDER}'",
            )],
            "f-string multiple variables substitution",
        );
    }

    #[test]
    fn f_string_with_numbers() {
        harness_find(
            r#"
table = "products"
min_price = 100
query = f"select * from {table} where price > {min_price}"
            "#,
            vec![(
                "query",
                "select * from {PLACEHOLDER} where price > {PLACEHOLDER}",
            )],
            "f-string with number substitution",
        );
    }

    #[test]
    fn percent_formatting_positional() {
        harness_find(
            r#"
query = "select * from %s where id = %d" % ("users", 123)
            "#,
            vec![("query", "select * from users where id = 123")],
            "percent formatting positional substitution",
        );
    }

    #[test]
    fn percent_formatting_named() {
        harness_find(
            r#"
query = "select * from %(table)s where status = '%(status)s'" % {"table": "users", "status": "active"}
            "#,
            vec![("query", "select * from users where status = 'active'")],
            "percent formatting named substitution",
        );
    }

    #[test]
    fn format_method_positional() {
        harness_find(
            r#"
query = "select * from {} where status = '{}'".format("users", "active")
            "#,
            vec![("query", "select * from users where status = 'active'")],
            "format method positional substitution",
        );
    }

    #[test]
    fn format_method_named() {
        harness_find(
            r#"
query = "select * from {table} where status = '{status}'".format(table="users", status="active")
            "#,
            vec![("query", "select * from users where status = 'active'")],
            "format method named substitution",
        );
    }

    #[test]
    fn format_method_numbered() {
        harness_find(
            r#"
query = "select * from {0} where id = {1}".format("users", 123)
            "#,
            vec![("query", "select * from users where id = 123")],
            "format method numbered substitution",
        );
    }

    #[test]
    fn multiline_f_string() {
        harness_find(
            r#"
table = "users"
status = "active"
query = f"""
    select 
        id,
        name,
        email
    from {table}
    where status = '{status}'
"""
            "#,
            vec![(
                "query",
                "\n    select \n        id,\n        name,\n        email\n    from {PLACEHOLDER}\n    where status = '{PLACEHOLDER}'\n",
            )],
            "multiline f-string substitution",
        );
    }

    #[test]
    fn complex_format_with_join() {
        harness_find(
            r#"
columns = ["id", "name", "email"]
table = "users"
query = "select {} from {}".format(", ".join(columns), table)
            "#,
            vec![("query", "select id, name, email from users")],
            "format with join operation substitution",
        );
    }

    #[test]
    fn nested_f_string_expressions() {
        harness_find(
            r#"
query = f"select * from {'table.' + 's'} where id > 5"
            "#,
            vec![("query", "select * from table.s where id > 5")],
            "f-string with expression evaluation",
        );
    }

    #[test]
    fn format_with_dictionary_unpacking() {
        harness_find(
            r#"
params = {"table": "orders", "status": "pending", "limit": 50}
query = "select * from {table} where status = '{status}' limit {limit}".format(**params)
            "#,
            vec![(
                "query",
                "select * from orders where status = 'pending' limit 50",
            )],
            "format with dictionary unpacking substitution",
        );
    }

    #[test]
    fn percent_with_mixed_types() {
        harness_find(
            r#"
query = "select * from %s where price > %.2f and quantity = %d" % ("products", 99.99, 10)
            "#,
            vec![(
                "query",
                "select * from products where price > 99.99 and quantity = 10",
            )],
            "percent formatting mixed types substitution",
        );
    }

    #[test]
    fn f_string_with_method_calls() {
        harness_find(
            r#"
table_name = "UsErS"
query = f"select * from {table_name.lower()}"
            "#,
            vec![("query", "select * from {PLACEHOLDER}")],
            "f-string with method call substitution",
        );
    }

    #[test]
    fn format_with_list_indexing() {
        harness_find(
            r#"
tables = ["users", "orders", "products"]
query = "select * from {} join {} on users.id = orders.user_id".format(tables[0], tables[1])
            "#,
            vec![(
                "query",
                "select * from {PLACEHOLDER} join {PLACEHOLDER} on users.id = orders.user_id",
            )],
            "format with list indexing substitution",
        );
    }

    #[test]
    fn f_string_with_dictionary_access() {
        harness_find(
            r#"
config = {"table": "customers", "limit": 100}
query = f"select * from {config['table']} limit {config['limit']}"
            "#,
            vec![("query", "select * from customers limit 100")],
            "f-string with dictionary access substitution",
        );
    }

    #[test]
    fn format_with_string_operations() {
        harness_find(
            r#"
prefix = "temp_"
table = "users"
query = "select * from {}".format(prefix + table)
            "#,
            vec![("query", "select * from {PLACEHOLDER}")],
            "format with string concatenation substitution",
        );
    }

    #[test]
    fn conditional_f_string() {
        harness_find(
            r#"
include_deleted = False
table_suffix = "_all" if include_deleted else ""
query = f"select * from users{table_suffix}"
            "#,
            vec![("query", "select * from users{PLACEHOLDER}")],
            "f-string with conditional substitution",
        );
    }

    #[test]
    fn format_with_arithmetic() {
        harness_find(
            r#"
query = "select * from users limit {}".format(20 * 5)
            "#,
            vec![("query", "select * from users limit 100")],
            "format with arithmetic substitution",
        );
    }
}
