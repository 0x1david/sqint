# Sqint

**A fast SQL linter for Python codebases**

Sqint is a specialized linter that analyzes Python code to find and validate SQL strings embedded in your application. By walking the Python AST and examining variable names, function calls, and class methods, Sqint catches SQL syntax errors before they reach production.

## Features

- 🔍 **Smart SQL Detection** - Finds SQL strings in variables, function calls, and class methods based on configurable patterns
- 🗃️ **Multi-dialect Support** - Supports PostgreSQL, Oracle, SQLite and many other dialects, plus configurable mappings
- ⚙️ **Flexible Configuration** - Configure through `sqint.toml` or `pyproject.toml`
- 🚀 **Built for Speed** - Written in Rust for fast analysis across large codebases
- 📦 **Easy Installation** - Installable via pip with no external dependencies
- 🔧 **Git Integration** - Incremental mode to analyze only changed files
- 🌐 **Monorepo Friendly** - Respects .gitignore and supports flexible file patterns

Sqint helps you catch SQL syntax errors early in development, preventing runtime database errors and improving code reliability.

## Table of Contents

- [Getting Started](#getting-started)
- [Installation](#installation)
- [Usage](#usage)
- [Configuration](#configuration)
- [SQL Dialect Support](#sql-dialect-support)
- [Contributing](#contributing)
- [License](#license)

## Getting Started

### Installation

Sqint is available as `sqint` on PyPI:

```bash
# With pip
pip install sqint

# With pipx
pipx install sqint
```

Starting with version 0.1.0, Sqint can be installed with standalone installers:

```bash
# On macOS and Linux
curl -LsSf https://astral.sh/sqint/install.sh | sh

# On Windows
powershell -c "irm https://astral.sh/sqint/install.ps1 | iex"
```

### Usage

To run Sqint on your Python code:

```bash
sqint                             # Check all Python files in current directory
sqint path/to/code/               # Check all Python files in specific directory
sqint path/to/file.py             # Check specific file
sqint --exclude "test_*.py"       # Exclude test files
sqint --errors-only               # Show only errors, not warnings
```

Initialize a configuration file:

```bash
sqint init                        # Create sqint.toml in current directory
sqint init --output config.toml   # Create config file with custom name
```

### Example

Given this Python code:

```python
# This will be detected and validated
user_query = "SELECT * FROM users WHERE id = ?"
cursor.execute(user_query, (user_id,))

# This will trigger a syntax error
bad_sql = "SELECT * FROM users WHERE"
db.fetchall(bad_sql)
```

Sqint will:
1. Find SQL strings in variables matching patterns like `*query*`, `*sql*`
2. Analyze function calls to methods like `execute()`, `fetchall()`
3. Check keyword parameters like `query=`, `sql=`
4. Validate the SQL syntax and report any errors

## Configuration

Sqint can be configured through a `sqint.toml` file or within your `pyproject.toml` file.

### Basic Configuration

Create a `sqint.toml` file in your project root:

```toml
# Variable names to look for and analyze
variable_contexts = [
    "*query*",
    "*sql*", 
    "*stmt*",
]

# Function names with arguments to validate
function_contexts = [
    "fetchall",
    "execute", 
    "fetchone",
    "select",
    "fetch_records"
]

# Directories to analyze
targets = ["."]

# File patterns to include
file_patterns = [
    "*.py",
    "*.pyi", 
    "*.ipynb",
]
```

### Advanced Configuration

```toml
# File patterns to exclude
exclude_patterns = ["*_test.py", "migrations/*"]

# Respect .gitignore files
respect_gitignore = true

# Log level: "trace", "debug", "info", "warn", "error", "bail"
loglevel = "error"

# Incremental mode - only analyze changed files
incremental_mode = true
baseline_branch = "main"
include_staged = true

# Performance tuning
parallel_processing = true
max_threads = 0  # Auto-detect based on CPU cores
thread_chunk_size = 1

# SQL parameter placeholders
param_markers = ["?", "%s", "%(name)s"]

# Dialect-specific mappings
[dialect_mappings]
"NOTNULL" = "NOT NULL"
"ISNULL" = "IS NULL"
```

### pyproject.toml Configuration

You can also configure Sqint in your `pyproject.toml`:

```toml
[tool.sqint]
variable_contexts = ["*query*", "*sql*"]
function_contexts = ["execute", "fetchall"]
targets = ["."]
file_patterns = ["*.py"]
min_sql_length = 10

[tool.sqint.dialect_mappings]
"NOTNULL" = "NOT NULL"
```

## SQL Dialect Support

Sqint supports multiple SQL dialects:

- **Generic** - Recommended as a baseline for multi-dialect codebases
- **PostgreSQL**
- **Oracle**
- **SQLite**
- **Ansi**
- **BigQuery**
- **ClickHouse**
- **DuckDb**
- **Hive**
- **MsSql**
- **MySql**
- **RedshiftSql**
- **Snowflake**

### Multi-dialect Projects

For codebases that use multiple SQL dialects, I recommend to use the Generic dialect with parameter markers and dialect mappings:

```toml
# Use generic dialect
dialect = "generic"

# Handle different parameter styles
param_markers = ["?", "%s", "%(name)s", ":param"]

# Map dialect differences
[dialect_mappings]
"NOTNULL" = "NOT NULL"
"ISNULL" = "IS NULL"
"LIMIT 1" = "ROWNUM = 1"  # Oracle-style
```

## Detection Patterns

Sqint finds SQL strings using several configurable patterns:

### Variable Names
```python
# Matches variable_contexts = ["*query*", "*sql*"]
user_query = "SELECT * FROM users"
sql_statement = "INSERT INTO logs VALUES (?, ?)"
update_stmt = "UPDATE users SET active = 1"
```

### Function Calls
```python
# Matches function_contexts = ["execute", "fetchall"]
cursor.execute("SELECT * FROM products")
db.fetchall("SELECT name FROM categories")
```

## Command Line Options

```bash
# Basic usage
sqint [PATH]                    # Check files/directories
sqint --config custom.toml      # Use custom config file
sqint --exclude "test_*.py"     # Exclude patterns
sqint --errors-only            # Show only errors
sqint --max-issues 10          # Limit reported issues
sqint --fail-on-issues         # Exit with error code if issues found

# Output formats
sqint --format colored         # Colored terminal output (default)
sqint --format plain          # Plain text output

# Debugging
sqint --debug                 # Enable debug output (Mostly intended for development)
sqint --loglevel error        # Set log level
```

## Examples

### Basic SQL Validation

```python
# ✓ Valid SQL - will pass
query = "SELECT id, name FROM users WHERE active = 1"
cursor.execute(query)

# ✗ Invalid SQL - will be caught
broken_query = "SELECT * FROM users WHERE"
cursor.execute(broken_query)
```

### Parameterized Queries

```python
# ✓ Valid with parameter markers
user_query = "SELECT * FROM users WHERE id = ? AND status = ?"
cursor.execute(user_query, (user_id, 'active'))

# ✓ Named parameters
update_sql = "UPDATE users SET name = %(name)s WHERE id = %(id)s"
cursor.execute(update_sql, {'name': 'John', 'id': 123})
```

### Complex Queries

```python
# ✓ Multi-line SQL strings
complex_query = """
    SELECT u.id, u.name, p.title
    FROM users u
    LEFT JOIN posts p ON u.id = p.user_id
    WHERE u.active = 1
    ORDER BY u.created_at DESC
"""
results = db.fetchall(complex_query)
```

## Integration

### Pre-commit Hook

Add Sqint to your `.pre-commit-config.yaml`:

```yaml
repos:
  - repo: https://github.com/your-org/sqint-pre-commit
    rev: v0.1.0
    hooks:
      - id: sqint
        args: [--errors-only]
```

### GitHub Actions

```yaml
name: SQL Linting
on: [push, pull_request]
jobs:
  sqint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install Sqint
        run: pip install sqint
      - name: Run Sqint
        run: sqint --fail-on-issues
```

### VS Code Integration

While there's no official VS Code extension yet, you can run Sqint from the integrated terminal or set up a task:

```json
{
    "version": "2.0.0",
    "tasks": [
        {
            "label": "Run Sqint",
            "type": "shell",
            "command": "sqint",
            "args": ["--errors-only"],
            "group": "build"
        }
    ]
}
```

## License

Sqint is released under the MIT OR Apache-2.0 license.

## Acknowledgements

Sqint is built with:
- [sqlparser](https://github.com/sqlparser-rs/sqlparser-rs) for SQL parsing
- [rustpython-parser](https://github.com/RustPython/RustPython) for parsing Python files to AST
- [clap](https://github.com/clap-rs/clap) for command-line interface

I'm grateful to the maintainers of these excellent tools and the value they provide to the Rust ecosystem.
