pub mod manage;
pub mod select;

use std::fs;
use std::io;
use std::path::Path;

use anyhow::{Context, Result};

/// If the file directory doesn't exist, create it; if it exists, take no action.
pub fn ensure_dir<P: AsRef<Path>>(dir: P) -> Result<()> {
    match fs::read_dir(dir.as_ref()) {
        Ok(_) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            fs::create_dir_all(dir.as_ref())
                .with_context(|| format!("create directory '{}'", dir.as_ref().display()))?;
            Ok(())
        }
        Err(err) => {
            Err(err).with_context(|| format!("read directory '{}'", dir.as_ref().display()))
        }
    }
}
