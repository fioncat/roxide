use std::borrow::Cow;
use std::ops::{Deref, DerefMut};

use anyhow::{Result, bail};

use crate::debug;
use crate::exec::git::branch::Branch;

use super::GitCmd;

#[derive(Debug, Clone)]
pub struct Remote(String);

impl Deref for Remote {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Remote {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Remote {
    pub fn new(name: &str) -> Self {
        Remote(name.to_string())
    }

    pub fn origin(cmd: GitCmd) -> Result<Option<Self>> {
        let remotes = Self::list(cmd)?;
        for remote in remotes {
            if remote.as_str() == "origin" {
                return Ok(Some(remote));
            }
        }
        Ok(None)
    }

    pub fn list(cmd: GitCmd) -> Result<Vec<Self>> {
        debug!("[remote] List remotes, cmd: {cmd:?}");
        let lines = cmd.lines(["remote"], "List remote")?;
        let remotes = lines.into_iter().map(Remote).collect::<Vec<_>>();
        debug!("[remote] Remotes: {remotes:?}");
        Ok(remotes)
    }

    pub fn get_url(&self, cmd: GitCmd) -> Result<String> {
        debug!("[remote] Get url for remote {:?}", self.as_str());
        let url = cmd.output(
            ["remote", "get-url", self.as_str()],
            format!("Get url for remote {}", self.as_str()),
        )?;
        if url.is_empty() {
            bail!("empty url for remote {:?}", self.as_str());
        }
        debug!("[remote] URL result: {url}");
        Ok(url)
    }

    pub fn get_target(&self, cmd: GitCmd, branch: &str) -> Result<String> {
        debug!("[remote] Get target for {:?}, branch: {branch}", self);
        let branch = if branch.is_empty() {
            let default_branch = Branch::remote_default(cmd, self.as_str())?;
            Cow::Owned(default_branch)
        } else {
            Cow::Borrowed(branch)
        };

        let target = format!("{}/{branch}", self.as_str());
        cmd.execute(
            ["fetch", self.as_str(), branch.as_ref()],
            format!("Fetch target {target}"),
        )?;
        debug!("[remote] Target: {target}");
        Ok(target)
    }

    pub fn commits_between(&self, cmd: GitCmd, branch: &str, with_id: bool) -> Result<Vec<String>> {
        let target = self.get_target(cmd, branch)?;

        let compare = format!("HEAD...{target}");
        let lines = cmd.lines(
            [
                "log",
                "--left-right",
                "--cherry-pick",
                "--oneline",
                &compare,
            ],
            format!("Get commits between {target}"),
        )?;
        let mut commits = Vec::with_capacity(lines.len());
        for line in lines {
            if !line.starts_with('<') {
                // If the commit message output by "git log xxx" does not start
                // with "<", it means that this commit is from the target branch.
                // Since we only list commits from current branch, ignore such
                // commits.
                continue;
            }

            let mut line = line.trim_start_matches('<').trim().to_string();
            if !with_id {
                let fields = line.split_whitespace().collect::<Vec<_>>();
                if fields.len() < 2 {
                    continue;
                }
                line = fields[1..].join(" ");
            }
            commits.push(line.to_string());
        }

        Ok(commits)
    }
}

#[cfg(test)]
mod tests {
    use crate::config::context::ConfigContext;
    use crate::exec::git;

    use super::*;

    #[test]
    fn test_remote() {
        let Some(repo_path) = git::tests::setup() else {
            return;
        };
        let ctx = ConfigContext::new_mock();
        let cmd = ctx.git_work_dir(&repo_path);

        let remotes = Remote::list(cmd).unwrap();
        assert_eq!(remotes.len(), 1);
        assert_eq!(remotes[0].as_str(), "origin");

        let remote = Remote::origin(cmd).unwrap().unwrap();
        assert_eq!(remote.as_str(), "origin");
    }

    #[test]
    fn test_get_url() {
        let Some(repo_path) = git::tests::setup() else {
            return;
        };
        let ctx = ConfigContext::new_mock();
        let cmd = ctx.git_work_dir(&repo_path);

        let remote = Remote::origin(cmd).unwrap().unwrap();
        let url = remote.get_url(cmd).unwrap();
        assert_eq!(url, "https://github.com/fioncat/roxide.git");
    }

    #[test]
    fn test_get_target() {
        let Some(repo_path) = git::tests::setup() else {
            return;
        };
        let ctx = ConfigContext::new_mock();
        let cmd = ctx.git_work_dir(&repo_path);

        let remote = Remote::origin(cmd).unwrap().unwrap();
        let target = remote.get_target(cmd, "").unwrap();
        assert_eq!(target, "origin/main");
    }
}
