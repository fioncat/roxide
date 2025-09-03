pub mod create;
pub mod current;
pub mod select;

use std::fs;
use std::io;
use std::path::Path;

use anyhow::{Context, Result};

use crate::config::remote::RemoteConfig;
use crate::db::repo::Repository;

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

pub fn get_repo_clone_url(remote: &RemoteConfig, repo: &Repository) -> Option<String> {
    let domain = remote.clone.as_ref()?;
    let owner = remote.get_owner(&repo.owner);

    if owner.ssh {
        Some(format!("git@{domain}:{}/{}.git", repo.owner, repo.name))
    } else {
        Some(format!("https://{domain}/{}/{}.git", repo.owner, repo.name))
    }
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
        Repository::parse_escaped_path(name),
    ))
}
