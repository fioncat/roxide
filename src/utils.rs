use std::env;
use std::fs::{self, Metadata, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::PathBuf;
use std::process;
use std::time::{Duration, SystemTime};

use anyhow::{anyhow, bail, Context, Error, Result};
use chrono::{Local, LocalResult, TimeZone};
use console::{self, style};
use file_lock::{FileLock, FileOptions};
use regex::Regex;

use crate::config;
use crate::errors::SilentExit;
use crate::info;

pub const SECOND: u64 = 1;
pub const MINUTE: u64 = 60 * SECOND;
pub const HOUR: u64 = 60 * MINUTE;
pub const DAY: u64 = 24 * HOUR;
pub const WEEK: u64 = 7 * DAY;
pub const MONTH: u64 = 30 * DAY;
pub const YEAR: u64 = 365 * DAY;

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

pub fn open_url(url: impl AsRef<str>) -> Result<()> {
    open::that(url.as_ref()).with_context(|| {
        format!(
            "unable to open url {} in default browser",
            style(url.as_ref()).yellow()
        )
    })
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
    match fs::read_dir(&path) {
        Ok(_) => {}
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err).with_context(|| format!("Read repo dir {}", path.display())),
    }
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

pub fn human_bytes<T: Into<u64>>(bytes: T) -> String {
    const BYTES_UNIT: f64 = 1000.0;
    const BYTES_SUFFIX: [&str; 9] = ["B", "KB", "MB", "GB", "TB", "PB", "EB", "ZB", "YB"];
    let size = bytes.into();
    let size = size as f64;
    if size <= 0.0 {
        return String::from("0B");
    }

    let base = size.log10() / BYTES_UNIT.log10();
    let result = format!("{:.1}", BYTES_UNIT.powf(base - base.floor()))
        .trim_end_matches(".0")
        .to_owned();

    [&result, BYTES_SUFFIX[base.floor() as usize]].join("")
}

pub fn dir_size(dir: PathBuf) -> Result<u64> {
    let mut total_size: u64 = 0;
    walk_dir(dir, |_path, meta| {
        if meta.is_file() {
            total_size += meta.len();
        }
        Ok(true)
    })?;

    Ok(total_size)
}

pub struct Lock {
    path: PathBuf,
    _file_lock: FileLock,
}

impl Lock {
    const RESOURCE_TEMPORARILY_UNAVAILABLE_CODE: i32 = 11;

    pub fn acquire(name: impl AsRef<str>) -> Result<Lock> {
        let dir = PathBuf::from(config::base().metadir.as_str());
        let path = dir.join(format!("lock_{}", name.as_ref()));
        ensure_dir(&path)?;

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

pub fn plural<T>(vec: &Vec<T>, name: &str) -> String {
    let plural = format!("{name}s");
    plural_full(vec, name, plural.as_str())
}

pub fn plural_full<T>(vec: &Vec<T>, name: &str, plural: &str) -> String {
    if vec.is_empty() {
        return format!("0 {}", name);
    }
    if vec.len() == 1 {
        format!("1 {}", name)
    } else {
        format!("{} {}", vec.len(), plural)
    }
}

pub fn walk_dir<F>(root: PathBuf, mut handle: F) -> Result<()>
where
    F: FnMut(&PathBuf, Metadata) -> Result<bool>,
{
    let mut stack = vec![root];
    while !stack.is_empty() {
        let dir = stack.pop().unwrap();
        let dir_read = match fs::read_dir(&dir) {
            Ok(dir_read) => dir_read,
            Err(err) if err.kind() == ErrorKind::NotFound => continue,
            Err(err) => return Err(err).with_context(|| format!("Read dir {}", dir.display())),
        };
        for entry in dir_read.into_iter() {
            let entry = entry.with_context(|| format!("Read subdir for {}", dir.display()))?;
            let sub = dir.join(entry.file_name());
            let meta = entry
                .metadata()
                .with_context(|| format!("Read metadata for {}", sub.display()))?;
            let is_dir = meta.is_dir();
            let next = handle(&sub, meta)?;
            if next && is_dir {
                stack.push(sub);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serial_test::serial;

    use super::*;

    #[test]
    #[serial]
    fn test_format_since() {
        let cases = [
            (10, "now"),
            (30, "30 seconds ago"),
            (60, "last minute"),
            (120, "2 minutes ago"),
            (DAY, "last day"),
            (3 * DAY, "3 days ago"),
            (WEEK, "last week"),
            (3 * WEEK, "3 weeks ago"),
            (MONTH, "last month"),
            (10 * MONTH, "10 months ago"),
            (YEAR, "last year"),
            (10 * YEAR, "10 years ago"),
        ];

        let now = config::now_secs();
        for (secs, expect) in cases {
            let time = now.saturating_sub(secs);
            let format = format_since(time);
            if expect != format.as_str() {
                panic!("Expect {expect}, Found {format}");
            }
        }
    }

    #[test]
    #[serial]
    fn test_format_time() {
        let now = config::now_secs();
        let time = format_time(now).unwrap();
        println!("{time}");
    }

    #[test]
    #[serial]
    fn test_parse_duration() {
        let cases = [
            ("3s", 3),
            ("120s", 120),
            ("450s", 450),
            ("3m", 3 * MINUTE),
            ("120m", 120 * MINUTE),
            ("12h", 12 * HOUR),
            ("30d", 30 * DAY),
        ];

        for (str, expect) in cases {
            let result = parse_duration_secs(str).unwrap();
            if result != expect {
                panic!("Expect {expect}, Found {result}");
            }
        }
    }

    #[test]
    #[serial]
    fn test_remove_dir_recursively() {
        const PATH: &str = "/tmp/test-roxide/sub01/sub02/sub03";
        fs::create_dir_all(PATH).unwrap();
        remove_dir_recursively(PathBuf::from(PATH)).unwrap();

        match fs::read_dir(PATH) {
            Ok(_) => panic!("Expect path {PATH} be deleted, but it is still exists"),
            Err(err) if err.kind() == ErrorKind::NotFound => {}
            Err(err) => panic!("{err}"),
        }
    }

    #[test]
    #[serial]
    fn test_walk_dir() {
        let root = config::current_dir().clone();
        walk_dir(root, |_path, _meta| Ok(true)).unwrap();
    }
}
