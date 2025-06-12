use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

static GLOBAL_LOG_LEVEL: AtomicU8 = AtomicU8::new(LogLevel::Info as u8);
static LOGGER_INITIALIZED: OnceLock<()> = OnceLock::new();

#[derive(Clone, Copy, Debug)]
pub enum LogLevel {
    Error = 0,
    Warn = 1,
    Info = 2,
    Debug = 3,
}

impl LogLevel {
    fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Error => "ERROR",
            LogLevel::Warn => "WARN",
            LogLevel::Info => "INFO",
            LogLevel::Debug => "DEBUG",
        }
    }

    fn color_code(&self) -> &'static str {
        match self {
            LogLevel::Error => "\x1b[31m", // Red
            LogLevel::Warn => "\x1b[33m",  // Yellow
            LogLevel::Info => "\x1b[32m",  // Green
            LogLevel::Debug => "\x1b[36m", // Cyan
        }
    }
}

pub struct Logger;

impl Logger {
    pub fn init(level: LogLevel) {
        LOGGER_INITIALIZED.get_or_init(|| {
            GLOBAL_LOG_LEVEL.store(level as u8, Ordering::Relaxed);
        });
    }

    pub fn set_level(level: LogLevel) {
        GLOBAL_LOG_LEVEL.store(level as u8, Ordering::Relaxed);
    }

    pub fn should_log(level: LogLevel) -> bool {
        let current_level = GLOBAL_LOG_LEVEL.load(Ordering::Relaxed);
        (level as u8) <= current_level
    }

    pub fn log_message(level: LogLevel, message: &str, file: &str, line: u32) {
        if Self::should_log(level) {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();

            println!(
                "{}{} [{}] {}:{} - {}\x1b[0m",
                level.color_code(),
                timestamp,
                level.as_str(),
                file,
                line,
                message
            );
        }
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
macro_rules! error {
    ($($arg:tt)*) => {
        log!($crate::LogLevel::Error, $($arg)*)
    };
}

#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {
        log!($crate::LogLevel::Warn, $($arg)*)
    };
}

#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        log!($crate::LogLevel::Info, $($arg)*)
    };
}

#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        log!($crate::LogLevel::Debug, $($arg)*)
    };
}
