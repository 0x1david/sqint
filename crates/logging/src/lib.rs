use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::io::{self, Write};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

static GLOBAL_LOG_LEVEL: AtomicU8 = AtomicU8::new(LogLevel::Error as u8);
static LOGGER_INITIALIZED: OnceLock<()> = OnceLock::new();
static HAS_ERROR_OCCURRED: AtomicBool = AtomicBool::new(false);

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default, ValueEnum,
)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Always = 0,
    #[default]
    Error = 1,
    Warn = 2,
    Info = 3,
    Bail = 4,
    Debug = 5,
}

impl LogLevel {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Always => "ALWAYS",
            Self::Error => "ERROR",
            Self::Warn => "WARN",
            Self::Info => "INFO",
            Self::Bail => "BAIL",
            Self::Debug => "DEBUG",
        }
    }

    const fn color_code(self) -> &'static str {
        match self {
            Self::Always => "\x1b[1;37m", // Bold White
            Self::Error => "\x1b[31m",    // Red
            Self::Warn => "\x1b[33m",     // Yellow
            Self::Info => "\x1b[32m",     // Green
            Self::Bail => "\x1b[1;31m",   // Bold Red
            Self::Debug => "\x1b[36m",    // Cyan
        }
    }

    fn should_use_color() -> bool {
        !(std::env::var("NO_COLOR").is_ok()
            || std::env::var("CI").is_ok()
            || !atty::is(atty::Stream::Stdout))
    }
}

pub struct Logger;

impl Logger {
    pub fn init(level: LogLevel) {
        LOGGER_INITIALIZED.get_or_init(|| {
            GLOBAL_LOG_LEVEL.store(level as u8, Ordering::Relaxed);
        });
    }

    pub fn current_level() -> LogLevel {
        let level_u8 = GLOBAL_LOG_LEVEL.load(Ordering::Relaxed);
        match level_u8 {
            5 => LogLevel::Debug,
            4 => LogLevel::Bail,
            3 => LogLevel::Info,
            2 => LogLevel::Warn,
            0 => LogLevel::Always,
            _ => LogLevel::Error,
        }
    }

    pub fn should_log(level: LogLevel) -> bool {
        let current_level = GLOBAL_LOG_LEVEL.load(Ordering::Relaxed);
        (level as u8) <= current_level
    }

    pub fn log_message(level: LogLevel, message: &str, file: &str, line: u32) {
        let filename = std::path::Path::new(file)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(file);

        let timestamp = if matches!(level, LogLevel::Debug) {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default();
            format!("[{}.{:03}] ", now.as_secs() % 86400, now.subsec_millis())
        } else {
            String::new()
        };

        let output = if LogLevel::should_use_color() {
            format!(
                "{}{}[{}] {}:{} - {}\x1b[0m",
                timestamp,
                level.color_code(),
                level.as_str(),
                filename,
                line,
                message
            )
        } else {
            format!(
                "{}[{}] {}:{} - {}",
                timestamp,
                level.as_str(),
                filename,
                line,
                message
            )
        };

        match level {
            LogLevel::Error | LogLevel::Bail => {
                let _ = writeln!(io::stderr(), "{output}");
            }
            _ => {
                let _ = writeln!(io::stdout(), "{output}");
            }
        }

        if level == LogLevel::Error {
            HAS_ERROR_OCCURRED.store(true, Ordering::Relaxed);
        }
    }

    pub fn has_error_occurred() -> bool {
        HAS_ERROR_OCCURRED.load(Ordering::Relaxed)
    }

    #[must_use]
    pub fn exit_code() -> i32 {
        i32::from(Self::has_error_occurred())
    }

    #[cfg(test)]
    pub fn reset_error_state() {
        HAS_ERROR_OCCURRED.store(false, Ordering::Relaxed);
    }
}

#[macro_export]
macro_rules! log {
    ($level:expr, $($arg:tt)*) => {
        if $crate::Logger::should_log($level) {
            $crate::Logger::log_message(
                $level,
                &format!($($arg)*),
                file!(),
                line!()
            )
        }
    };
}
#[macro_export]
macro_rules! always_log {
    ($fmt:expr $(, $($arg:tt)*)?) => {
        $crate::log!($crate::LogLevel::Always, $fmt $(, $($arg)*)?)
    };
}

#[macro_export]
macro_rules! return_log {
    ($fmt:expr $(, $($arg:tt)*)?) => {
        $crate::log!($crate::LogLevel::Always, $fmt $(, $($arg)*)?);
        return
    };
}

#[macro_export]
macro_rules! error {
    ($fmt:expr $(, $($arg:tt)*)?) => {
        $crate::log!($crate::LogLevel::Error, $fmt $(, $($arg)*)?)
    };
}

#[macro_export]
macro_rules! bail {
    ($return_value:expr, $fmt:expr $(, $($arg:tt)*)?) => {{
        $crate::log!($crate::LogLevel::Bail, $fmt $(, $($arg)*)?);
        return $return_value;
    }};
}

#[macro_export]
macro_rules! bail_with {
    ($return_value:expr, $fmt:expr $(, $($arg:tt)*)?) => {{
        $crate::log!($crate::LogLevel::Bail, $fmt $(, $($arg)*)?);
        $return_value
    }};
}

#[macro_export]
macro_rules! warn {
    ($fmt:expr $(, $($arg:tt)*)?) => {
        $crate::log!($crate::LogLevel::Warn, $fmt $(, $($arg)*)?)
    };
}

#[macro_export]
macro_rules! info {
    ($fmt:expr $(, $($arg:tt)*)?) => {
        $crate::log!($crate::LogLevel::Info, $fmt $(, $($arg)*)?)
    };
}

#[macro_export]
macro_rules! debug {
    ($fmt:expr $(, $($arg:tt)*)?) => {
        $crate::log!($crate::LogLevel::Debug, $fmt $(, $($arg)*)?)
    };
}
