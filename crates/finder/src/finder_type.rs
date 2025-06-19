use std::ops::{Add, Div, Mul, Sub};

#[derive(Debug)]
pub(crate) enum FinderType {
    Str(String),
    Int(String),
    Float(f64),
    Bool(bool),
    Tuple(Vec<FinderType>),
    Placeholder,
    Unhandled,
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
