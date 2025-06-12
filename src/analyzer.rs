use sqlparser::ast::Statement;
use sqlparser::dialect::{GenericDialect, PostgreSqlDialect, SQLiteDialect};
use sqlparser::parser::Parser;

use crate::finder::{SqlExtract, SqlString};
use crate::{debug, error, info};

#[derive(Debug, Clone)]
pub enum StatementType {
    Select,
    Insert,
    Update,
    Delete,
    Create,
    Drop,
    Other(String),
}

#[derive(Debug, Clone)]
pub enum SqlDialect {
    Generic,
    PostgreSQL,
    SQLite,
}

pub struct SqlAnalyzer {
    dialect: SqlDialect,
}

impl SqlAnalyzer {
    pub fn new(dialect: SqlDialect) -> Self {
        Self { dialect }
    }

    pub fn analyze_sql_extract(&self, extract: &SqlExtract) {
        if extract.strings.is_empty() {
            debug!("Empty extract `{}`", extract.file_path)
        }

        extract
            .strings
            .iter()
            .map(|sql_string| self.analyze_sql_string(sql_string))
            .collect()
    }

    fn analyze_sql_string(&self, sql_string: &SqlString) {
        let filled_sql = fill_placeholders(&sql_string.sql_content);

        match self.parse_sql(&filled_sql) {
            Ok(_) => {
                info!("Valid sql string: {}", sql_string.sql_content)
            }
            Err(e) => {
                error!("Invalid sql string: {}", sql_string.sql_content)
            }
        }
    }

    fn parse_sql(&self, sql: &str) -> Result<Vec<Statement>, sqlparser::parser::ParserError> {
        let dialect: Box<dyn sqlparser::dialect::Dialect> = match self.dialect {
            SqlDialect::Generic => Box::new(GenericDialect {}),
            SqlDialect::PostgreSQL => Box::new(PostgreSqlDialect {}),
            SqlDialect::SQLite => Box::new(SQLiteDialect {}),
        };

        Parser::parse_sql(&*dialect, sql)
    }
}

/// Prepare SQL for parsing by replacing placeholders with dummy values
fn fill_placeholders(sql: &str) -> String {
    sql.replace('?', "'PLACEHOLDER'")
        .replace(":1", "'PLACEHOLDER'")
        .replace(":2", "'PLACEHOLDER'")
}
