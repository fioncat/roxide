use std::collections::HashMap;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::PathBuf;
use std::process::{self, Command, Stdio};
use std::time::{Duration, SystemTime};

use anyhow::{anyhow, bail, Context, Error, Result};
use chrono::{Local, LocalResult, TimeZone};
use console::{self, style};
use dialoguer::theme::ColorfulTheme;
use dialoguer::Input;
use file_lock::{FileLock, FileOptions};
use pad::PadStr;
use regex::Regex;

use crate::config;
use crate::config::types::Remote;
use crate::errors::SilentExit;

pub const SECOND: u64 = 1;
pub const MINUTE: u64 = 60 * SECOND;
pub const HOUR: u64 = 60 * MINUTE;
pub const DAY: u64 = 24 * HOUR;
pub const WEEK: u64 = 7 * DAY;
pub const MONTH: u64 = 30 * DAY;
pub const YEAR: u64 = 365 * DAY;

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

pub fn parse_query(remote: &Remote, query: impl AsRef<str>) -> (String, String) {
    let (mut owner, mut name) = parse_query_raw(query);
    if let Some(raw_owner) = remote.owner_alias.get(owner.as_str()) {
        owner = raw_owner.clone();
    }
    if let Some(raw_name) = remote.repo_alias.get(name.as_str()) {
        name = raw_name.clone();
    }
    (owner, name)
}

