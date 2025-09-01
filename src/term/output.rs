use std::sync::OnceLock;

use console::style;

#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        if !cfg!(test) && $crate::term::output::is_debug() {
            $crate::term::output::print_hint("DEBUG", "blue");
            eprintln!($($arg)*);
        }
    };
}

#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        if !cfg!(test) {
            $crate::term::output::print_hint("==>", "green");
            eprintln!($($arg)*);
        }
    };
}

#[macro_export]
macro_rules! print {
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
            $crate::term::output::print_hint("WARNING", "yellow");
            eprintln!($($arg)*);
        }
    };
}

static DEBUG: OnceLock<bool> = OnceLock::new();

pub fn set_debug(debug: bool) {
    let _ = DEBUG.set(debug);
}

pub fn is_debug() -> bool {
    DEBUG.get().copied().unwrap_or(false)
}

static NO_STYLE: OnceLock<bool> = OnceLock::new();

pub fn set_no_style(no_style: bool) {
    let _ = NO_STYLE.set(no_style);
}

pub fn is_no_style() -> bool {
    NO_STYLE.get().copied().unwrap_or(false)
}

pub fn print_hint(hint: &str, color: &str) {
    if is_no_style() {
        eprint!("{hint} ");
        return;
    }
    let styled = match color {
        "blue" => style(hint).blue().bold(),
        "green" => style(hint).green().bold(),
        "yellow" => style(hint).yellow().bold(),
        _ => return,
    };
    eprint!("{styled} ");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_debug() {
        assert!(!is_debug());
        set_debug(true);
        assert!(is_debug());
        // Next set should not take effect
        set_debug(false);
        assert!(is_debug());
    }

    #[test]
    fn test_no_style() {
        assert!(!is_no_style());
        set_no_style(true);
        assert!(is_no_style());
        // Next set should not take effect
        set_no_style(false);
        assert!(is_no_style());
    }
}
