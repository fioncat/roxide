use anyhow::{Result, bail};

use crate::debug;

use super::GitCmd;

pub fn ensure_no_uncommitted_changes(cmd: GitCmd) -> Result<()> {
    if count_uncommitted_changes(cmd)? > 0 {
        bail!("uncommitted changes found, please commit them first");
    }
    Ok(())
}

pub fn count_uncommitted_changes(cmd: GitCmd) -> Result<usize> {
    debug!("[commit] Count uncommitted changes, cmd: {cmd:?}");
    let count = cmd
        .lines(["status", "-s"], "Count uncommitted changes")?
        .len();
    debug!("[commit] Uncommitted changes count: {count}");
    Ok(count)
}

pub fn get_current_commit(cmd: GitCmd) -> Result<String> {
    debug!("[commit] Get current commit, cmd: {cmd:?}");
    let commit = cmd.output(["rev-parse", "HEAD"], "Get current commit")?;
    debug!("[commit] Current commit: {commit}");
    Ok(commit)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::config::context::ConfigContext;
    use crate::exec::git;

    use super::*;

    #[test]
    fn test_uncommitted() {
        let Some(repo_path) = git::tests::setup() else {
            return;
        };
        let ctx = ConfigContext::new_mock();
        let cmd = ctx.git_work_dir(&repo_path);

        assert_eq!(count_uncommitted_changes(cmd).unwrap(), 0);
        assert!(ensure_no_uncommitted_changes(cmd).is_ok());

        // Try to add a new file, which should cause uncommitted changes
        let path = format!("{}/new_file.txt", repo_path.display());
        fs::write(&path, "Hello, world!").unwrap();

        assert_eq!(count_uncommitted_changes(cmd).unwrap(), 1);
        assert_eq!(
            ensure_no_uncommitted_changes(cmd).unwrap_err().to_string(),
            "uncommitted changes found, please commit them first"
        );

        fs::remove_file(&path).unwrap();

        assert_eq!(count_uncommitted_changes(cmd).unwrap(), 0);
        assert!(ensure_no_uncommitted_changes(cmd).is_ok());
    }
}
