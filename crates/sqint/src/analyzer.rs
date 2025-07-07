use std::collections::HashMap;

use sqlparser::dialect::{
    AnsiDialect, BigQueryDialect, ClickHouseDialect, DuckDbDialect, GenericDialect, HiveDialect,
    MsSqlDialect, MySqlDialect, PostgreSqlDialect, RedshiftSqlDialect, SQLiteDialect,
    SnowflakeDialect,
};

use sqlparser::parser::{Parser, ParserError};

use finder::{SqlExtract, SqlString};
use logging::{error, info};

#[derive(Debug, Clone)]
pub enum SqlDialect {
    Generic,
    PostgreSQL,
    SQLite,
    Ansi,
    BigQuery,
    ClickHouse,
    DuckDb,
    Hive,
    MsSql,
    MySql,
    RedshiftSql,
    Snowflake,
}

pub struct SqlAnalyzer {
    dialect: Box<dyn sqlparser::dialect::Dialect>,
    mappings: HashMap<String, String>,
}

impl SqlAnalyzer {
    pub fn new(
        dialect: &SqlDialect,
        mut dialect_mappings: HashMap<String, String>,
        placeholders: &[String],
    ) -> Self {
        let dialect: Box<dyn sqlparser::dialect::Dialect> = match dialect {
            SqlDialect::Generic => Box::new(GenericDialect {}),
            SqlDialect::PostgreSQL => Box::new(PostgreSqlDialect {}),
            SqlDialect::SQLite => Box::new(SQLiteDialect {}),
            SqlDialect::Ansi => Box::new(AnsiDialect {}),
            SqlDialect::BigQuery => Box::new(BigQueryDialect {}),
            SqlDialect::ClickHouse => Box::new(ClickHouseDialect {}),
            SqlDialect::DuckDb => Box::new(DuckDbDialect {}),
            SqlDialect::Hive => Box::new(HiveDialect {}),
            SqlDialect::MsSql => Box::new(MsSqlDialect {}),
            SqlDialect::MySql => Box::new(MySqlDialect {}),
            SqlDialect::RedshiftSql => Box::new(RedshiftSqlDialect {}),
            SqlDialect::Snowflake => Box::new(SnowflakeDialect {}),
        };
        for p in placeholders {
            dialect_mappings.insert(p.clone(), "PLACEHOLDER".to_string());
        }

        Self {
            dialect,
            mappings: dialect_mappings,
        }
    }

    pub fn analyze_sql_extract(&self, extract: &SqlExtract) {
        extract
            .strings
            .iter()
            .for_each(|sql_string| self.analyze_sql_string(sql_string, &extract.file_path));
    }

    fn analyze_sql_string(&self, sql_string: &SqlString, filename: &str) {
        let filled_sql = self.fill_placeholders(&sql_string.sql_content);

        match Parser::parse_sql(&*self.dialect, &filled_sql) {
            Ok(_) => info!("Valid sql string: `{}`", sql_string.sql_content),
            Err(e) => {
                error!(
                    "Invalid sql literal in {} at {} `{}`: `{}` => {}",
                    filename,
                    sql_string.range.start,
                    sql_string.variable_name,
                    sql_string.sql_content,
                    SqlError::from_parser_error(e).reason
                );
            }
        }
    }

    // Multipass fill doesnt' seem to induce much of a performance loss on a reasonable scale.
    // So singlepass is probably not needed for now.
    fn fill_placeholders(&self, sql: &str) -> String {
        self.mappings
            .iter()
            .fold(sql.to_string(), |acc, (k, v)| acc.replace(k, v))
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

                // if line information is present in msg
                msg.find(line_marker).map_or_else(
                    || {
                        Self::new(
                            "SQL parsing error with no position information".to_string(),
                            0,
                            0,
                        )
                    },
                    {
                        |line_start_idx| {
                            let line_num_start = line_start_idx + line_marker.len();

                            // if col information is also present
                            msg[line_num_start..].find(col_marker).map_or_else(
                                || {
                                    Self::new(
                                        "Malformed error message: missing column information"
                                            .to_string(),
                                        0,
                                        0,
                                    )
                                },
                                |comma_idx| {
                                    let line_num_end = line_num_start + comma_idx;
                                    let col_num_start = line_num_end + col_marker.len();
                                    let line =
                                        msg[line_num_start..line_num_end].parse().unwrap_or(0);
                                    let column = msg[col_num_start..].parse().unwrap_or(0);
                                    let reason_msg = msg[..line_start_idx].to_string();
                                    Self::new(reason_msg, line, column)
                                },
                            )
                        }
                    },
                )
            }
            ParserError::RecursionLimitExceeded => {
                Self::new("Recursion Limit Exceeded".to_string(), 0, 0)
            }
        }
    }
}

impl SqlDialect {
    pub fn from_str(dialect_str: &str) -> Option<Self> {
        let normalized = dialect_str.to_lowercase();

        match normalized.as_str() {
            "postgres" => Some(SqlDialect::PostgreSQL),
            "mysql" => Some(SqlDialect::MySql),
            "sqlite" => Some(SqlDialect::SQLite),
            "mssql" => Some(SqlDialect::MsSql),
            "bigquery" => Some(SqlDialect::BigQuery),
            "snowflake" => Some(SqlDialect::Snowflake),
            "redshift" => Some(SqlDialect::RedshiftSql),
            "clickhouse" => Some(SqlDialect::ClickHouse),
            "duckdb" => Some(SqlDialect::DuckDb),
            "hive" => Some(SqlDialect::Hive),
            "ansi" => Some(SqlDialect::Ansi),
            "generic" | "default" => Some(SqlDialect::Generic),
            _ => None,
        }
    }

    pub fn supported_dialects() -> Vec<&'static str> {
        vec![
            "postgres",
            "mysql",
            "sqlite",
            "mssql",
            "bigquery",
            "snowflake",
            "redshift",
            "clickhouse",
            "duckdb",
            "hive",
            "ansi",
            "generic",
            "default",
        ]
    }
}
