use std::sync::Arc;

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
    dialect: Box<dyn sqlparser::dialect::Dialect>,
}

impl SqlAnalyzer {
    pub fn new(dialect: SqlDialect) -> Self {
        let dialect: Box<dyn sqlparser::dialect::Dialect> = match dialect {
            SqlDialect::Generic => Box::new(GenericDialect {}),
            SqlDialect::PostgreSQL => Box::new(PostgreSqlDialect {}),
            SqlDialect::SQLite => Box::new(SQLiteDialect {}),
        };
        Self { dialect }
    }

    pub fn analyze_sql_extract(&self, extract: &SqlExtract, cfg: Arc<Config>) {
        if extract.strings.is_empty() {
            debug!("Empty extract `{}`", extract.file_path);
        }

        extract
            .strings
            .iter()
            // .filter(|s| cfg.variable_names.contains(&s.variable_name))
            .for_each(|sql_string| self.analyze_sql_string(sql_string));
    }

    fn analyze_sql_string(&self, sql_string: &SqlString) {
        let filled_sql = fill_placeholders(&sql_string.sql_content);

        match Parser::parse_sql(&*self.dialect, &filled_sql) {
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

                // Check if line information is present in the error message
                if let Some(line_start_idx) = msg.find(line_marker) {
                    let line_num_start = line_start_idx + line_marker.len();

                    // Check if column information is also present
                    if let Some(comma_idx) = msg[line_num_start..].find(col_marker) {
                        let line_num_end = line_num_start + comma_idx;
                        let col_num_start = line_num_end + col_marker.len();
                        let line = msg[line_num_start..line_num_end].parse().unwrap_or(0);
                        let column = msg[col_num_start..].parse().unwrap_or(0);
                        let reason_msg = msg[..line_start_idx].to_string();
                        Self::new(reason_msg, line, column)
                    } else {
                        // Line marker found but no column marker
                        Self::new(
                            "Malformed error message: missing column information".to_string(),
                            0,
                            0,
                        )
                    }
                } else {
                    // No line information available in the error message
                    Self::new(
                        "SQL parsing error with no position information".to_string(),
                        0,
                        0,
                    )
                }
            }
            ParserError::RecursionLimitExceeded => {
                Self::new("Recursion Limit Exceeded".to_string(), 0, 0)
            }
        }
    }
}

/// Prepare SQL for parsing by replacing placeholders with dummy values
/// TODO: Config defined list of placeholders and their replacements
fn fill_placeholders(sql: &str) -> String {
    sql.replace("{PLACEHOLDER}", "PLACEHOLDER")
        .replace("?", "'PLACEHOLDER'")
        .replace("ISNULL", "IS NULL")
}
