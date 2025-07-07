"""
Test file for SQL linter - contains various patterns of SQL strings
mini
"""

# import sqlite3
# import psycopg2

# Simple variable assignments (should be detected)
query = "SELECT * FROM users WHERE id = 1"
sql = "INSERT INTO products (name, price) VALUES ('laptop', 999.99)"
statement = "UPDATE users SET last_login = NOW() WHERE id = ?"
cmd = "DELETE FROM sessions WHERE expires_at < NOW()"

# Multi Assignments
not_query, is_query, not_even_query = "Foo", "select * from table where apples == 'green'", "Baz"

# Variables with non-SQL content (should be ignored)
message = "Hello world"
config_path = "/etc/myapp/config.json"
regular_string = "This is just a regular string"

# Function calls with SQL (should be detected)
# def get_user_data():
#     cursor.execute("SELECT name, email FROM users WHERE active = 1")
#     return cursor.fetchall()

# def create_user(name, email):
#     db.query("INSERT INTO users (name, email) VALUES (?, ?)", (name, email))

# Method calls on database objects
# conn = sqlite3.connect('test.db')
# cursor = conn.cursor()
# cursor.execute("CREATE TABLE IF NOT EXISTS users (id INTEGER PRIMARY KEY, name TEXT)")
# cursor.executemany("INSERT INTO users (name) VALUES (?)", [("Alice",), ("Bob",)])

# Dictionary with SQL queries
# queries = {
#     "get_all": "SELECT * FROM users",
#     "get_active": "SELECT * FROM users WHERE active = 1",
#     "count_users": "SELECT COUNT(*) FROM users"
# }

# Multi-line SQL strings
_sql = """
    SELECT u.name, u.email, p.title
    FROM users u
    LEFT JOIN posts p ON u.id = p.user_id
    WHERE u.created_at > '2023-01-01'
    its apsten adskflnasdfnlk
    ORDER BY u.name
"""

# SQL with syntax errors (should be caught by linter)
bad_query = "SELCT * FROM users"  # typo in SELECT
invalid_sql = "INSERT INTO users (name VALUES ('test')"  # missing closing paren

# F-strings and formatted SQL (tricky cases)
user_id = 123
dynamic_query = f"SELECT * FROM users WHERE id = {user_id}"
format_query = "SELECT * FROM {} WHERE status = %s".format("orders")

# SQL-like strings that aren't actually SQL
log_message = "User SELECT operation completed"
documentation = "The SELECT statement retrieves data from tables"

# Class with SQL methods
class UserRepository:
    def __init__(self, connection):
        self.conn = connection
    
    def find_by_id(self, user_id):
        sql = "SELECT * FROM users WHERE id = ?"
        # return self.conn.execute(sql, (user_id,)).fetchone()
    
    def create_user(self, name, email):
        query = "INSERT INTO users (name, email) VALUES (?, ?)"
        # self.conn.execute(query, (name, email))

# Function parameters that are SQL
# def execute_query(sql, params=None):
#     cursor.execute(sql, params or ())

# def run_statement(statement):
#     return db.query(statement)

# Edge cases
empty_query = ""
none_query = None
sql_comment = "-- This is a SQL comment SELECT * FROM users"

# Raw strings
raw_sql = r"SELECT * FROM users WHERE name LIKE '%\_%'"

# Concatenated SQL (potential injection risk)
table_name = "users"
dangerous_query = "SELECT * FROM " + table_name  # Should ideally be flagged
TESTdangerous_queryQSS = "SELECT * FROM iks[] " + table_name  # Should ideally be flagged

# if __name__ == "__main__":
#     # More SQL in main execution
#     cursor.execute("SELECT COUNT(*) FROM users")
#     result = cursor.fetchone()
#     print(f"Total users: {result[0]}")
