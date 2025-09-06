use std::borrow::Cow;
use std::path::Path;
use std::sync::OnceLock;

use anyhow::{Result, bail};
use regex::Regex;
use serde::Serialize;

use crate::debug;
use crate::term::list::{List, ListItem};

const HEAD_BRANCH_PREFIX: &str = "HEAD branch:";
const BRANCH_REMOTE_PREFIX: &str = "remotes/";
const BRANCH_ORIGIN_PREFIX: &str = "origin/";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum BranchStatus {
    #[serde(rename = "sync")]
    Sync,
    #[serde(rename = "gone")]
    Gone,
    #[serde(rename = "ahead")]
    Ahead,
    #[serde(rename = "behind")]
    Behind,
    #[serde(rename = "conflict")]
    Conflict,
    #[serde(rename = "detached")]
    Detached,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Branch {
    pub name: String,
    pub status: BranchStatus,
    pub current: bool,
    pub commit_id: String,
    pub commit_message: String,
}

pub struct BranchList {
    pub branches: Vec<Branch>,
    pub total: u32,
}

static BRANCH_RE: OnceLock<Regex> = OnceLock::new();

fn get_branch_re() -> &'static Regex {
    BRANCH_RE.get_or_init(|| {
        Regex::new(r"^(\*)*[ ]*([^ ]*)[ ]*([^ ]*)[ ]*(\[[^\]]*\])*[ ]*(.*)$").unwrap()
    })
}

impl Branch {
    pub fn list<P>(path: Option<P>, mute: bool) -> Result<Vec<Self>>
    where
        P: AsRef<Path> + std::fmt::Debug,
    {
        debug!("[branch] List branch for {path:?}");
        let lines = super::new(["branch", "-vv"], path, "List git branch", mute).lines()?;
        let mut branches = Vec::with_capacity(lines.len());
        for line in lines {
            let branch = Self::parse(&line)?;
            branches.push(branch);
        }

        debug!("[branch] List result: {branches:?}");
        Ok(branches)
    }

    pub fn list_remote<P>(path: Option<P>, mute: bool) -> Result<Vec<String>>
    where
        P: AsRef<Path> + std::fmt::Debug,
    {
        debug!("[branch] List remote branch for {path:?}");
        let lines = super::new(["branch", "-al"], path, "List git remote branch", mute).lines()?;
        let mut branches = Vec::with_capacity(lines.len());
        for line in lines {
            debug!("[branch] Remote branch line: {line:?}");
            if !line.starts_with(BRANCH_REMOTE_PREFIX) {
                continue;
            }
            let line = line.trim_start_matches(BRANCH_REMOTE_PREFIX);

            if !line.starts_with(BRANCH_ORIGIN_PREFIX) {
                continue;
            }
            let line = line.trim_start_matches(BRANCH_ORIGIN_PREFIX);
            if line.is_empty() {
                continue;
            }

            if line.starts_with("HEAD ->") {
                continue;
            }
            branches.push(line.to_string());
        }

        debug!("[branch] List remote result: {branches:?}");
        Ok(branches)
    }

    pub fn default<P>(path: Option<P>, mute: bool) -> Result<String>
    where
        P: AsRef<Path> + std::fmt::Debug,
    {
        Self::remote_default(path, mute, "origin")
    }

    pub fn remote_default<P>(path: Option<P>, mute: bool, remote: &str) -> Result<String>
    where
        P: AsRef<Path> + std::fmt::Debug,
    {
        debug!("[branch] Get remote default branch for {path:?}, remote: {remote}");

        let head_ref = format!("refs/remotes/{remote}/HEAD");
        let remote_ref = format!("refs/remotes/{remote}/");

        let mut git = super::new(
            ["symbolic-ref", head_ref.as_str()],
            path.as_ref(),
            "Get default branch by symbolic-ref",
            mute,
        );
        if let Ok(out) = git.output() {
            debug!("[branch] Use symbolic-ref to get default branch ok, output: {out:?}");
            let branch = out.trim_start_matches(remote_ref.as_str()).trim();
            if branch.is_empty() {
                bail!("remote default branch is empty");
            }

            debug!("[branch] Default branch from symbolic-ref: {branch:?}");
            return Ok(branch.to_string());
        }

        let lines = super::new(
            ["remote", "show", remote],
            path,
            "Get default branch by remote show",
            mute,
        )
        .lines()?;

        for line in lines {
            if !line.starts_with(HEAD_BRANCH_PREFIX) {
                continue;
            }
            let branch = line.trim_start_matches(HEAD_BRANCH_PREFIX).trim();
            if branch.is_empty() {
                bail!("remote default branch from `git remote show` is empty");
            }
            debug!("[branch] Default branch from `git remote show`: {branch:?}");
            return Ok(branch.to_string());
        }

        bail!("no default branch found by command `git remote show`");
    }

    pub fn current<P>(path: Option<P>, mute: bool) -> Result<String>
    where
        P: AsRef<Path> + std::fmt::Debug,
    {
        debug!("[branch] Get current branch for {path:?}");
        let branch = super::new(
            ["branch", "--show-current"],
            path,
            "Get current branch",
            mute,
        )
        .output()?;
        if branch.is_empty() {
            bail!("current branch is empty");
        }
        debug!("[branch] Current branch: {branch:?}");
        Ok(branch)
    }

