use std::env;
use std::fs::{self, Metadata, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::PathBuf;
use std::process::{self, Command, Stdio};
use std::time::{Duration, SystemTime};

use anyhow::{anyhow, bail, Context, Error, Result};
use chrono::{Local, LocalResult, TimeZone};
use console::{self, style, Term};
use dialoguer::theme::ColorfulTheme;
use dialoguer::Input;
use file_lock::{FileLock, FileOptions};
use pad::PadStr;
use regex::Regex;
use serde::Serialize;
use serde_json::ser::PrettyFormatter;
use serde_json::Serializer;

use crate::config;
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

pub fn open_url(url: impl AsRef<str>) -> Result<()> {
    open::that(url.as_ref()).with_context(|| {
        format!(
            "unable to open url {} in default browser",
            style(url.as_ref()).yellow()
        )
    })
}

pub fn confirm(msg: impl AsRef<str>) -> Result<bool> {
    let msg = format!(":: {}?", msg.as_ref());
    _ = write!(std::io::stderr(), "{} [Y/n] ", style(msg).bold());

    let mut answer = String::new();
    scanf::scanf!("{}", answer).context("confirm: Scan terminal stdin")?;
    if answer.to_lowercase() != "y" {
        return Ok(false);
    }

    return Ok(true);
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

pub fn edit_items(items: Vec<String>) -> Result<Vec<String>> {
    let content = items.join("\n");
    let content = edit_content(content.as_str(), "filter_names", true)?;
    Ok(content
        .split('\n')
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect())
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

pub fn confirm_items(
    items: &Vec<String>,
    action: &str,
    noun: &str,
    name: &str,
    plural: &str,
) -> Result<()> {
    if !confirm_items_weak(items, action, noun, name, plural)? {
        bail!(SilentExit { code: 60 });
    }
    Ok(())
}

pub fn confirm_items_weak(
    items: &Vec<String>,
    action: &str,
    noun: &str,
    name: &str,
    plural: &str,
) -> Result<bool> {
    if items.is_empty() {
        println!("Nothing to {action}");
        return Ok(false);
    }

    info!("Require confirm to {}", action);
    println!();

    let term = Term::stdout();
    let (_, col_size) = term.size();
    let col_size = col_size as usize;

    let name = if items.len() == 1 { name } else { plural };
    let head = format!("{} ({}): ", name, items.len());
    let head_size = console::measure_text_width(head.as_str());
    let head_space = " ".repeat(head_size);

    let mut current_size: usize = 0;
    for (idx, item) in items.iter().enumerate() {
        let item_size = console::measure_text_width(&item);
        if current_size == 0 {
            if idx == 0 {
                print!("{head}{item}");
            } else {
                print!("{head_space}{item}");
            }
            current_size = head_size + item_size;
            continue;
        }

        current_size += 2 + item_size;
        if current_size > col_size {
            println!();
            print!("{head_space}{item}");
            current_size = head_size + item_size;
            continue;
        }

        print!("  {item}");
    }
    println!("\n");

    println!(
        "Total {} {} to {}",
        items.len(),
        name.to_lowercase(),
        action
    );
    println!();
    let ok = confirm(format!("Proceed with {noun}"))?;
    if ok {
        println!();
    }
    Ok(ok)
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

pub fn show_json<T: Serialize>(value: T) -> Result<()> {
    let formatter = PrettyFormatter::with_indent(b"  ");
    let mut buf = Vec::new();
    let mut ser = Serializer::with_formatter(&mut buf, formatter);
    value.serialize(&mut ser).context("Serialize object")?;
    let json = String::from_utf8(buf).context("UTF8 encode json")?;
    println!("{json}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_format_time() {
        let now = config::now_secs();
        let time = format_time(now).unwrap();
        println!("{time}");
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
            Err(err) if err.kind() == ErrorKind::NotFound => {}
            Err(err) => panic!("{err}"),
        }
    }

    #[test]
    fn test_walk_dir() {
        let root = config::current_dir().clone();
        walk_dir(root, |path, meta| {
            println!(
                "Iter path: {}, IsDir: {}, IsFile: {}",
                path.display(),
                meta.is_dir(),
                meta.is_file()
            );
            Ok(true)
        })
        .unwrap();
    }

    #[test]
    fn test_show_json() {
        #[derive(Debug, Serialize)]
        struct TestInfo {
            pub name: String,
            pub age: i32,
            pub emails: Vec<String>,
        }

        let info = TestInfo {
            name: String::from("Rowlet"),
            age: 25,
            emails: vec![
                String::from("lazycat7706@gmail.com"),
                String::from("631029386@qq.com"),
            ],
        };
        show_json(&info).unwrap();
    }
}
