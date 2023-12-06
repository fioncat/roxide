#![allow(dead_code)]

use std::io::{self, Write};
use std::path::PathBuf;
use std::{fs, process};

use anyhow::{bail, Context, Error, Result};
use chrono::{Local, LocalResult, TimeZone};
use console::{self, style};
use regex::Regex;

use crate::config::Config;
use crate::errors::SilentExit;
use crate::info;

#[cfg(test)]
#[macro_export]
macro_rules! vec_strings {
    ( $( $s:expr ),* ) => {
        vec![
            $(
                String::from($s),
            )*
        ]
    };
}

#[cfg(test)]
#[macro_export]
macro_rules! hashset {
    ( $( $x:expr ),* ) => {
        {
            let mut set = HashSet::new();
            $(
                set.insert($x);
            )*
            set
        }
    };
}

#[cfg(test)]
#[macro_export]
macro_rules! hashset_strings {
    ( $( $x:expr ),* ) => {
        {
            let mut set = HashSet::new();
            $(
                set.insert(String::from($x));
            )*
            set
        }
    };
}

#[cfg(test)]
#[macro_export]
macro_rules! hashmap {
    ($($key:expr => $value:expr),*) => {{
        let mut map = HashMap::new();
        $(map.insert($key, $value);)*
        map
    }};
}

#[cfg(test)]
#[macro_export]
macro_rules! hashmap_strings {
    ($($key:expr => $value:expr),*) => {{
        let mut map = HashMap::new();
        $(map.insert(String::from($key), String::from($value));)*
        map
    }};
}

pub const SECOND: u64 = 1;
pub const MINUTE: u64 = 60 * SECOND;
pub const HOUR: u64 = 60 * MINUTE;
pub const DAY: u64 = 24 * HOUR;
pub const WEEK: u64 = 7 * DAY;
pub const MONTH: u64 = 30 * DAY;
pub const YEAR: u64 = 365 * DAY;

/// If the file directory doesn't exist, create it; if it exists, take no action.
pub fn ensure_dir(path: &PathBuf) -> Result<()> {
    if let Some(dir) = path.parent() {
        match fs::read_dir(dir) {
            Ok(_) => Ok(()),
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                fs::create_dir_all(dir)
                    .with_context(|| format!("create directory {}", dir.display()))?;
                Ok(())
            }
            Err(err) => Err(err).with_context(|| format!("read directory {}", dir.display())),
        }
    } else {
        Ok(())
    }
}

/// Write the content to a file; if the file doesn't exist, create it. If the
/// directory where the file is located doesn't exist, create it as well.
pub fn write_file(path: &PathBuf, data: &[u8]) -> Result<()> {
    ensure_dir(path)?;
    let mut opts = fs::OpenOptions::new();
    opts.create(true).truncate(true).write(true);
    let mut file = opts
        .open(path)
        .with_context(|| format!("open file {}", path.display()))?;
    file.write_all(data)
        .with_context(|| format!("write file {}", path.display()))?;
    Ok(())
}

/// UNIX file lock are utilized to lock an entire process during an operation,
/// enabling certain process-level atomic operations. Once a process acquires a file
/// lock, any attempts by other identical processes to acquire the lock will fail.
/// There's no need for manual release of the file lock; it automatically releases
/// upon object release.
///
/// See: [`file_lock`] and [`file_lock::FileLock`].
pub struct FileLock {
    _path: PathBuf,
    /// Wrap the `file_lock` crate
    _file_lock: file_lock::FileLock,
}

impl FileLock {
    const RESOURCE_TEMPORARILY_UNAVAILABLE_CODE: i32 = 11;