    pub fn parse(line: &str) -> Result<Self> {
        debug!("[branch] Parse branch line: {line:?}");
        let mut iter = get_branch_re().captures_iter(line);
        let caps = match iter.next() {
            Some(caps) => caps,
            None => bail!("invalid branch line"),
        };

        // We have 6 captures:
        //   0 -> line itself
        //   1 -> current branch
        //   2 -> branch name
        //   3 -> commit id
        //   4 -> remote description
        //   5 -> commit message
        if caps.len() != 6 {
            bail!("branch line capture count not match");
        }
        let mut current = false;
        if caps.get(1).is_some() {
            current = true;
        }

        let Some(name) = caps.get(2) else {
            bail!("branch line missing name");
        };

        let status = match caps.get(4) {
            Some(remote_desc) => {
                let remote_desc = remote_desc.as_str();
                let behind = remote_desc.contains("behind");
                let ahead = remote_desc.contains("ahead");

                if remote_desc.contains("gone") {
                    BranchStatus::Gone
                } else if ahead && behind {
                    BranchStatus::Conflict
                } else if ahead {
                    BranchStatus::Ahead
                } else if behind {
                    BranchStatus::Behind
                } else {
                    BranchStatus::Sync
                }
            }
            None => BranchStatus::Detached,
        };

        let Some(commit_id) = caps.get(3) else {
            bail!("branch line missing commit id");
        };

        let Some(commit_message) = caps.get(5) else {
            bail!("branch line missing commit message");
        };

        let branch = Branch {
            name: name.as_str().to_string(),
            status,
            current,
            commit_id: commit_id.as_str().to_string(),
            commit_message: commit_message.as_str().to_string(),
        };
        debug!("[branch] Parse result: {branch:?}");
        Ok(branch)
    }
}

impl ListItem for Branch {
    fn row<'a>(&'a self, title: &str) -> Cow<'a, str> {
        let name = if self.current {
            format!("* {}", self.name).into()
        } else {
            Cow::Borrowed(self.name.as_str())
        };
        let message = super::short_message(&self.commit_message);
        match title {
            "Name" => name,
            "Status" => format!("{:?}", self.status).into(),
            "CommitID" => Cow::Borrowed(&self.commit_id),
            "Message" => message,
            _ => Cow::Borrowed(""),
        }
    }
}

impl List<Branch> for BranchList {
    fn titles(&self) -> Vec<&'static str> {
        vec!["Name", "Status", "CommitID", "Message"]
    }

    fn total(&self) -> u32 {
        self.total
    }

    fn items(&self) -> &[Branch] {
        &self.branches
    }
}

#[cfg(test)]
mod tests {
    use crate::exec::git;

    use super::*;

    #[test]
    fn test_list() {
        let Some(repo_path) = git::tests::setup() else {
            return;
        };

        let branches = Branch::list(Some(repo_path), true).unwrap();
        for branch in branches {
            if branch.name == "main" {
                assert_eq!(branch.status, BranchStatus::Sync);
                assert!(branch.current);
            }
        }
    }

    #[test]
    fn test_current() {
        let Some(repo_path) = git::tests::setup() else {
            return;
        };

        let current = Branch::current(Some(repo_path), true).unwrap();
        assert_eq!(current, "main");
    }

    #[test]
    fn test_default() {
        let Some(repo_path) = git::tests::setup() else {
            return;
        };

        let default = Branch::default(Some(repo_path), true).unwrap();
        assert_eq!(default, "main");
    }

    #[test]
    fn test_parse() {
        let test_cases = [
            (
                "* main cf11adb [origin/main] My best commit since the project begin",
                Branch {
                    name: "main".to_string(),
                    status: BranchStatus::Sync,
                    current: true,
                    commit_id: "cf11adb".to_string(),
                    commit_message: "My best commit since the project begin".to_string(),
                },
            ),
            (
                "release/1.6 dc07e7ec7 [origin/release/1.6] Merge pull request #9024 from akhilerm/cherry-pick-9021-release/1.6",
                Branch {
                    name: "release/1.6".to_string(),
                    status: BranchStatus::Sync,
                    current: false,
                    commit_id: "dc07e7ec7".to_string(),
                    commit_message:
                        "Merge pull request #9024 from akhilerm/cherry-pick-9021-release/1.6"
                            .to_string(),
                },
            ),
            (
                "feat/update-version 3b0569d62 [origin/feat/update-version: ahead 1] chore: update cargo version",
                Branch {
                    name: "feat/update-version".to_string(),
                    status: BranchStatus::Ahead,
                    current: false,
                    commit_id: "3b0569d62".to_string(),
                    commit_message: "chore: update cargo version".to_string(),
                },
            ),
            (
                "master       b4a40de [origin/master: ahead 1, behind 1] test commit",
                Branch {
                    name: "master".to_string(),
                    status: BranchStatus::Conflict,
                    current: false,
                    commit_id: "b4a40de".to_string(),
                    commit_message: "test commit".to_string(),
                },
            ),
            (
                "* dev        b4a40de test commit",
                Branch {
                    name: "dev".to_string(),
                    status: BranchStatus::Detached,
                    current: true,
                    commit_id: "b4a40de".to_string(),
                    commit_message: "test commit".to_string(),
                },
            ),
        ];

        for (line, expected) in test_cases {
            let branch = Branch::parse(line).unwrap();
            assert_eq!(branch, expected);
        }
    }
}
