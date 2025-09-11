pub mod backup;
pub mod current;
pub mod disk_usage;
pub mod mirror;
pub mod ops;
pub mod select;
pub mod wait_action;

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::db::repo::Repository;

/// If the directory doesn't exist, create it; if it exists, take no action.
pub fn ensure_dir<P: AsRef<Path>>(dir: P) -> Result<()> {
    if dir.as_ref().exists() {
        return Ok(());
    }
    fs::create_dir_all(dir.as_ref())
        .with_context(|| format!("create directory {:?}", dir.as_ref().display()))
}

pub fn parse_workspace_path<W, P>(workspace: W, path: P) -> Option<(String, String, String)>
where
    W: AsRef<Path>,
    P: AsRef<Path>,
{
    let relative = path.as_ref().strip_prefix(workspace.as_ref()).ok()?;
    let components = relative
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect::<Vec<_>>();

    if components.len() < 3 {
        return None;
    }

    let (remote, owner, name) = (components[0], components[1], components[2]);
    Some((
        Repository::parse_escaped_path(remote),
        Repository::parse_escaped_path(owner),
        name.to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_workspace_path() {
        let workspace = "/home/user/dev";
        let cases = [
            (
                "/home/user/dev/github/owner/repo",
                Some(("github", "owner", "repo")),
            ),
            (
                "/home/user/dev/gitlab/owner/repo/src/config",
                Some(("gitlab", "owner", "repo")),
            ),
            (
                "/home/user/dev/gitlab/owner/repo",
                Some(("gitlab", "owner", "repo")),
            ),
            (
                "/home/user/dev/github/owner/repo/subdir/",
                Some(("github", "owner", "repo")),
            ),
            (
                "/home/user/dev/gitlab/group.subgroup/repo",
                Some(("gitlab", "group/subgroup", "repo")),
            ),
            (
                "/home/user/dev/gitlab/group.subgroup/repo.subrepo",
                Some(("gitlab", "group/subgroup", "repo.subrepo")),
            ),
            ("/home/user/dev/github/owner", None),
            ("/home/user/dev/github", None),
            ("/home/user/dev/", None),
            ("/home/user/other/github/owner/repo", None),
            ("/usr/bin", None),
            ("", None),
            ("/", None),
        ];
        for (path, expect) in cases {
            let result = parse_workspace_path(workspace, path);
            let expect = expect.map(|(r, o, n)| (r.to_string(), o.to_string(), n.to_string()));
            assert_eq!(result, expect);
        }
    }
}
