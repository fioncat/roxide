use std::env;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::PathBuf;
use std::process;
use std::time::{Duration, SystemTime};

use anyhow::{bail, Context, Error, Result};
use chrono::{Local, LocalResult, TimeZone};
use console::{self, style};
use dialoguer::theme::ColorfulTheme;
use dialoguer::{Editor, Input};
use pad::PadStr;

use crate::errors::SilentExit;

pub const SECOND: u64 = 1;
pub const MINUTE: u64 = 60 * SECOND;
pub const HOUR: u64 = 60 * MINUTE;
pub const DAY: u64 = 24 * HOUR;
pub const WEEK: u64 = 7 * DAY;

#[macro_export]
macro_rules! confirm {
    ($dst:expr $(,)?) => {
        $crate::utils::must_confirm($dst)?;
    };
    ($fmt:expr, $($arg:tt)*) => {
        let msg = format!($fmt, $($arg)*);
        $crate::utils::must_confirm(msg.as_str())?;
    };
}

#[macro_export]
macro_rules! exec {
    ($dst:expr $(,)?) => {
        {
            $crate::utils::show_exec($dst);
        }
    };
    ($fmt:expr, $($arg:tt)*) => {
        {
            let msg = format!($fmt, $($arg)*);
            $crate::utils::show_exec(msg.as_str());
        }
    };
}

#[macro_export]
macro_rules! info {
    ($dst:expr $(,)?) => {
        {
            $crate::utils::show_info($dst);
        }
    };
    ($fmt:expr, $($arg:tt)*) => {
        {
            let msg = format!($fmt, $($arg)*);
            $crate::utils::show_info(msg.as_str());
        }
    };
}

pub fn show_exec(msg: impl AsRef<str>) {
    let msg = format!("{} {}", style("==>").cyan(), msg.as_ref());
    write_stderr(msg);
}

pub fn show_info(msg: impl AsRef<str>) {
    let msg = format!("{} {}", style("==>").green(), msg.as_ref());
    write_stderr(msg);
}

pub fn write_stderr(msg: String) {
    _ = writeln!(std::io::stderr(), "{}", msg);
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

pub fn open_url(url: impl AsRef<str>) -> Result<()> {
    open::that(url.as_ref()).with_context(|| {
        format!(
            "unable to open url {} in default browser",
            style(url.as_ref()).yellow()
        )
    })
}

pub fn confirm(msg: impl AsRef<str>) -> Result<bool> {
    let result = dialoguer::Confirm::with_theme(&dialoguer::theme::ColorfulTheme::default())
        .with_prompt(msg.as_ref())
        .interact_on(&console::Term::stderr());
    match result {
        Ok(ok) => Ok(ok),
        Err(err) => Err(err).context("Terminal confirm"),
    }
}

pub fn must_confirm(msg: impl AsRef<str>) -> Result<()> {
    let ok = confirm(msg)?;
    if !ok {
        bail!(SilentExit { code: 60 });
    }
    Ok(())
}

pub fn input(msg: impl AsRef<str>, require: bool, default: Option<&str>) -> Result<String> {
    let theme = ColorfulTheme::default();
    let mut input: Input<String> = Input::with_theme(&theme);
    input.with_prompt(msg.as_ref());
    if let Some(default) = default {
        input.with_initial_text(default.to_string());
    }
    let text = input.interact_text().context("Terminal input")?;
    if require && text.is_empty() {
        bail!("Input cannot be empty");
    }
    Ok(text)
}

pub fn edit<S>(default: S, ext: S, require: bool) -> Result<String>
where
    S: AsRef<str>,
{
    let text = Editor::new()
        .executable(ext.as_ref())
        .edit(default.as_ref())
        .context("Terminal edit")?;
    match text {
        Some(text) => Ok(text),
        None => {
            if require {
                bail!("Edit cannot be empty");
            }
            Ok(String::new())
        }
    }
}

pub struct Table {
    ncol: usize,
    rows: Vec<Vec<String>>,
}

impl Table {
    pub fn with_capacity(size: usize) -> Table {
        Table {
            ncol: 0,
            rows: Vec::with_capacity(size),
        }
    }

    pub fn add(&mut self, row: Vec<String>) {
        if row.is_empty() {
            panic!("Empty row");
        }
        if self.ncol == 0 {
            self.ncol = row.len();
        } else if row.len() != self.ncol {
            panic!("Unexpect row len");
        }
        self.rows.push(row);
    }

    pub fn show(self) {
        let mut pads = Vec::with_capacity(self.ncol);
        for i in 0..self.ncol {
            let mut max_len: usize = 0;
            for row in self.rows.iter() {
                let cell = row.get(i).unwrap();
                if cell.len() > max_len {
                    max_len = cell.len()
                }
            }
            pads.push(max_len + 2);
        }

        for row in self.rows.into_iter() {
            for (i, cell) in row.into_iter().enumerate() {
                let pad = pads[i];
                let cell = cell.pad_to_width_with_alignment(pad, pad::Alignment::Left);
                print!("{}", cell);
            }
            println!()
        }
    }
}

pub fn format_time(time: u64) -> Result<String> {
    match Local.timestamp_opt(time as i64, 0) {
        LocalResult::None => bail!("Invalid timestamp {time}"),
        LocalResult::Ambiguous(_, _) => bail!("Ambiguous parse timestamp {time}"),
        LocalResult::Single(time) => Ok(time.format("%Y-%m-%d %H:%M:%S").to_string()),
    }
}
