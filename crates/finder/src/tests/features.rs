#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use crate::*;
    use rustpython_parser::{
        Parse,
        ast::{self},
    };

    fn harness_create_test_finder() -> SqlFinder {
        SqlFinder::new(FinderConfig {
            variables: HashSet::from_iter([
                "query".to_string(),
                "sql".to_string(),
                "also_query".to_string(),
            ]),
            min_sql_length: 1,

            func_names: HashSet::from_iter([
                "query_fun".to_string(),
                "sql_fun".to_string(),
                "also_query_fun".to_string(),
            ]),
        })
    }

    fn harness_find(code: &str, expected: Vec<(&str, &str)>, name: &str) {
        let parsed = ast::Suite::parse(code, "test.py").expect("Failed to parse");
        let finder = harness_create_test_finder();
        let contexts = finder.analyze_stmts(&parsed);

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
            vec![("query", "select {PLACEHOLDER} from {PLACEHOLDER}")],
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
                "select * from {PLACEHOLDER} where status = '{PLACEHOLDER}' limit {PLACEHOLDER}",
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
            vec![("query", "select * from {PLACEHOLDER} limit {PLACEHOLDER}")],
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
    #[test]
    fn simple_function_call() {
        harness_find(
            r#"execute("SELECT * FROM users WHERE active = 1")"#,
            vec![("execute", "SELECT * FROM users WHERE active = 1")],
            "simple function call",
        );
    }

    #[test]
    fn method_call_on_object() {
        harness_find(
            r#"db.query_fun("SELECT id, name FROM products ORDER BY name")"#,
            vec![("query_fun", "SELECT id, name FROM products ORDER BY name")],
            "method call on object",
        );
    }

    #[test]
    fn chained_method_calls() {
        harness_find(
            r#"database.connection.sql_fun("UPDATE users SET last_login = NOW()")"#,
            vec![("sql_fun", "UPDATE users SET last_login = NOW()")],
            "chained method calls",
        );
    }

    #[test]
    fn function_call_with_multiple_args() {
        harness_find(
            r#"execute_query("SELECT * FROM orders WHERE date > ?", "2023-01-01")"#,
            vec![], // execute_query not in func_names
            "function call with multiple args",
        );
    }

    #[test]
    fn function_call_with_kwargs() {
        harness_find(
            r#"query_fun(sql="SELECT * FROM users", timeout=30)"#,
            vec![("query_fun", "SELECT * FROM users")],
            "function call with keyword arguments",
        );
    }

    #[test]
    fn nested_function_calls() {
        harness_find(
            r#"outer_func(sql_fun("SELECT COUNT(*) FROM products"))"#,
            vec![("sql_fun", "SELECT COUNT(*) FROM products")],
            "nested function calls",
        );
    }

    #[test]
    fn function_call_in_expression() {
        harness_find(
            r#"result = query_fun("SELECT * FROM cache") or default_query()"#,
            vec![("query_fun", "SELECT * FROM cache")],
            "function call in boolean expression",
        );
    }

    #[test]
    fn function_call_with_f_string() {
        harness_find(
            r#"
table = "users"
sql_fun(f"SELECT * FROM {table} WHERE active = 1")
        "#,
            vec![("sql_fun", "SELECT * FROM {PLACEHOLDER} WHERE active = 1")],
            "function call with f-string",
        );
    }

    #[test]
    fn function_call_with_format_method() {
        harness_find(
            r#"query_fun("SELECT * FROM {} WHERE status = '{}'".format("orders", "pending"))"#,
            vec![("query_fun", "SELECT * FROM orders WHERE status = 'pending'")],
            "function call with format method",
        );
    }

    #[test]
    fn function_call_with_percent_formatting() {
        harness_find(
            r#"also_query_fun("SELECT * FROM %s WHERE id = %d" % ("products", 123))"#,
            vec![("also_query_fun", "SELECT * FROM products WHERE id = 123")],
            "function call with percent formatting",
        );
    }

    #[test]
    fn function_call_with_multiline_sql() {
        harness_find(
            r#"
sql_fun("""
    SELECT 
        u.id,
        u.name,
        COUNT(o.id) as order_count
    FROM users u
    LEFT JOIN orders o ON u.id = o.user_id
    GROUP BY u.id, u.name
""")
        "#,
            vec![(
                "sql_fun",
                "\n    SELECT \n        u.id,\n        u.name,\n        COUNT(o.id) as order_count\n    FROM users u\n    LEFT JOIN orders o ON u.id = o.user_id\n    GROUP BY u.id, u.name\n",
            )],
            "function call with multiline SQL",
        );
    }

    #[test]
    fn multiple_function_calls_same_line() {
        harness_find(
            r#"query_fun("SELECT 1"); sql_fun("SELECT 2")"#,
            vec![("query_fun", "SELECT 1"), ("sql_fun", "SELECT 2")],
            "multiple function calls same line",
        );
    }

    #[test]
    fn function_call_in_conditional() {
        harness_find(
            r#"
if condition:
    query_fun("SELECT * FROM users WHERE role = 'admin'")
else:
    sql_fun("SELECT * FROM users WHERE role = 'user'")
        "#,
            vec![
                ("query_fun", "SELECT * FROM users WHERE role = 'admin'"),
                ("sql_fun", "SELECT * FROM users WHERE role = 'user'"),
            ],
            "function calls in conditional",
        );
    }

    #[test]
    fn function_call_in_loop() {
        harness_find(
            r#"
for table in tables:
    also_query_fun(f"SELECT COUNT(*) FROM {table}")
        "#,
            vec![("also_query_fun", "SELECT COUNT(*) FROM {PLACEHOLDER}")],
            "function call in loop",
        );
    }

    #[test]
    fn function_call_in_try_except() {
        harness_find(
            r#"
try:
    query_fun("SELECT * FROM risky_table WHERE complex_join = true")
except Exception:
    sql_fun("SELECT * FROM fallback_table")
        "#,
            vec![
                (
                    "query_fun",
                    "SELECT * FROM risky_table WHERE complex_join = true",
                ),
                ("sql_fun", "SELECT * FROM fallback_table"),
            ],
            "function calls in try/except",
        );
    }

    #[test]
    fn function_call_with_variable_argument() {
        harness_find(
            r#"
user_query = "SELECT * FROM users WHERE id = ?"
sql_fun(user_query)
        "#,
            vec![("sql_fun", "{PLACEHOLDER}")],
            "function call with variable argument",
        );
    }

    #[test]
    fn function_call_with_list_comprehension() {
        harness_find(
            r#"query_fun("SELECT id FROM users WHERE id IN ({})".format(",".join([str(i) for i in range(5)])))"#,
            vec![(
                "query_fun",
                "SELECT id FROM users WHERE id IN ({PLACEHOLDER})",
            )],
            "function call with list comprehension",
        );
    }

    #[test]
    fn function_call_return_statement() {
        harness_find(
            r#"
def get_user_query():
    return query_fun("SELECT * FROM users WHERE active = 1")
        "#,
            vec![("query_fun", "SELECT * FROM users WHERE active = 1")],
            "function call in return statement",
        );
    }

    #[test]
    fn lambda_with_function_call() {
        harness_find(
            r#"callback = lambda: sql_fun("SELECT * FROM temp_data")"#,
            vec![("sql_fun", "SELECT * FROM temp_data")],
            "function call in lambda",
        );
    }

    #[test]
    fn function_call_as_default_argument() {
        harness_find(
            r#"
def process_data(query=query_fun("SELECT * FROM default_table")):
    pass
        "#,
            vec![("query_fun", "SELECT * FROM default_table")],
            "function call as default argument",
        );
    }

    #[test]
    fn function_call_in_list_context() {
        harness_find(
            r#"queries = [query_fun("SELECT 1"), sql_fun("SELECT 2"), also_query_fun("SELECT 3")]"#,
            vec![
                ("query_fun", "SELECT 1"),
                ("sql_fun", "SELECT 2"),
                ("also_query_fun", "SELECT 3"),
            ],
            "function calls in list context",
        );
    }

    #[test]
    fn function_call_in_dict_context() {
        harness_find(
            r#"
queries = {
    "users": query_fun("SELECT * FROM users"),
    "orders": sql_fun("SELECT * FROM orders")
}
        "#,
            vec![
                ("query_fun", "SELECT * FROM users"),
                ("sql_fun", "SELECT * FROM orders"),
            ],
            "function calls in dictionary context",
        );
    }

    #[test]
    fn function_call_with_string_concatenation() {
        harness_find(
            r#"query_fun("SELECT * FROM " + "users" + " WHERE active = 1")"#,
            vec![("query_fun", "SELECT * FROM users WHERE active = 1")],
            "function call with string concatenation",
        );
    }

    #[test]
    fn chained_calls_different_functions() {
        harness_find(
            r#"
query_fun("SELECT id FROM users").sql_fun("UPDATE users SET active = 1")
        "#,
            vec![
                ("query_fun", "SELECT id FROM users"),
                ("sql_fun", "UPDATE users SET active = 1"),
            ],
            "chained calls with different function names",
        );
    }

    #[test]
    fn function_call_with_unpacked_args() {
        harness_find(
            r#"
args = ["SELECT * FROM dynamic_table"]
query_fun(*args)
        "#,
            vec![("query_fun", "{PLACEHOLDER}")],
            "function call with unpacked arguments",
        );
    }

    #[test]
    fn function_call_with_unpacked_kwargs() {
        harness_find(
            r#"
kwargs = {"sql": "SELECT * FROM users", "timeout": 30}
sql_fun(**kwargs)
        "#,
            vec![("sql_fun", "{PLACEHOLDER}")],
            "function call with unpacked keyword arguments",
        );
    }
}
