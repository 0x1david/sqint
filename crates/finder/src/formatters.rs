use crate::assign::ConstType;

pub(crate) fn format_value_as_unsigned(value: &ConstType) -> Option<String> {
    match value {
        ConstType::Int(i) => i.parse::<u64>().ok().map(|i| i.to_string()),
        ConstType::Float(f) => Some((*f as u64).to_string()),
        ConstType::Bool(b) => Some(if *b { "1".to_string() } else { "0".to_string() }),
        ConstType::Str(s) => s.parse::<u64>().ok().map(|i| i.to_string()),
        _ => None,
    }
}

pub(crate) fn format_value_as_binary(value: &ConstType) -> Option<String> {
    match value {
        ConstType::Int(i) => i.parse::<i64>().ok().map(|i| format!("{:b}", i)),
        ConstType::Float(f) => Some(format!("{:b}", *f as i64)),
        ConstType::Bool(b) => Some(if *b { "1".to_string() } else { "0".to_string() }),
        _ => None,
    }
}

pub(crate) fn format_value_as_general(value: &ConstType, specifier: &str) -> Option<String> {
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
        _ => None,
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

pub(crate) fn format_value_as_float(value: &ConstType, specifier: &str) -> Option<String> {
    let precision = extract_precision(specifier).unwrap_or(6);
    match value {
        ConstType::Float(f) => Some(format!("{:.prec$}", f, prec = precision)),
        ConstType::Int(i) => i
            .parse::<f64>()
            .ok()
            .map(|f| format!("{:.prec$}", f, prec = precision)),
        ConstType::Bool(b) => Some(if *b {
            format!("{:.prec$}", 1.0, prec = precision)
        } else {
            format!("{:.prec$}", 0.0, prec = precision)
        }),
        ConstType::Str(s) => s
            .parse::<f64>()
            .ok()
            .map(|f| format!("{:.prec$}", f, prec = precision)),
        _ => None,
    }
}

pub(crate) fn format_value_as_pointer(value: &ConstType) -> Option<String> {
    match value {
        ConstType::Int(i) => i.parse::<usize>().ok().map(|i| format!("0x{:x}", i)),
        ConstType::Float(f) => Some(format!("0x{:x}", *f as usize)),
        _ => None,
    }
}

pub(crate) fn extract_precision(specifier: &str) -> Option<usize> {
    if let Some(dot_pos) = specifier.find('.') {
        let after_dot = &specifier[dot_pos + 1..];
        if let Some(end) = after_dot.find(|c: char| c.is_alphabetic()) {
            after_dot[..end].parse().ok()
        } else {
            None
        }
    } else {
        None
    }
}
pub(crate) fn format_value_as_int(value: &ConstType) -> Option<String> {
    match value {
        ConstType::Int(i) => Some(i.clone()),
        ConstType::Float(f) => Some((*f as i64).to_string()),
        ConstType::Bool(b) => Some(if *b { "1".to_string() } else { "0".to_string() }),
        ConstType::Str(s) => s.parse::<i64>().ok().map(|i| i.to_string()),
        _ => None,
    }
}

pub(crate) fn format_value_as_octal(value: &ConstType) -> Option<String> {
    match value {
        ConstType::Int(i) => i.parse::<i64>().ok().map(|i| format!("{:o}", i)),
        ConstType::Float(f) => Some(format!("{:o}", *f as i64)),
        ConstType::Bool(b) => Some(if *b { "1".to_string() } else { "0".to_string() }),
        _ => None,
    }
}

pub(crate) fn format_value_as_hex(value: &ConstType, uppercase: bool) -> Option<String> {
    match value {
        ConstType::Int(i) => i.parse::<i64>().ok().map(|i| {
            if uppercase {
                format!("{:X}", i)
            } else {
                format!("{:x}", i)
            }
        }),
        ConstType::Float(f) => Some(if uppercase {
            format!("{:X}", *f as i64)
        } else {
            format!("{:x}", *f as i64)
        }),
        ConstType::Bool(b) => Some(if *b { "1".to_string() } else { "0".to_string() }),
        _ => None,
    }
}
pub(crate) fn format_value_as_scientific(value: &ConstType, specifier: &str) -> Option<String> {
    let precision = extract_precision(specifier).unwrap_or(6);
    let uppercase = specifier.contains('E');

    match value {
        ConstType::Float(f) => {
            if uppercase {
                Some(format!("{:.prec$E}", f, prec = precision))
            } else {
                Some(format!("{:.prec$e}", f, prec = precision))
            }
        }
        ConstType::Int(i) => i.parse::<f64>().ok().map(|f| {
            if uppercase {
                format!("{:.prec$E}", f, prec = precision)
            } else {
                format!("{:.prec$e}", f, prec = precision)
            }
        }),
        ConstType::Bool(b) => {
            let val = if *b { 1.0 } else { 0.0 };
            if uppercase {
                Some(format!("{:.prec$E}", val, prec = precision))
            } else {
                Some(format!("{:.prec$e}", val, prec = precision))
            }
        }
        ConstType::Str(s) => s.parse::<f64>().ok().map(|f| {
            if uppercase {
                format!("{:.prec$E}", f, prec = precision)
            } else {
                format!("{:.prec$e}", f, prec = precision)
            }
        }),
        _ => None,
    }
}

pub(crate) fn format_value_as_char(value: &ConstType) -> Option<String> {
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
        _ => None,
    }
}