    /// Attempt to acquire the file lock; this function will fail if there are
    /// issues with the filesystem or if another process has already acquired the
    /// lock. We will create a `lock_{name}` file lock under the metadir directory,
    /// which will store the current process's PID.
    ///
    /// # Arguments
    ///
    /// * `cfg` - We will create file lock under `cfg.metadir`.
    /// * `name` - File lock name, you can use this to create locks at different
    /// granularity to lock different processes.
    pub fn acquire(cfg: &Config, name: impl AsRef<str>) -> Result<FileLock> {
        let path = cfg.get_meta_dir().join(format!("lock_{}", name.as_ref()));
        ensure_dir(&path)?;

        let lock_opts = file_lock::FileOptions::new()
            .write(true)
            .create(true)
            .truncate(true);
        let mut file_lock = match file_lock::FileLock::lock(&path, false, lock_opts) {
            Ok(lock) => lock,
            Err(err) => match err.raw_os_error() {
                Some(code) if code == Self::RESOURCE_TEMPORARILY_UNAVAILABLE_CODE => {
                    bail!("acquire file lock error, {} is occupied by another roxide, please wait for it to complete", name.as_ref());
                }
                _ => {
                    return Err(err).with_context(|| format!("acquire file lock {}", name.as_ref()))
                }
            },
        };

        // Write current pid to file lock.
        let pid = process::id();
        let pid = format!("{pid}");

        file_lock
            .file
            .write_all(pid.as_bytes())
            .with_context(|| format!("write pid to lock file {}", path.display()))?;
        file_lock
            .file
            .flush()
            .with_context(|| format!("flush pid to lock file {}", path.display()))?;

        // The file lock will be released after file_lock dropped.
        Ok(FileLock {
            _path: path,
            _file_lock: file_lock,
        })
    }
}

/// See: [`shellexpand::full`].
pub fn expandenv(s: impl AsRef<str>) -> Result<String> {
    let s = shellexpand::full(s.as_ref())
        .with_context(|| format!("expand env for '{}'", s.as_ref()))?;
    Ok(s.to_string())
}

/// Exit the process with error.
pub fn error_exit(err: Error) {
    _ = writeln!(io::stderr(), "{}: {err:#}", style("error").red());
    process::exit(2);
}

/// If there are no errors, exit zero. If there is an error, print the error and
/// exit with none-zero.
pub fn handle_result(result: Result<()>) {
    match result {
        Ok(()) => {}
        Err(err) => match err.downcast::<SilentExit>() {
            Ok(SilentExit { code }) => process::exit(code as _),
            Err(err) => error_exit(err),
        },
    }
}

/// Open url in default web browser, see: [`open`].
pub fn open_url(url: impl AsRef<str>) -> Result<()> {
    open::that(url.as_ref()).with_context(|| {
        format!(
            "unable to open url {} in default browser",
            style(url.as_ref()).yellow()
        )
    })
}

