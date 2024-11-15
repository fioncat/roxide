use std::collections::HashSet;
use std::path::Path;

use anyhow::{bail, Context, Result};
use chrono::Local;
use console::{style, StyledObject};
use glob::Pattern as GlobPattern;
use regex::{Captures, Regex};

use crate::api::Provider;
use crate::config::Config;
use crate::exec::Cmd;
use crate::repo::Repo;
use crate::utils;
use crate::{confirm, info};

/// List all git files in `path`, use `git ls-files`, this will respect `.gitignore` file.
/// Also, this function will respect `ignores` arg, matched path will not be returned.
pub fn list_git_files(path: &Path, ignores: &[GlobPattern]) -> Result<Vec<String>> {
    let path = format!("{}", path.display());
    let mut items = Cmd::git(&["-C", path.as_str(), "ls-files"])
        .lines()
        .context("list git files")?;
    if !ignores.is_empty() {
        items.retain(|item| {
            for pattern in ignores.iter() {
                if pattern.matches(item) {
                    return false;
                }
            }
            true
        });
    }

    Ok(items)
}

/// If there are uncommitted changes in the current Git repository, return an error.
/// This will use `git status -s` to check.
pub fn ensure_no_uncommitted() -> Result<()> {
    let mut git =
        Cmd::git(&["status", "-s"]).with_display("Check if repository has uncommitted change");
    let lines = git.lines()?;
    if !lines.is_empty() {
        bail!(
            "you have {} to handle",
            utils::plural(&lines, "uncommitted change"),
        );
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
pub enum BranchStatus {
    Sync,
    Gone,
    Ahead,
    Behind,
    Conflict,
    Detached,
}

impl BranchStatus {
    pub fn display(&self) -> StyledObject<&'static str> {
        match self {
            Self::Sync => style("sync").green(),
            Self::Gone => style("gone").red(),
            Self::Ahead => style("ahead").yellow(),
            Self::Behind => style("behind").yellow(),
            Self::Conflict => style("conflict").yellow().bold(),
            Self::Detached => style("detached").red(),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct GitBranch {
    pub name: String,
    pub status: BranchStatus,

    pub current: bool,
}

impl GitBranch {
    const BRANCH_REGEX: &'static str = r"^(\*)*[ ]*([^ ]*)[ ]*([^ ]*)[ ]*(\[[^\]]*\])*[ ]*(.*)$";
    const HEAD_BRANCH_PREFIX: &'static str = "HEAD branch:";

    pub fn get_regex() -> Regex {
        Regex::new(Self::BRANCH_REGEX).expect("parse git branch regex")
    }

    pub fn list() -> Result<Vec<GitBranch>> {
        let re = Self::get_regex();
        let lines = Cmd::git(&["branch", "-vv"]).lines()?;
        let mut branches: Vec<GitBranch> = Vec::with_capacity(lines.len());
        for line in lines {
            let branch = Self::parse(&re, line)?;
            branches.push(branch);
        }

        Ok(branches)
    }

    pub fn list_remote(remote: &str) -> Result<Vec<String>> {
        let lines = Cmd::git(&["branch", "-al"]).lines()?;
        let remote_prefix = format!("{remote}/");
        let mut items = Vec::with_capacity(lines.len());
        for line in lines {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if !line.starts_with("remotes/") {
                continue;
            }
            let item = line.strip_prefix("remotes/").unwrap();
            if !item.starts_with(&remote_prefix) {
                continue;
            }
            let item = item.strip_prefix(&remote_prefix).unwrap().trim();
            if item.is_empty() {
                continue;
            }
            if item.starts_with("HEAD ->") {
                continue;
            }
            items.push(item.to_string());
        }

        let lines = Cmd::git(&["branch"]).lines()?;
        let mut local_branch_map = HashSet::with_capacity(lines.len());
        for line in lines {
            let mut line = line.trim();
            if line.is_empty() {
                continue;
            }
            if line.starts_with('*') {
                line = line.strip_prefix('*').unwrap().trim();
            }
            local_branch_map.insert(line.to_string());
        }

        Ok(items
            .into_iter()
            .filter(|item| !local_branch_map.contains(item))
            .collect())
    }

    pub fn default() -> Result<String> {
        Self::default_by_remote("origin")
    }

    pub fn default_by_remote(remote: &str) -> Result<String> {
        info!("Get default branch for {}", remote);
        let head_ref = format!("refs/remotes/{}/HEAD", remote);
        let remote_ref = format!("refs/remotes/{}/", remote);

        let mut git = Cmd::git(&["symbolic-ref", &head_ref]).with_display_cmd();
        if let Ok(out) = git.read() {
            if out.is_empty() {
                bail!("default branch is empty")
            }
            return match out.strip_prefix(&remote_ref) {
                Some(s) => Ok(s.to_string()),
                None => bail!("invalid ref output by git: {}", style(out).yellow()),
            };
        }
        // If failed, user might not switch to this branch yet, let's
        // use "git show <remote>" instead to get default branch.
        let mut git = Cmd::git(&["remote", "show", remote]).with_display_cmd();
        let lines = git.lines()?;
        Self::parse_default_branch(lines)
    }

    pub fn parse_default_branch(lines: Vec<String>) -> Result<String> {
        for line in lines {
            if let Some(branch) = line.trim().strip_prefix(Self::HEAD_BRANCH_PREFIX) {
                let branch = branch.trim();
                if branch.is_empty() {
                    bail!("default branch returned by git remote show is empty");
                }
                return Ok(branch.to_string());
            }
        }

        bail!("no default branch returned by git remote show, please check your git command");
    }

    pub fn current(mute: bool) -> Result<String> {
        let mut cmd = Cmd::git(&["branch", "--show-current"]);
        if !mute {
            cmd = cmd.with_display("Get current branch info");
        }
        cmd.read()
    }

    pub fn parse(re: &Regex, line: impl AsRef<str>) -> Result<GitBranch> {
        let parse_err = format!(
            "invalid branch description {}, please check your git command",
            style(line.as_ref()).yellow()
        );
        let mut iter = re.captures_iter(line.as_ref());
        let caps = match iter.next() {
            Some(caps) => caps,
            None => bail!(parse_err),
        };
        // We have 6 captures:
        //   0 -> line itself
        //   1 -> current branch
        //   2 -> branch name
        //   3 -> commit id
        //   4 -> remote description
        //   5 -> commit message
        if caps.len() != 6 {
            bail!(parse_err)
        }
        let mut current = false;
        if caps.get(1).is_some() {
            current = true;
        }

        let name = match caps.get(2) {
            Some(name) => name.as_str().trim(),
            None => bail!("{}: missing name", parse_err),
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

        Ok(GitBranch {
            name: name.to_string(),
            status,
            current,
        })
    }
}

pub struct GitRemote(String);

impl GitRemote {
    pub fn list() -> Result<Vec<GitRemote>> {
        let lines = Cmd::git(&["remote"])
            .with_display("List git remotes")
            .lines()?;
        Ok(lines.iter().map(|s| GitRemote(s.to_string())).collect())
    }

    pub fn new() -> GitRemote {
        GitRemote(String::from("origin"))
    }

    pub fn from_upstream(cfg: &Config, repo: &Repo, provider: &dyn Provider) -> Result<GitRemote> {
        let remotes = Self::list()?;
        let upstream_remote = remotes
            .into_iter()
            .find(|remote| remote.0.as_str() == "upstream");
        if let Some(remote) = upstream_remote {
            return Ok(remote);
        }

        info!("Get upstream for {}", repo.name_with_remote());
        let api_repo = provider.get_repo(&repo.owner, &repo.name)?;
        if api_repo.upstream.is_none() {
            bail!(
                "repo {} is not forked, so it has not an upstream",
                repo.name_with_remote()
            );
        }
        let api_upstream = api_repo.upstream.unwrap();
        let upstream = Repo::from_api_upstream(cfg, &repo.remote, api_upstream);
        let url = upstream.clone_url();

        confirm!(
            "Do you want to set upstream to {}: {}",
            upstream.name_with_remote(),
            url
        );

        Cmd::git(&["remote", "add", "upstream", url.as_str()])
            .execute_unchecked()?
            .check()?;
        Ok(GitRemote(String::from("upstream")))
    }

    pub fn target(&self, branch: Option<&str>) -> Result<String> {
        let (target, branch) = match branch {
            Some(branch) => (format!("{}/{}", self.0, branch), branch.to_string()),
            None => {
                let branch = GitBranch::default_by_remote(&self.0)?;
                (format!("{}/{}", self.0, branch), branch)
            }
        };
        Cmd::git(&["fetch", self.0.as_str(), branch.as_str()])
            .with_display_cmd()
            .execute()?;
        Ok(target)
    }

    pub fn commits_between(&self, branch: Option<&str>) -> Result<Vec<String>> {
        let target = self.target(branch)?;
        let compare = format!("HEAD...{}", target);
        let lines = Cmd::git(&[
            "log",
            "--left-right",
            "--cherry-pick",
            "--oneline",
            compare.as_str(),
        ])
        .with_display(format!("Get commits between {target}"))
        .lines()?;
        let commits: Vec<_> = lines
            .iter()
            .filter(|line| {
                // If the commit message output by "git log xxx" does not start
                // with "<", it means that this commit is from the target branch.
                // Since we only list commits from current branch, ignore such
                // commits.
                line.trim().starts_with('<')
            })
            .map(|line| line.strip_prefix('<').unwrap().to_string())
            .map(|line| {
                let mut fields = line.split_whitespace();
                fields.next();
                fields
                    .map(|field| field.to_string())
                    .collect::<Vec<String>>()
                    .join(" ")
            })
            .collect();
        Ok(commits)
    }

    pub fn get_url(&self) -> Result<String> {
        let url = Cmd::git(&["remote", "get-url", &self.0])
            .with_display(format!("Get url for remote {}", self.0))
            .read()?;
        Ok(url)
    }
}

pub struct GitTag(pub String);

impl std::fmt::Display for GitTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl GitTag {
    const NUM_REGEX: &'static str = r"\d+";
    const PLACEHOLDER_REGEX: &'static str = r"\{(\d+|%[yYmMdD])(\+)*}";

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub fn list() -> Result<Vec<GitTag>> {
        let tags: Vec<_> = Cmd::git(&["tag"])
            .lines()?
            .iter()
            .filter(|line| !line.trim().is_empty())
            .map(|line| GitTag(line.trim().to_string()))
            .collect();
        Ok(tags)
    }

    pub fn get(s: impl AsRef<str>) -> Result<GitTag> {
        let tags = Self::list()?;
        for tag in tags {
            if tag.as_str() == s.as_ref() {
                return Ok(tag);
            }
        }
        bail!("could not find tag '{}'", s.as_ref())
    }

    pub fn new(s: impl AsRef<str>) -> GitTag {
        GitTag(s.as_ref().to_string())
    }

    pub fn latest() -> Result<GitTag> {
        Cmd::git(&["fetch", "origin", "--prune-tags"])
            .with_display("Fetch tags")
            .execute()?;
        let output = Cmd::git(&["describe", "--tags", "--abbrev=0"])
            .with_display("Get latest tag")
            .read()?;
        if output.is_empty() {
            bail!("no latest tag");
        }
        Ok(GitTag(output))
    }

    pub fn apply_rule(&self, rule: impl AsRef<str>) -> Result<GitTag> {
        let num_re = Regex::new(Self::NUM_REGEX).context("unable to parse num regex")?;
        let ph_re = Regex::new(Self::PLACEHOLDER_REGEX)
            .context("unable to parse rule placeholder regex")?;

        let nums: Vec<i32> = num_re
            .captures_iter(self.0.as_str())
            .map(|caps| {
                caps.get(0)
                    .unwrap()
                    .as_str()
                    .parse()
                    // The caps here MUST be number, so it is safe to use expect here
                    .expect("unable to parse num caps")
            })
            .collect();

        let mut with_date = false;
        let result = ph_re.replace_all(rule.as_ref(), |caps: &Captures| {
            let rep = caps.get(1).unwrap().as_str();
            let idx: usize = match rep.parse() {
                Ok(idx) => idx,
                Err(_) => {
                    with_date = true;
                    return rep.to_string();
                }
            };
            let num = if idx >= nums.len() { 0 } else { nums[idx] };
            let plus = caps.get(2);
            match plus {
                Some(_) => format!("{}", num + 1),
                None => format!("{}", num),
            }
        });
        let mut result = result.to_string();
        if with_date {
            let date = Local::now();
            result = date.format(&result).to_string();
        }

        Ok(GitTag(result))
    }
}

#[cfg(test)]
mod git_tests {
    use crate::git::*;

    #[test]
    fn test_parse_branch() {
        let cases = vec![
            (
                "* main cf11adb [origin/main] My best commit since the project begin",
                "main",
                BranchStatus::Sync,
                true,
            ),
            (
                "release/1.6 dc07e7ec7 [origin/release/1.6] Merge pull request #9024 from akhilerm/cherry-pick-9021-release/1.6",
                "release/1.6",
                BranchStatus::Sync,
                false
            ),
            (
                "feat/update-version 3b0569d62 [origin/feat/update-version: ahead 1] chore: update cargo version",
                "feat/update-version",
                BranchStatus::Ahead,
                false
            ),
            (
                "* feat/tmp-dev 92bbd6e [origin/feat/tmp-dev: gone] Merge pull request #6 from fioncat/hello",
                "feat/tmp-dev",
                BranchStatus::Gone,
                true
            ),
            (
                "master       b4a40de [origin/master: ahead 1, behind 1] test commit",
                "master",
                BranchStatus::Conflict,
                false
            ),
            (
                "* dev        b4a40de test commit",
                "dev",
                BranchStatus::Detached,
                true
            ),
        ];

        let re = GitBranch::get_regex();
        for (raw, name, status, current) in cases {
            let result = GitBranch::parse(&re, raw).unwrap();
            let expect = GitBranch {
                name: String::from(name),
                status,
                current,
            };
            assert_eq!(result, expect);
        }
    }

    #[test]
    fn test_parse_default_branch() {
        // Lines from `git remote show origin`
        let lines = vec![
            "  * remote origin",
            "  Fetch URL: git@github.com:fioncat/roxide.git",
            "  Push  URL: git@github.com:fioncat/roxide.git",
            "  HEAD branch: main",
            "  Remote branches:",
            "    feat/test tracked",
            "    main      tracked",
            "  Local branches configured for 'git pull':",
            "    feat/test merges with remote feat/test",
            "    main      merges with remote main",
            "  Local refs configured for 'git push':",
            "    feat/test pushes to feat/test (up to date)",
            "    main      pushes to main      (up to date)",
        ];

        let default_branch = GitBranch::parse_default_branch(
            lines.into_iter().map(|line| line.to_string()).collect(),
        )
        .unwrap();
        assert_eq!(default_branch, "main");
    }

    #[test]
    fn test_apply_tag_rule() {
        let cases = vec![
            ("v0.1.0", "v{0}.{1+}.0", "v0.2.0"),
            ("v12.1.0", "v{0+}.0.0", "v13.0.0"),
            ("v0.8.13", "{0}.{1}.{2+}-rc1", "0.8.14-rc1"),
            ("1.2.23", "{0}.{1}.{2+}-rc{3}", "1.2.24-rc0"),
            ("1.2.23", "{0}.{1}.{2+}-rc{3+}", "1.2.24-rc1"),
            ("1.2.20-rc4", "{0}.{1}.{2+}-rc{3+}", "1.2.21-rc5"),
        ];

        for (before, rule, expect) in cases {
            let tag = GitTag(String::from(before));
            let result = tag.apply_rule(rule).unwrap();
            assert_eq!(result.as_str(), expect);
        }
    }
}
