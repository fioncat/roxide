use std::sync::OnceLock;

use console::style;

#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        $crate::debug::write_debug_logs(format!($($arg)*))
    };
}

#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        $crate::debug::write_info_logs(format!($($arg)*))
    };
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        $crate::debug::write_info_logs(format!($($arg)*))
    };
}

#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {
        $crate::debug::write_warn_logs(format!($($arg)*))
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Level {
    Debug,
    #[default]
    Info,
    Print,
    Warn,
    Mute,
}

static LEVEL: OnceLock<Level> = OnceLock::new();

pub fn set_level(level: Level) {
    LEVEL.set(level).unwrap();
}

fn get_level() -> Level {
    LEVEL.get().copied().unwrap_or_default()
}

pub fn write_debug_logs(msg: String) {
    if !matches!(get_level(), Level::Debug) {
        return;
    }
    eprintln!("DEBUG {msg}");
}

pub fn write_info_logs(msg: String) {
    if !matches!(get_level(), Level::Debug | Level::Info) {
        return;
    }

    let styled = style("==>").green().bold();
    eprintln!("{styled} {msg}");
}

pub fn write_print_logs(msg: String) {
    if !matches!(get_level(), Level::Debug | Level::Info | Level::Print) {
        return;
    }
    eprintln!("{msg}");
}

pub fn write_warn_logs(msg: String) {
    if !matches!(
        get_level(),
        Level::Debug | Level::Info | Level::Print | Level::Warn
    ) {
        return;
    }

    let styled = style(format!("WARN {msg}")).yellow().bold();
    eprintln!("{styled}");
}
