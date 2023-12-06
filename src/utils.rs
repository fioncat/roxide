#![allow(dead_code)]

use std::io::{self, Write};
use std::path::PathBuf;
use std::{fs, process};

use anyhow::{bail, Context, Result};

use crate::config::Config;

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
