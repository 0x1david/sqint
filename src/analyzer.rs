use std::error::Error;

use sqlparser::ast::Statement;
use sqlparser::dialect::{GenericDialect, PostgreSqlDialect, SQLiteDialect};
use sqlparser::parser::{Parser, ParserError};

use crate::finder::{SqlExtract, SqlString};
use crate::{debug, error, info};

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
                info!("Valid sql string: `{}`", sql_string.sql_content)
            }
            Err(e) => {
                error!(
                    "Invalid sql string: `{}` => {}",
                    sql_string.sql_content,
                    SqlError::from_parser_error(e).reason
                )
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

#[derive(Debug, Default)]
struct SqlError {
    pub reason: String,
    pub line: usize,
    pub col: usize,
}

impl SqlError {
    fn new(reason: String, line: usize, col: usize) -> Self {
        SqlError { reason, line, col }
    }

    fn from_parser_error(e: ParserError) -> SqlError {
        match e {
            ParserError::ParserError(msg) | ParserError::TokenizerError(msg) => {
                let line_marker = " at Line: ";
                let col_marker = ", Column: ";

                let line_start_idx = msg
                    .find(line_marker)
                    .expect("Should always contain line information.");

                let line_num_start = line_start_idx + line_marker.len();

                let comma_idx = msg[line_num_start..]
                    .find(col_marker)
                    .expect("Should always contain col information.");

                let line_num_end = line_num_start + comma_idx;
                let col_num_start = line_num_end + col_marker.len();

                let line = msg[line_num_start..line_num_end].parse().unwrap_or(0);
                let column = msg[col_num_start..].parse().unwrap_or(0);

                let reason_msg = msg[..line_start_idx].to_string();
                SqlError::new(reason_msg, line, column)
            }
            ParserError::RecursionLimitExceeded => {
                SqlError::new("Recursion Limit Exceeded".to_string(), 0, 0)
            }
        }
    }
}

/// Prepare SQL for parsing by replacing placeholders with dummy values
fn fill_placeholders(sql: &str) -> String {
    sql.replace('?', "'PLACEHOLDER'")
        .replace(":1", "'PLACEHOLDER'")
        .replace(":2", "'PLACEHOLDER'")
}
