use std::path::Path;

use anyhow::{Result, bail};

use crate::debug;

pub fn ensure_no_uncommitted_changes<P>(path: Option<P>, mute: bool) -> Result<()>
where
    P: AsRef<Path> + std::fmt::Debug,
{
    if count_uncommitted_changes(path, mute)? > 0 {
        bail!("uncommitted changes found, please commit them first");
    }
    Ok(())
}

pub fn count_uncommitted_changes<P>(path: Option<P>, mute: bool) -> Result<usize>
where
    P: AsRef<Path> + std::fmt::Debug,
{
    debug!("[commit] Count uncommitted changes for {path:?}");
    let count = super::new(["status", "-s"], path, "Count uncommitted changes", mute)
        .lines()?
        .len();
    debug!("[commit] Uncommitted changes count: {count}");
    Ok(count)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::exec::git;

    use super::*;

    #[test]
    fn test_uncommitted() {
        let Some(repo_path) = git::tests::setup() else {
            return;
        };

        assert_eq!(count_uncommitted_changes(Some(repo_path), true).unwrap(), 0);
        assert!(ensure_no_uncommitted_changes(Some(repo_path), true).is_ok());

        // Try to add a new file, which should cause uncommitted changes
        let path = format!("{}/new_file.txt", repo_path);
        fs::write(&path, "Hello, world!").unwrap();

        assert_eq!(count_uncommitted_changes(Some(repo_path), true).unwrap(), 1);
        assert_eq!(
            ensure_no_uncommitted_changes(Some(repo_path), true)
                .unwrap_err()
                .to_string(),
            "uncommitted changes found, please commit them first"
        );

        fs::remove_file(&path).unwrap();

        assert_eq!(count_uncommitted_changes(Some(repo_path), true).unwrap(), 0);
        assert!(ensure_no_uncommitted_changes(Some(repo_path), true).is_ok());
    }
}