/// Return the duration in a human-readable form from the current time to `time`.
pub fn format_since(cfg: &Config, time: u64) -> String {
    let duration = cfg.now().saturating_sub(time);

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

/// Convert a Unix timestamp to a human-readable time.
pub fn format_time(time: u64) -> Result<String> {
    match Local.timestamp_opt(time as i64, 0) {
        LocalResult::None => bail!("invalid timestamp {time}"),
        LocalResult::Ambiguous(_, _) => bail!("ambiguous parse timestamp {time}"),
        LocalResult::Single(time) => Ok(time.format("%Y-%m-%d %H:%M:%S").to_string()),
    }
}

/// Convert the user-provided duration string to seconds. The basic format for
/// duration is: `{number}{unit}`.
///
/// Available units:
///
/// - `s`: seconds.
/// - `m`: minutes.
/// - `h`: hours.
/// - `d`: days.
///
/// `{number}` must be a positive integer, and higher or lower granularity is not
/// supported.
///
/// # Examples
///
/// ```
/// assert_eq!(parse_duration_secs("32s").unwrap(), 32)
/// assert_eq!(parse_duration_secs("2m").unwrap(), 120)
/// assert_eq!(parse_duration_secs("1h").unwrap(), 3600)
/// assert_eq!(parse_duration_secs("2d").unwrap(), 3600*24*2)
/// ```
pub fn parse_duration_secs(s: impl AsRef<str>) -> Result<u64> {
    let parse_err = format!(
        "invalid duration '{}', the format should be <number><s|m|h|d>",
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
        Some(num) => num.as_str().parse::<u64>().with_context(|| {
            format!("invalid duration number '{}'", style(num.as_str()).yellow())
        })?,
        None => bail!("missing number in duration"),
    };
    if number == 0 {
        bail!("duration cannot be zero");
    }

    let unit = match caps.get(2) {
        Some(unit) => unit.as_str(),
        None => bail!("missing unit in duration"),
    };

    let secs = match unit {
        "s" => number,
        "m" => number * MINUTE,
        "h" => number * HOUR,
        "d" => number * DAY,
        _ => bail!("invalid unit '{}'", unit),
    };

    Ok(secs)
}

/// Recursively walk all entries in a directory and call the provided `handle`
/// function for each entry.
pub fn walk_dir<F>(root: PathBuf, mut handle: F) -> Result<()>
where
    F: FnMut(&PathBuf, fs::Metadata) -> Result<bool>,
{
    let mut stack = vec![root];
    while !stack.is_empty() {
        let dir = stack.pop().unwrap();
        let dir_read = match fs::read_dir(&dir) {
            Ok(dir_read) => dir_read,
            Err(err) if err.kind() == io::ErrorKind::NotFound => continue,
            Err(err) => return Err(err).with_context(|| format!("read dir '{}'", dir.display())),
        };
        for entry in dir_read.into_iter() {
            let entry = entry.with_context(|| format!("read sub dir for '{}'", dir.display()))?;
            let sub = dir.join(entry.file_name());
            let meta = entry
                .metadata()
                .with_context(|| format!("read metadata for '{}'", sub.display()))?;
            let is_dir = meta.is_dir();
            let next = handle(&sub, meta)?;
            if next && is_dir {
                stack.push(sub);
            }
        }
    }
    Ok(())
}

/// Recursively traverse the entire directory and return the size of the entire
/// directory.
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

/// Convert a size to a human-readable string, for example, "32KB".
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

/// If the length of `vec` is less than or equal to 1, return `name` itself; if
/// greater than 1, return the plural form of `name`.
pub fn plural<T>(vec: &Vec<T>, name: &str) -> String {
    let plural = format!("{name}s");
    plural_full(vec, name, plural.as_str())
}

/// Similar to [`plural`] but allows manually specifying the plural word. Suitable
/// for special plural scenarios.
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

/// Remove a directory, recursively deleting until reaching a non-empty parent
/// directory.
pub fn remove_dir_recursively(path: PathBuf) -> Result<()> {
    match fs::read_dir(&path) {
        Ok(_) => {}
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err).with_context(|| format!("read repo dir '{}'", path.display())),
    }
    info!("Remove dir {}", path.display());
    fs::remove_dir_all(&path).context("remove directory")?;

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
                fs::remove_dir(dir).context("remove directory")?;
                match dir.parent() {
                    Some(parent) => dir = parent,
                    None => return Ok(()),
                }
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(err) => return Err(err).with_context(|| format!("read dir '{}'", dir.display())),
        }
    }
}

#[cfg(test)]
mod utils_tests {
    use crate::config::config_tests;
    use crate::utils::*;

    #[test]
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

        let cfg = config_tests::load_test_config("utils/format_since");
        for (secs, expect) in cases {
            let time = cfg.now().saturating_sub(secs);
            let format = format_since(&cfg, time);
            if expect != format.as_str() {
                panic!("Expect {expect}, Found {format}");
            }
        }
    }

    #[test]
    fn test_format_time() {
        let cfg = config_tests::load_test_config("utils/format_time");
        let now = cfg.now();
        let time = format_time(now).unwrap();
        assert!(!time.is_empty());
    }

    #[test]
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
    fn test_remove_dir_recursively() {
        const PATH: &str = "/tmp/test-roxide/sub01/sub02/sub03";
        fs::create_dir_all(PATH).unwrap();
        remove_dir_recursively(PathBuf::from(PATH)).unwrap();

        match fs::read_dir(PATH) {
            Ok(_) => panic!("Expect path {PATH} be deleted, but it is still exists"),
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => panic!("{err}"),
        }
    }

    #[test]
    fn test_walk_dir() {
        let cfg = config_tests::load_test_config("utils/format_time");
        let path = cfg.get_current_dir().clone();
        walk_dir(path, |_path, _meta| Ok(true)).unwrap();
    }
}
