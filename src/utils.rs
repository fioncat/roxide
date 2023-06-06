use std::env;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::PathBuf;
use std::process;
use std::time::{Duration, SystemTime};

use anyhow::{Context, Error, Result};
use console::{self, style};

use crate::errors::SilentExit;

pub const SECOND: u64 = 1;
pub const MINUTE: u64 = 60 * SECOND;
pub const HOUR: u64 = 60 * MINUTE;
pub const DAY: u64 = 24 * HOUR;
pub const WEEK: u64 = 7 * DAY;

#[macro_export]
macro_rules! confirm {
    ($fmt:expr, $($arg:tt)*) => {
        let msg = format!($fmt, $($arg)*);
        let result = dialoguer::Confirm::with_theme(&dialoguer::theme::ColorfulTheme::default())
            .with_prompt(msg)
            .interact_on(&console::Term::stderr());
        match result {
            Ok(ok) => {
                if !ok {
                    anyhow::bail!($crate::errors::SilentExit{ code: 60 });
                }
            }
            Err(err) => anyhow::bail!("Confirm error: {err:#}"),
        };
    };
}

#[macro_export]
macro_rules! show_exec {
    ($fmt:expr, $($arg:tt)*) => {
        {
            let msg = format!($fmt, $($arg)*);
            let msg = format!("{} {}", console::style("==>").cyan(),
                console::style(msg).bold());
            _ = writeln!(std::io::stderr(), "{}", msg);
        }
    };
}

#[macro_export]
macro_rules! info {
    ($fmt:expr, $($arg:tt)*) => {
        {
            let msg = format!($fmt, $($arg)*);
            let msg = format!("{} {}", console::style("==>").green(), msg);
            _ = writeln!(std::io::stderr(), "{}", msg);
        }
    };
}

pub fn ensure_dir(path: &PathBuf) -> Result<()> {
    if let Some(dir) = path.parent() {
        match fs::read_dir(dir) {
            Ok(_) => Ok(()),
            Err(err) if err.kind() == ErrorKind::NotFound => {
                fs::create_dir_all(dir)
                    .with_context(|| format!("Create directory {}", dir.display()))?;
                Ok(())
            }
            Err(err) => Err(err).with_context(|| format!("Read directory {}", dir.display())),
        }
    } else {
        Ok(())
    }
}

pub fn write_file(path: &PathBuf, data: &[u8]) -> Result<()> {
    ensure_dir(path)?;
    let mut opts = OpenOptions::new();
    opts.create(true).truncate(true).write(true);
    let mut file = opts
        .open(path)
        .with_context(|| format!("Open file {}", path.display()))?;
    file.write_all(data)
        .with_context(|| format!("Write file {}", path.display()))?;
    Ok(())
}

pub fn current_time() -> Result<Duration> {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .context("System clock set to invalid time")
}

pub fn current_secs() -> Result<u64> {
    Ok(current_time()?.as_secs())
}

pub fn current_dir() -> Result<PathBuf> {
    env::current_dir().context("Get current directory")
}

pub fn error_exit(err: Error) {
    _ = writeln!(std::io::stderr(), "{}: {err:#}", style("error").red());
    process::exit(2);
}

pub fn handle_result(result: Result<()>) {
    match result {
        Ok(()) => {}
        Err(err) => match err.downcast::<SilentExit>() {
            Ok(SilentExit { code }) => process::exit(code as _),
            Err(err) => error_exit(err),
        },
    }
}

pub fn parse_query(query: impl AsRef<str>) -> (String, String) {
    let items: Vec<_> = query.as_ref().split("/").collect();
    let items_len = items.len();
    let mut group_buffer: Vec<String> = Vec::with_capacity(items_len - 1);
    let mut base = "";
    for (idx, item) in items.iter().enumerate() {
        if idx == items_len - 1 {
            base = item;
        } else {
            group_buffer.push(item.to_string());
        }
    }
    (group_buffer.join("/"), base.to_string())
}
