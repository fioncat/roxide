use std::io::Write;
use std::path::PathBuf;
use std::process;

use anyhow::{bail, Context, Result};

use crate::config::Config;
use crate::utils;

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
    ///   granularity to lock different processes.
    pub fn acquire(cfg: &Config, name: impl AsRef<str>) -> Result<FileLock> {
        let path = cfg.get_meta_dir().join("lock").join(name.as_ref());
        utils::ensure_dir(&path)?;

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
