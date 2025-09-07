use std::fs;
use std::io::Write;
use std::sync::OnceLock;

use chrono::Local;

#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        if let Some(file) = $crate::term::output::get_debug() {
            $crate::term::output::write_debug(file, format!($($arg)*));
        }
    };
}

#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        if !cfg!(test) {
            let hint = console::style("==>").green().bold();
            eprint!("{hint} ");
            eprintln!($($arg)*);
        }
    };
}

#[macro_export]
macro_rules! output {
    ($($arg:tt)*) => {
        if !cfg!(test) {
            eprint!($($arg)*);
        }
    };
}

#[macro_export]
macro_rules! outputln {
    ($($arg:tt)*) => {
        if !cfg!(test) {
            eprintln!($($arg)*);
        }
    };
}

#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {
        if !cfg!(test) {
            let message = console::style(format!($($arg)*)).yellow();
            let hint = console::style("WARNING:").yellow().bold();
            eprintln!("{hint} {message}");
        }
    };
}

#[macro_export]
macro_rules! cursor_up {
    () => {
        if !cfg!(test) {
            eprint!("\x1b[A\x1b[K");
        }
    };
}

static DEBUG: OnceLock<String> = OnceLock::new();

pub fn set_debug(file: String) {
    let _ = DEBUG.set(file);
}

pub fn get_debug() -> Option<&'static String> {
    DEBUG.get()
}

pub fn write_debug(file: &str, msg: String) {
    let time = Local::now();
    let time_str = time.format("%Y-%m-%d %H:%M:%S").to_string();

    let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(file) else {
        warn!("Failed to open debug file: {file}");
        return;
    };

    if let Err(e) = file.write_all(format!("{time_str} - {msg}\n").as_bytes()) {
        warn!("Failed to write debug info: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_debug() {
        let file = "tests/debug.log";
        let _ = fs::remove_file(file);
        assert!(get_debug().is_none());
        set_debug(String::from(file));
        assert_eq!(get_debug(), Some(&String::from(file)));
        // Next set should not take effect
        set_debug(String::from("tests/new_debug.log"));
        assert_eq!(get_debug(), Some(&String::from(file)));

        debug!("This is a test debug message.");
        let content = fs::read_to_string(file).unwrap();
        assert!(content.contains("This is a test debug message."));
    }
}
