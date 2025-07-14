use std::ops::{Add, Div, Mul, Sub};
use std::path::{Path, PathBuf};
use std::{env, fmt};

use globset::{Glob, GlobSet, GlobSetBuilder};
use logging::{always_log, error};
use regex::Regex;

use crate::preanalysis::ByteRange;

// Internal result type for processing
#[derive(Debug, Clone)]
pub struct SqlResult {
    pub byte_range: ByteRange,
    pub variable_name: String,
    pub content: FinderType,
}

#[derive(Debug, Clone)]
pub struct SqlExtract {
    pub file_path: String,
    pub strings: Vec<SqlString>,
    pub rel_path: String,
}

impl SqlExtract {
    pub fn new(file_path: String, strings: Vec<SqlString>) -> Self {
        let cwd = env::current_dir().expect("Can't get cwd, this is likely a permissions issue.");
        let cwd_name = cwd
            .file_name()
            .expect("Current directory should have a name");

        let relative_part = Path::new(&file_path)
            .strip_prefix(&cwd)
            .expect("Should always be able to strip prefix cwd from curr_file.");

        let mut full_rel_path = PathBuf::new();
        full_rel_path.push(cwd_name);
        full_rel_path.push(relative_part);

        let rel_path = full_rel_path.to_string_lossy().to_string();

        Self {
            file_path,
            strings,
            rel_path,
        }
    }
}

/// Represents a detected SQL variable
#[derive(Debug, Clone)]
pub struct SqlString {
    pub variable_name: String,
    pub sql_content: String,
    pub range: crate::preanalysis::Range,
}

impl SqlString {
    pub fn new(
        variable_name: String,
        sql_content: String,
        range: crate::preanalysis::Range,
    ) -> Self {
        Self {
            variable_name,
            sql_content,
            range,
        }
    }
    fn truncate_content(&self, len: usize) -> &str {
        &self.sql_content[..self.sql_content.len().min(len)]
    }
    #[must_use]
    pub fn trunc_default(&self) -> &str {
        self.truncate_content(50)
    }
}

#[derive(Debug, Clone)]
pub struct FinderConfig {
    variable_ctx: GlobSet,
    func_ctx: GlobSet,
    sql_regex: Regex,
}

impl FinderConfig {
    #[must_use]
    #[allow(clippy::missing_panics_doc)]
    pub fn new(variable_ctx: &[String], func_ctx: &[String]) -> Self {
        Self {
            variable_ctx: slice_to_glob(variable_ctx, "variable_contexts"),
            func_ctx: slice_to_glob(func_ctx, "function_contexts"),
            sql_regex: Regex::new(r"(?i)^\s*(select|insert|update|delete|create|drop|alter|truncate|with|explain|show|describe)\b").unwrap(),
        }
    }
    pub(crate) fn is_sql_variable_name(&self, name: &str) -> bool {
        self.variable_ctx.is_match(name)
    }

    pub(crate) fn is_sql_function_name(&self, name: &str) -> bool {
        self.func_ctx.is_match(name)
    }

    pub(crate) fn is_sql_str(&self, input: &str) -> bool {
        self.sql_regex.is_match(input)
    }
}

fn slice_to_glob(patterns: &[String], log_ctx: &str) -> GlobSet {
    let valid_globs = patterns
        .iter()
        .filter_map(|pattern| match Glob::new(pattern) {
            Ok(glob) => Some(glob),
            Err(e) => {
                always_log!("Failed to parse {log_ctx} glob pattern '{pattern}': {e}");
                None
            }
        });

    let builder = valid_globs.fold(GlobSetBuilder::new(), |mut builder, glob| {
        builder.add(glob);
        builder
    });

    builder.build().unwrap_or_else(|e| {
        error!("Failed to build GlobSet for {log_ctx}: {e}");
        GlobSetBuilder::new().build().unwrap()
    })
}

#[derive(Debug, Clone)]
pub enum FinderType {
    Str(String),
    Int(String),
    Float(f64),
    Bool(bool),
    Tuple(Vec<FinderType>),
    Placeholder,
}

impl FinderType {
    pub fn get_str(&self) -> Option<&str> {
        match self {
            Self::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn is_placeholder(&self) -> bool {
        matches!(self, Self::Placeholder)
    }
}

impl std::fmt::Display for FinderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Str(s) => write!(f, "{s}"),
            Self::Int(i) => write!(f, "{i}"),
            Self::Float(fl) => write!(f, "{fl}"),
            // Using numeric booleans for maximum db compatibility
            Self::Bool(b) => write!(f, "{}", u8::from(*b)),
            Self::Tuple(t) => {
                write!(f, "(")?;
                for (i, item) in t.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{item}")?;
                }
                write!(f, ")")
            }
            Self::Placeholder => write!(f, "PLACEHOLDER"),
        }
    }
}
impl Add for FinderType {
    type Output = Option<Self>;

    fn add(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Self::Placeholder, _) | (_, Self::Placeholder) => Some(Self::Placeholder),
            (Self::Str(s1), Self::Str(s2)) => Some(Self::Str(s1 + &s2)),
            (Self::Int(s1), Self::Int(s2)) => Some(Self::Int(s1 + &s2)),
            (Self::Float(f1), Self::Float(f2)) => Some(Self::Float(f1 + f2)),
            (Self::Bool(b1), Self::Bool(b2)) => Some(Self::Bool(b1 || b2)),
            (Self::Tuple(mut t1), Self::Tuple(t2)) => {
                t1.extend(t2);
                Some(Self::Tuple(t1))
            }
            (_, _) => None,
        }
    }
}
impl Sub for FinderType {
    type Output = Option<Self>;

    fn sub(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Self::Placeholder, _) | (_, Self::Placeholder) => Some(Self::Placeholder),
            (Self::Float(f1), Self::Float(f2)) => Some(Self::Float(f1 - f2)),
            (Self::Int(s1), Self::Int(s2)) => {
                if let (Ok(i1), Ok(i2)) = (s1.parse::<i64>(), s2.parse::<i64>()) {
                    Some(Self::Int((i1 - i2).to_string()))
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

impl Mul for FinderType {
    type Output = Option<Self>;

    fn mul(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Self::Placeholder, _) | (_, Self::Placeholder) => Some(Self::Placeholder),
            (Self::Float(f1), Self::Float(f2)) => Some(Self::Float(f1 * f2)),
            (Self::Int(s1), Self::Int(s2)) => {
                if let (Ok(i1), Ok(i2)) = (s1.parse::<i64>(), s2.parse::<i64>()) {
                    Some(Self::Int((i1 * i2).to_string()))
                } else {
                    None
                }
            }
            (Self::Str(s), Self::Int(n)) | (Self::Int(n), Self::Str(s)) => n
                .parse::<usize>()
                .ok()
                .map(|count| Self::Str(s.repeat(count))),
            _ => None,
        }
    }
}

impl Div for FinderType {
    type Output = Option<Self>;

    fn div(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Self::Placeholder, _) | (_, Self::Placeholder) => Some(Self::Placeholder),
            (Self::Float(f1), Self::Float(f2)) => {
                if f2.is_normal() {
                    Some(Self::Float(f1 / f2))
                } else {
                    None
                }
            }

            (Self::Int(s1), Self::Int(s2)) => {
                if let (Ok(i1), Ok(i2)) = (s1.parse::<i64>(), s2.parse::<i64>()) {
                    if i2 == 0 {
                        Some(Self::Int((i1 / i2).to_string()))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    }
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
            writeln!(f, "{sql_string}")?;
        }
        Ok(())
    }
}
