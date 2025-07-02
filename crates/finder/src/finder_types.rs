use std::fmt;
use std::ops::{Add, Div, Mul, Sub};

use globset::{Glob, GlobSet, GlobSetBuilder};
use logging::{always_log, error};

use crate::range::ByteRange;

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
}

/// Represents a detected SQL variable
#[derive(Debug, Clone)]
pub struct SqlString {
    pub variable_name: String,
    pub sql_content: String,
    pub range: crate::range::Range,
}

impl SqlString {
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
    class_ctx: GlobSet,
}

impl FinderConfig {
    #[must_use]
    pub fn new(variable_ctx: &[String], func_ctx: &[String], class_ctx: &[String]) -> Self {
        Self {
            variable_ctx: slice_to_glob(variable_ctx, "variable_contexts"),
            func_ctx: slice_to_glob(func_ctx, "function_contexts"),
            class_ctx: slice_to_glob(class_ctx, "class_contexts"),
        }
    }
    pub(crate) fn is_sql_variable_name(&self, name: &str) -> bool {
        self.variable_ctx.is_match(name)
    }

    pub(crate) fn is_sql_function_name(&self, name: &str) -> bool {
        self.func_ctx.is_match(name)
    }

    pub(crate) fn is_sql_class_name(&self, name: &str) -> bool {
        self.class_ctx.is_match(name)
    }
}

fn slice_to_glob(patterns: &[String], log_ctx: &str) -> GlobSet {
    let valid_globs: Vec<Glob> = patterns
        .iter()
        .filter_map(|pattern| match Glob::new(pattern) {
            Ok(glob) => Some(glob),
            Err(e) => {
                always_log!("Failed to parse {log_ctx} glob pattern '{pattern}': {e}");
                None
            }
        })
        .collect();

    let builder = valid_globs
        .into_iter()
        .fold(GlobSetBuilder::new(), |mut builder, glob| {
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
    Unhandled,
}

impl FinderType {
    pub fn len(&self) -> usize {
        match self {
            Self::Str(s) | Self::Int(s) => s.len(),
            Self::Float(f) => f.to_string().len(),
            Self::Bool(b) => {
                if *b {
                    4
                } else {
                    5
                }
            }
            Self::Tuple(vec) => {
                if vec.is_empty() {
                    2
                } else {
                    2 + vec.iter().map(Self::len).sum::<usize>() + (vec.len() - 1) * 2
                }
            }
            Self::Placeholder => 11,
            Self::Unhandled => 9,
        }
    }

    pub fn truncate_content(&self, max_len: usize) -> String {
        match self {
            Self::Str(s) | Self::Int(s) => {
                if s.len() <= max_len {
                    s.clone()
                } else {
                    format!("{}...", &s[..max_len.saturating_sub(3)])
                }
            }
            _ => {
                let full_string = self.to_string();
                if full_string.len() <= max_len {
                    full_string
                } else {
                    format!("{}...", &full_string[..max_len.saturating_sub(3)])
                }
            }
        }
    }
    pub fn trunc_default(&self) -> String {
        self.truncate_content(50)
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
            Self::Placeholder => write!(f, "{{PLACEHOLDER}}"),
            Self::Unhandled => write!(f, "<unhandled>"),
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
