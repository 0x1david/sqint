#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]
use logging::except_none;

use crate::assign::ConstType;

pub fn format_value_as_unsigned(value: &ConstType) -> Option<String> {
    match value {
        ConstType::Int(i) => i.parse::<u64>().ok().map(|i| i.to_string()),
        ConstType::Float(f) => Some((*f as u64).to_string()),
        ConstType::Bool(b) => Some(if *b { "1".to_string() } else { "0".to_string() }),
        ConstType::Str(s) => s.parse::<u64>().ok().map(|i| i.to_string()),
        _ => except_none!("Unhandled unsigned value formatting: {value}"),
    }
}

pub fn format_value_as_binary(value: &ConstType) -> Option<String> {
    match value {
        ConstType::Int(i) => i.parse::<i64>().ok().map(|i| format!("{i:b}")),
        ConstType::Float(f) => Some(format!("{:b}", *f as i64)),
        ConstType::Bool(b) => Some(if *b { "1".to_string() } else { "0".to_string() }),
        _ => except_none!("Unhandled binary value formatting: {value}"),
    }
}

pub fn format_value_as_general(value: &ConstType, specifier: &str) -> Option<String> {
    let precision = extract_precision(specifier).unwrap_or(6);
    let uppercase = specifier.contains('G');

    match value {
        ConstType::Float(f) => Some(format_general_float(*f, precision, uppercase)),
        ConstType::Int(i) => i
            .parse::<f64>()
            .ok()
            .map(|f| format_general_float(f, precision, uppercase)),
        ConstType::Bool(b) => {
            let val = if *b { 1.0 } else { 0.0 };
            Some(format_general_float(val, precision, uppercase))
        }
        ConstType::Str(s) => s
            .parse::<f64>()
            .ok()
            .map(|f| format_general_float(f, precision, uppercase)),
        _ => except_none!("Unhandled general value formatting: {value}"),
    }
}
fn format_general_float(f: f64, precision: usize, uppercase: bool) -> String {
    let abs_f = f.abs();
    let exponent = if abs_f == 0.0 {
        0
    } else {
        abs_f.log10().floor() as i32
    };

    if exponent < -4 || exponent >= precision as i32 {
        if uppercase {
            format!("{:.prec$E}", f, prec = precision.saturating_sub(1))
        } else {
            format!("{:.prec$e}", f, prec = precision.saturating_sub(1))
        }
    } else {
        let formatted = format!(
            "{:.prec$}",
            f,
            prec = precision
                .saturating_sub(1)
                .saturating_sub(exponent.max(0) as usize)
        );

        if formatted.contains('.') {
            formatted
                .trim_end_matches('0')
                .trim_end_matches('.')
                .to_string()
        } else {
            formatted
        }
    }
}

pub fn format_value_as_float(value: &ConstType, specifier: &str) -> Option<String> {
    let precision = extract_precision(specifier).unwrap_or(6);
    match value {
        ConstType::Float(f) => Some(format!("{f:.precision$}")),
        ConstType::Int(i) => i.parse::<f64>().ok().map(|f| format!("{f:.precision$}")),
        ConstType::Bool(b) => Some(if *b {
            format!("{:.precision$}", 1.0)
        } else {
            format!("{:.precision$}", 0.0)
        }),
        ConstType::Str(s) => s.parse::<f64>().ok().map(|f| format!("{f:.precision$}")),
        _ => except_none!("Unhandled float value formatting: {value}"),
    }
}

pub fn format_value_as_pointer(value: &ConstType) -> Option<String> {
    match value {
        ConstType::Int(i) => i.parse::<usize>().ok().map(|i| format!("0x{i:x}")),
        ConstType::Float(f) => Some(format!("0x{:x}", *f as usize)),
        _ => except_none!("Unhandled pointer value formatting: {value}"),
    }
}

pub fn extract_precision(specifier: &str) -> Option<usize> {
    specifier.find('.').and_then(|dot_pos| {
        let after_dot = &specifier[dot_pos + 1..];
        after_dot
            .find(|c: char| c.is_alphabetic())
            .and_then(|end| after_dot[..end].parse().ok())
    })
}
pub fn format_value_as_int(value: &ConstType) -> Option<String> {
    match value {
        ConstType::Int(i) => Some(i.clone()),
        ConstType::Float(f) => Some((*f as i64).to_string()),
        ConstType::Bool(b) => Some(if *b { "1".to_string() } else { "0".to_string() }),
        ConstType::Str(s) => s.parse::<i64>().ok().map(|i| i.to_string()),
        _ => except_none!("Unhandled integer value formatting: {value}"),
    }
}

pub fn format_value_as_octal(value: &ConstType) -> Option<String> {
    match value {
        ConstType::Int(i) => i.parse::<i64>().ok().map(|i| format!("{i:o}")),
        ConstType::Float(f) => Some(format!("{:o}", *f as i64)),
        ConstType::Bool(b) => Some(if *b { "1".to_string() } else { "0".to_string() }),
        _ => except_none!("Unhandled octal value formatting: {value}"),
    }
}

pub fn format_value_as_hex(value: &ConstType, uppercase: bool) -> Option<String> {
    match value {
        ConstType::Int(i) => i.parse::<i64>().ok().map(|i| {
            if uppercase {
                format!("{i:X}")
            } else {
                format!("{i:x}")
            }
        }),
        ConstType::Float(f) => Some(if uppercase {
            format!("{:X}", *f as i64)
        } else {
            format!("{:x}", *f as i64)
        }),
        ConstType::Bool(b) => Some(if *b { "1".to_string() } else { "0".to_string() }),
        _ => except_none!("Unhandled hex value formatting: {value}"),
    }
}
pub fn format_value_as_scientific(value: &ConstType, specifier: &str) -> Option<String> {
    let precision = extract_precision(specifier).unwrap_or(6);
    let uppercase = specifier.contains('E');

    match value {
        ConstType::Float(f) => {
            if uppercase {
                Some(format!("{f:.precision$E}"))
            } else {
                Some(format!("{f:.precision$e}"))
            }
        }
        ConstType::Int(i) => i.parse::<f64>().ok().map(|f| {
            if uppercase {
                format!("{f:.precision$E}")
            } else {
                format!("{f:.precision$e}")
            }
        }),
        ConstType::Bool(b) => {
            let val = if *b { 1.0 } else { 0.0 };
            if uppercase {
                Some(format!("{val:.precision$E}"))
            } else {
                Some(format!("{val:.precision$e}"))
            }
        }
        ConstType::Str(s) => s.parse::<f64>().ok().map(|f| {
            if uppercase {
                format!("{f:.precision$E}")
            } else {
                format!("{f:.precision$e}")
            }
        }),
        _ => except_none!("Unhandled scientific value formatting: {value}"),
    }
}

pub fn format_value_as_char(value: &ConstType) -> Option<String> {
    match value {
        ConstType::Int(i) => {
            if let Ok(code) = i.parse::<u32>() {
                if let Some(ch) = char::from_u32(code) {
                    return Some(ch.to_string());
                }
            }
            None
        }
        ConstType::Str(s) => {
            if s.len() == 1 {
                Some(s.clone())
            } else {
                None
            }
        }
        _ => except_none!("Unhandled char value formatting: {value}"),
    }
}
