use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

static GLOBAL_LOG_LEVEL: AtomicU8 = AtomicU8::new(LogLevel::Info as u8);
static LOGGER_INITIALIZED: OnceLock<()> = OnceLock::new();
static HAS_ERROR_OCCURRED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug)]
pub enum LogLevel {
    Always = 0,
    Error = 1,
    Warn = 2,
    Info = 3,
    Exception = 4,
    Debug = 5,
}

impl LogLevel {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Always => "ALWAYS",
            Self::Error => "ERROR",
            Self::Warn => "WARN",
            Self::Info => "INFO",
            Self::Exception => "PROGRAMMING EXCEPTION",
            Self::Debug => "DEBUG",
        }
    }

    const fn color_code(self) -> &'static str {
        match self {
            Self::Always => "\x1b[1;37m",                // Bold White
            Self::Error | Self::Exception => "\x1b[31m", // Red
            Self::Warn => "\x1b[33m",                    // Yellow
            Self::Info => "\x1b[32m",                    // Green
            Self::Debug => "\x1b[36m",                   // Cyan
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

    pub fn should_log(level: LogLevel) -> bool {
        let current_level = GLOBAL_LOG_LEVEL.load(Ordering::Relaxed);
        (level as u8) <= current_level
    }

    pub fn log_message(level: LogLevel, message: &str, file: &str, line: u32) {
        if Self::should_log(level) {
            println!(
                "{}[{}] {}:{} - {}\x1b[0m",
                level.color_code(),
                level.as_str(),
                file,
                line,
                message
            );
        }

        if matches!(level, LogLevel::Error) {
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

// TODO: Change exceptions later to LogLevel exception, currently always for ease of development
#[macro_export]
macro_rules! exception {
    ($($arg:tt)*) => {
        $crate::log!($crate::LogLevel::Always, $($arg)*)
    };
}

#[macro_export]
macro_rules! except_none {
    ($($arg:tt)*) => {
        {
        $crate::log!($crate::LogLevel::Always, $($arg)*);
        None
        }
    };
}

#[macro_export]
macro_rules! except_ret {
    ($ret:expr, $($arg:tt)*) => {
        {
        $crate::log!($crate::LogLevel::Always, $($arg)*);
        $ret
        }
    };
}

#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {
        $crate::log!($crate::LogLevel::Error, $($arg)*)
    };
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