fn parse_query_raw(query: impl AsRef<str>) -> (String, String) {
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

pub fn revert_map(map: &HashMap<String, String>) -> HashMap<String, String> {
    map.iter()
        .map(|(key, value)| (value.clone(), key.clone()))
        .collect()
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

pub fn edit_content<S>(raw: S, name: S, require: bool) -> Result<String>
where
    S: AsRef<str>,
{
    let editor = get_editor()?;
    let edit_path = PathBuf::from(&config::base().metadir)
        .join("tmp")
        .join(name.as_ref());
    if !raw.as_ref().is_empty() {
        let data = raw.as_ref().to_string();
        write_file(&edit_path, data.as_bytes())?;
    }
    edit_file(&editor, &edit_path)?;

    let data = fs::read(&edit_path).context("Read edit file")?;
    fs::remove_file(&edit_path).context("Remove edit file")?;
    let content = String::from_utf8(data).context("Decode edit content as utf-8")?;
    if require && content.is_empty() {
        bail!("Require edit content");
    }

    Ok(content)
}

pub fn edit_file(editor: impl AsRef<str>, path: &PathBuf) -> Result<()> {
    ensure_dir(path)?;
    let mut cmd = Command::new(editor.as_ref());
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());
    cmd.stdin(Stdio::inherit());
    cmd.arg(&path);
    cmd.output()
        .with_context(|| format!("Use editor {} to edit {}", editor.as_ref(), path.display()))?;
    Ok(())
}

pub fn get_editor() -> Result<String> {
    let editor = env::var("EDITOR").context("Get EDITOR env")?;
    if editor.is_empty() {
        bail!("Could not get your default editor, please check env `EDITOR`");
    }
    Ok(editor)
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

pub fn format_since(time: u64) -> String {
    let duration = config::now_secs().saturating_sub(time);

    let unit: &str;
    let value: u64;
    if duration < MINUTE {
        unit = "second";
        if duration < 30 {
            return String::from("now");
        }
        value = duration;
    } else if duration < HOUR {
        unit = "minute";
        value = duration / MINUTE;
    } else if duration < DAY {
        unit = "hour";
        value = duration / HOUR;
    } else if duration < WEEK {
        unit = "day";
        value = duration / DAY;
    } else if duration < MONTH {
        unit = "week";
        value = duration / WEEK;
    } else if duration < YEAR {
        unit = "month";
        value = duration / MONTH;
    } else {
        unit = "year";
        value = duration / YEAR;
    }

    if value > 1 {
        format!("{value} {unit}s ago")
    } else {
        format!("last {unit}")
    }
}

pub fn format_time(time: u64) -> Result<String> {
    match Local.timestamp_opt(time as i64, 0) {
        LocalResult::None => bail!("Invalid timestamp {time}"),
        LocalResult::Ambiguous(_, _) => bail!("Ambiguous parse timestamp {time}"),
        LocalResult::Single(time) => Ok(time.format("%Y-%m-%d %H:%M:%S").to_string()),
    }
}

pub fn parse_duration_secs(s: impl AsRef<str>) -> Result<u64> {
    let parse_err = format!(
        "Invalid duration {}, the format should be <number><s|m|h|d>",
        style(s.as_ref()).yellow()
    );

    const DURATION_REGEX: &str = r"^(\d+)([s|m|h|d])$";
    let re = Regex::new(DURATION_REGEX).expect("parse duration regex");
    let mut iter = re.captures_iter(s.as_ref());
    let caps = match iter.next() {
        Some(caps) => caps,
        None => bail!(parse_err),
    };

    // We have 3 captures:
    //   0 -> string itself
    //   1 -> number
    //   2 -> unit
    if caps.len() != 3 {
        bail!(parse_err);
    }
    let number = match caps.get(1) {
        Some(num) => num
            .as_str()
            .parse::<u64>()
            .with_context(|| format!("Invalid duration number {}", style(num.as_str()).yellow()))?,
        None => bail!("Missing number in duration"),
    };
    if number == 0 {
        bail!("duration cannot be zero");
    }

    let unit = match caps.get(2) {
        Some(unit) => unit.as_str(),
        None => bail!("Missing unit in duration"),
    };

    let secs = match unit {
        "s" => number,
        "m" => number * MINUTE,
        "h" => number * HOUR,
        "d" => number * DAY,
        _ => bail!("Invalid unit {}", unit),
    };

    Ok(secs)
}

pub fn remove_dir_recursively(path: PathBuf) -> Result<()> {
    info!("Remove dir {}", path.display());
    fs::remove_dir_all(&path).context("Remove directory")?;

    let dir = path.parent();
    if let None = dir {
        return Ok(());
    }
    let mut dir = dir.unwrap();
    loop {
        match fs::read_dir(dir) {
            Ok(dir_read) => {
                let count = dir_read.count();
                if count > 0 {
                    return Ok(());
                }
                info!("Remove dir {}", dir.display());
                fs::remove_dir(dir).context("Remove directory")?;
                match dir.parent() {
                    Some(parent) => dir = parent,
                    None => return Ok(()),
                }
            }
            Err(err) if err.kind() == ErrorKind::NotFound => return Ok(()),
            Err(err) => return Err(err).with_context(|| format!("Read dir {}", dir.display())),
        }
    }
}

pub fn human_bytes<T: Into<f64>>(bytes: T) -> String {
    const BYTES_UNIT: f64 = 1000.0;
    const BYTES_SUFFIX: [&str; 9] = ["B", "KB", "MB", "GB", "TB", "PB", "EB", "ZB", "YB"];
    let size = bytes.into();
    if size <= 0.0 {
        return String::from("0B");
    }

    let base = size.log10() / BYTES_UNIT.log10();
    let result = format!("{:.1}", BYTES_UNIT.powf(base - base.floor()))
        .trim_end_matches(".0")
        .to_owned();

    [&result, BYTES_SUFFIX[base.floor() as usize]].join("")
}

pub fn dir_size(dir: PathBuf) -> Result<String> {
    let mut stack = vec![dir];
    let mut total_size: u64 = 0;
    loop {
        let maybe_dir = stack.pop();
        if let None = maybe_dir {
            return Ok(human_bytes(total_size as f64));
        }
        let current_dir = maybe_dir.unwrap();

        let read_dir = match fs::read_dir(&current_dir) {
            Ok(read_dir) => read_dir,
            Err(err) if err.kind() == ErrorKind::NotFound => continue,
            Err(err) => {
                return Err(err)
                    .with_context(|| format!("Read directory {}", current_dir.display()))
            }
        };

        for item in read_dir {
            let item =
                item.with_context(|| format!("Read directory item for {}", current_dir.display()))?;
            let path = item.path();
            let meta = item
                .metadata()
                .with_context(|| format!("Get metadata for {}", path.display()))?;
            if meta.is_file() {
                let size = meta.len();
                total_size += size;
                continue;
            }
            if meta.is_dir() {
                stack.push(path);
            }
        }
    }
}

pub struct Lock {
    path: PathBuf,
    _file_lock: FileLock,
}

impl Lock {
    const RESOURCE_TEMPORARILY_UNAVAILABLE_CODE: i32 = 11;

    pub fn acquire(name: impl AsRef<str>) -> Result<Lock> {
        let dir = PathBuf::from(config::base().metadir.as_str());
        ensure_dir(&dir)?;
        let path = dir.join(format!("lock_{}", name.as_ref()));

        let lock_opts = FileOptions::new().write(true).create(true).truncate(true);
        let mut file_lock = match FileLock::lock(&path, false, lock_opts) {
            Ok(lock) => lock,
            Err(err) => match err.raw_os_error() {
                Some(code) if code == Self::RESOURCE_TEMPORARILY_UNAVAILABLE_CODE => {
                    bail!("Acquire file lock error, {} is occupied by another roxide, please wait for it to complete", name.as_ref());
                }
                _ => {
                    return Err(err).with_context(|| format!("Acquire file lock {}", name.as_ref()))
                }
            },
        };

        let pid = process::id();
        let pid = format!("{pid}");

        file_lock
            .file
            .write_all(pid.as_bytes())
            .with_context(|| format!("Write pid to lock file {}", path.display()))?;
        file_lock
            .file
            .flush()
            .with_context(|| format!("Flush pid to lock file {}", path.display()))?;

        Ok(Lock {
            path,
            _file_lock: file_lock,
        })
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        match fs::remove_file(&self.path) {
            Ok(_) => {}
            Err(err) => error_exit(anyhow!(
                "Delete lock file {} error: {err}",
                self.path.display()
            )),
        }
    }
}
