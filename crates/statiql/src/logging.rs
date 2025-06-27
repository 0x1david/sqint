use std::io::{self, Write};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

static GLOBAL_LOG_LEVEL: AtomicU8 = AtomicU8::new(LogLevel::Info as u8);
static LOGGER_INITIALIZED: OnceLock<()> = OnceLock::new();
static HAS_ERROR_OCCURRED: AtomicBool = AtomicBool::new(false);
static HAS_BAIL_OCCURRED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Always = 0,
    Error = 1,
    Warn = 2,
    Info = 3,
    Bail = 4,
    Debug = 5,
}

impl LogLevel {
    fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Always => "ALWAYS",
            LogLevel::Error => "ERROR",
            LogLevel::Warn => "WARN",
            LogLevel::Info => "INFO",
            LogLevel::Bail => "BAIL",
            LogLevel::Debug => "DEBUG",
        }
    }

    fn color_code(&self) -> &'static str {
        match self {
            LogLevel::Always => "\x1b[1;37m", // Bold White
            LogLevel::Error => "\x1b[31m",    // Red
            LogLevel::Warn => "\x1b[33m",     // Yellow
            LogLevel::Info => "\x1b[32m",     // Green
            LogLevel::Bail => "\x1b[1;31m",   // Bold Red
            LogLevel::Debug => "\x1b[36m",    // Cyan
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
            0 => LogLevel::Always,
            1 => LogLevel::Error,
            2 => LogLevel::Warn,
            3 => LogLevel::Info,
            4 => LogLevel::Bail,
            5 => LogLevel::Debug,
            _ => LogLevel::Info, // fallback
        }
    }

    pub fn should_log(level: LogLevel) -> bool {
        let current_level = GLOBAL_LOG_LEVEL.load(Ordering::Relaxed);
        (level as u8) <= current_level
    }

    pub fn log_message(level: LogLevel, message: &str, file: &str, line: u32) {
        if !Self::should_log(level) {
            return;
        }

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
                let _ = writeln!(io::stderr(), "{}", output);
                let _ = io::stderr().flush();
            }
            _ => {
                let _ = writeln!(io::stdout(), "{}", output);
                let _ = io::stdout().flush();
            }
        }

        match level {
            LogLevel::Error => {
                HAS_ERROR_OCCURRED.store(true, Ordering::Relaxed);
            }
            LogLevel::Bail => {
                HAS_BAIL_OCCURRED.store(true, Ordering::Relaxed);
            }
            _ => {}
        }
    }

    pub fn has_error_occurred() -> bool {
        HAS_ERROR_OCCURRED.load(Ordering::Relaxed)
    }

    pub fn has_bail_occurred() -> bool {
        HAS_BAIL_OCCURRED.load(Ordering::Relaxed)
    }

    pub fn exit_code() -> i32 {
        if Self::has_error_occurred() { 1 } else { 0 }
    }

    #[cfg(test)]
    pub fn reset_error_state() {
        HAS_ERROR_OCCURRED.store(false, Ordering::Relaxed);
        HAS_BAIL_OCCURRED.store(false, Ordering::Relaxed);
    }
}

#[macro_export]
macro_rules! log {
    ($level:expr, $($arg:tt)*) => {
        $crate::Logger::log_message(
            $level,
            &format!($($arg)*),
            file!(),
            line!()
        )
    };
}

#[macro_export]
macro_rules! always_log {
    ($($arg:tt)*) => {
        $crate::log!($crate::LogLevel::Always, $($arg)*)
    };
}

#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {
        $crate::log!($crate::LogLevel::Error, $($arg)*)
    };
}

#[macro_export]
macro_rules! bail {
    ($return_value:expr, $($arg:tt)*) => {{
        $crate::log!($crate::LogLevel::Bail, $($arg)*);
        return $return_value;
    }};
}

#[macro_export]
macro_rules! bail_with {
    ($return_value:expr, $($arg:tt)*) => {{
        $crate::log!($crate::LogLevel::Bail, $($arg)*);
        $return_value
    }};
}

#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {
        $crate::log!($crate::LogLevel::Warn, $($arg)*)
    };
}

#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        $crate::log!($crate::LogLevel::Info, $($arg)*)
    };
}

#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        $crate::log!($crate::LogLevel::Debug, $($arg)*)
    };
}
