use sqlparser::ast::Statement;
use sqlparser::dialect::{GenericDialect, PostgreSqlDialect, SQLiteDialect};
use sqlparser::parser::{Parser, ParserError};

use finder::{SqlExtract, SqlString};
use logging::{debug, error, info};

use crate::config::Config;

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
    pub const fn new(dialect: SqlDialect) -> Self {
        Self { dialect }
    }

    pub fn analyze_sql_extract(&self, extract: &SqlExtract, cfg: &Config) {
        if extract.strings.is_empty() {
            debug!("Empty extract `{}`", extract.file_path);
        }

        extract
            .strings
            .iter()
            .filter(|s| cfg.variable_names.contains(&s.variable_name))
            .for_each(|sql_string| self.analyze_sql_string(sql_string));
    }

    fn analyze_sql_string(&self, sql_string: &SqlString) {
        let filled_sql = fill_placeholders(&sql_string.sql_content);

        match self.parse_sql(&filled_sql) {
            Ok(_) => info!("Valid sql string: `{}`", sql_string.sql_content),
            Err(e) => {
                error!(
                    "Invalid sql string: `{}` => {}",
                    sql_string.sql_content,
                    SqlError::from_parser_error(e).reason
                );
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
    const fn new(reason: String, line: usize, col: usize) -> Self {
        Self { reason, line, col }
    }

    fn from_parser_error(e: ParserError) -> Self {
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
                Self::new(reason_msg, line, column)
            }
            ParserError::RecursionLimitExceeded => {
                Self::new("Recursion Limit Exceeded".to_string(), 0, 0)
            }
        }
    }
}

/// Prepare SQL for parsing by replacing placeholders with dummy values
fn fill_placeholders(sql: &str) -> String {
    sql.replace("{PLACEHOLDER}", "PLACEHOLDER")
        .replace(":1", "'PLACEHOLDER'")
        .replace(":2", "'PLACEHOLDER'")
}
