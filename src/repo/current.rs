use std::sync::Arc;

use anyhow::{Result, bail};

use crate::config::context::ConfigContext;
use crate::db::repo::Repository;
use crate::debug;

pub fn get_current_repo(ctx: Arc<ConfigContext>) -> Result<Repository> {
    match get_current_repo_optional(ctx)? {
        Some(repo) => Ok(repo),
        None => bail!("you are not in any repository"),
    }
}

pub fn get_current_repo_optional(ctx: Arc<ConfigContext>) -> Result<Option<Repository>> {
    debug!(
        "[current] Get current repo, dir: {}",
        ctx.current_dir.display()
    );
    let db = ctx.get_db()?;

    if let Some((remote, owner, name)) =
        super::parse_workspace_path(&ctx.cfg.workspace, &ctx.current_dir)
    {
        debug!(
            "[current] Now in workspace repo, remote: {remote:?}, owner: {owner:?}, name: {name:?}"
        );
        let repo = db.with_transaction(|tx| tx.repo().get_optional(&remote, &owner, &name))?;
        debug!("[current] Current workspace repo: {repo:?}");
        return Ok(repo);
    }

    debug!("[current] Not in workspace repo, try by path");
    let path = format!("{}", ctx.current_dir.display());
    let repo = db.with_transaction(|tx| tx.repo().get_by_path_optional(&path))?;
    debug!("[current] Current path repo: {repo:?}");
    Ok(repo)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use crate::config::context;
    use crate::repo;

    use super::*;

    #[test]
    fn test_current_repo() {
        let ctx = context::tests::build_test_context(
            "current_repo",
            Some(PathBuf::from("/path/to/nvimdots")),
        );
        let repo = get_current_repo(ctx).unwrap();
        assert_eq!(
            repo,
            Repository {
                remote: "github".to_string(),
                owner: "fioncat".to_string(),
                name: "nvimdots".to_string(),
                path: Some("/path/to/nvimdots".to_string()),
                sync: true,
                pin: true,
                last_visited_at: 3333,
                visited_count: 0,
                new_created: false,
            }
        );
    }

    #[test]
    fn test_current_repo_workspace() {
        let workspace = "./tests/current_repo_workspace";
        repo::ensure_dir(workspace).unwrap();
        let workspace = fs::canonicalize(workspace).unwrap();
        let repo_path = workspace
            .join("workspace")
            .join("github")
            .join("fioncat")
            .join("roxide")
            .join("src");
        let ctx = context::tests::build_test_context("current_repo_workspace", Some(repo_path));
        let repo = get_current_repo(ctx).unwrap();
        assert_eq!(
            repo,
            Repository {
                remote: "github".to_string(),
                owner: "fioncat".to_string(),
                name: "roxide".to_string(),
                path: None,
                sync: true,
                pin: true,
                last_visited_at: 2234,
                visited_count: 20,
                new_created: false,
            }
        )
    }
}
