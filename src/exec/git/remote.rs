use std::borrow::Cow;
use std::path::Path;

use anyhow::{Result, bail};

use crate::debug;
use crate::exec::git::branch::Branch;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remote<'a> {
    pub name: String,
    pub path: Option<&'a Path>,
    mute: bool,
}

impl<'a> Remote<'a> {
    pub fn new<P>(name: String, path: Option<&'a P>, mute: bool) -> Self
    where
        P: AsRef<Path>,
    {
        Self {
            name,
            path: path.map(|p| p.as_ref()),
            mute,
        }
    }

    pub fn origin<P>(path: Option<&'a P>, mute: bool) -> Result<Option<Self>>
    where
        P: AsRef<Path> + std::fmt::Debug + ?Sized,
    {
        let remotes = Self::list(path, mute)?;
        for remote in remotes {
            if remote.name == "origin" {
                return Ok(Some(remote));
            }
        }
        Ok(None)
    }

    pub fn list<P>(path: Option<&'a P>, mute: bool) -> Result<Vec<Self>>
    where
        P: AsRef<Path> + std::fmt::Debug + ?Sized,
    {
        debug!("[remote] List remotes for {path:?}");
        let lines = super::new(["remote"], path.as_ref(), "List remote", mute).lines()?;
        let mut remotes = Vec::with_capacity(lines.len());
        debug!("[remote] Remotes: {lines:?}");
        for line in lines {
            remotes.push(Remote {
                name: line,
                path: path.map(|p| p.as_ref()),
                mute,
            })
        }

        Ok(remotes)
    }

    pub fn get_url(&self) -> Result<String> {
        debug!("[remote] Get url for remote {:?}", self.name);
        let url = super::new(
            ["remote", "get-url", &self.name],
            self.path,
            format!("Get url for remote {}", self.name),
            self.mute,
        )
        .output()?;
        if url.is_empty() {
            bail!("empty url for remote {:?}", self.name);
        }
        debug!("[remote] URL result: {url}");
        Ok(url)
    }

    pub fn get_target(&self, branch: &str) -> Result<String> {
        debug!("[remote] Get target for {:?}, branch: {branch}", self);
        let branch = if branch.is_empty() {
            let default_branch = Branch::remote_default(self.path, self.mute, &self.name)?;
            Cow::Owned(default_branch)
        } else {
            Cow::Borrowed(branch)
        };

        let target = format!("{}/{branch}", self.name);
        let mut git = super::new(
            ["fetch", &self.name, branch.as_ref()],
            self.path,
            format!("Fetch target {target}"),
            self.mute,
        )
        .mute();
        if let Some(p) = self.path {
            git = git.current_dir(p);
        }
        git.execute()?;
        debug!("[remote] Target: {target}");
        Ok(target)
    }
}

#[cfg(test)]
mod tests {
    use crate::exec::git;

    use super::*;

    #[test]
    fn test_remote() {
        let Some(repo_path) = git::tests::setup() else {
            return;
        };

        let remotes = Remote::list(Some(repo_path), true).unwrap();
        assert_eq!(remotes.len(), 1);
        assert_eq!(remotes[0].name, "origin");

        let remote = Remote::origin(Some(repo_path), true).unwrap().unwrap();
        assert_eq!(remote.name, "origin");
    }

    #[test]
    fn test_get_url() {
        let Some(repo_path) = git::tests::setup() else {
            return;
        };

        let remote = Remote::origin(Some(repo_path), true).unwrap().unwrap();
        let url = remote.get_url().unwrap();
        assert_eq!(url, "https://github.com/fioncat/roxide.git");
    }

    #[test]
    fn test_get_target() {
        let Some(repo_path) = git::tests::setup() else {
            return;
        };

        let remote = Remote::origin(Some(repo_path), true).unwrap().unwrap();
        let target = remote.get_target("").unwrap();
        assert_eq!(target, "origin/main");
    }
}
