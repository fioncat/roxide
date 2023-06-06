#![allow(dead_code)]

use std::io::{ErrorKind, Read, Write};
use std::mem;
use std::process::{ChildStdout, Command, Stdio};

use anyhow::{bail, Context, Result};
use chrono::offset::Local;
use console::{style, StyledObject};
use regex::{Captures, Regex};

use crate::api;
use crate::config;
use crate::errors::SilentExit;
use crate::repo::database::Database;
use crate::repo::types::Repo;
use crate::{confirm, info, show_exec};

pub struct Shell {
    cmd: Command,
    desc: Option<&'static str>,
    input: Option<String>,

    mute: bool,
}

pub struct ShellResult {
    pub code: Option<i32>,

    pub stdout: ChildStdout,
}

impl ShellResult {
    pub fn read(&mut self) -> Result<String> {
        let mut output = String::new();
        self.stdout
            .read_to_string(&mut output)
            .context("Read stdout from command")?;
        Ok(output.trim().to_string())
    }

    pub fn check(&self) -> Result<()> {
        match self.code {
            Some(0) => Ok(()),
            _ => bail!(SilentExit { code: 130 }),
        }
    }

    pub fn checked_read(&mut self) -> Result<String> {
        self.check()?;
        self.read()
    }

    pub fn checked_lines(&mut self) -> Result<Vec<String>> {
        let output = self.checked_read()?;
        let lines: Vec<String> = output
            .split("\n")
            .filter_map(|line| {
                if line.is_empty() {
                    return None;
                }
                Some(line.to_string())
            })
            .collect();
        Ok(lines)
    }
}

impl Shell {
    pub fn new(program: &str) -> Shell {
        Self::with_args(program, &[])
    }

    pub fn with_args(program: &str, args: &[&str]) -> Shell {
        let mut cmd = Command::new(program);
        if !args.is_empty() {
            cmd.args(args);
        }
        // The stdout will be captured anyway to protected roxide's own output.
        // If shell fails, the caller should handle its stdout manually.
        cmd.stdout(Stdio::piped());
        // We rediect command's stderr to roxide's. So that user can view command's
        // error output directly.
        cmd.stderr(Stdio::inherit());

        Shell {
            cmd,
            desc: None,
            input: None,
            mute: false,
        }
    }

    pub fn git(args: &[&str]) -> Shell {
        Self::with_args("git", args)
    }

    pub fn sh(script: &str) -> Shell {
        // FIXME: We add `> /dev/stderr` at the end of the script to ensure that
        // the script does not output any content to stdout. This method is not
        // applicable to Windows and a more universal method is needed.
        let script = format!("{script} > /dev/stderr");
        Self::with_args("sh", &["-c", script.as_str()])
    }

    pub fn with_desc(&mut self, desc: &'static str) -> &mut Self {
        self.desc = Some(desc);
        self
    }

    pub fn with_input(&mut self, input: String) -> &mut Self {
        self.input = Some(input);
        self
    }

    pub fn set_mute(&mut self, mute: bool) -> &mut Self {
        self.mute = mute;
        self
    }

    pub fn execute(&mut self) -> Result<ShellResult> {
        self.show_desc();

        let mut child = match self.cmd.spawn() {
            Ok(child) => child,
            Err(e) if e.kind() == ErrorKind::NotFound => {
                bail!(
                    "Could not find command `{}`, please make sure it is installed",
                    self.cmd.get_program().to_str().unwrap()
                );
            }
            Err(e) => {
                return Err(e).with_context(|| {
                    format!(
                        "could not launch `{}`",
                        self.cmd.get_program().to_str().unwrap()
                    )
                })
            }
        };

        if let Some(input) = &self.input {
            let handle = child.stdin.as_mut().unwrap();
            if let Err(err) = write!(handle, "{}", input) {
                return Err(err).context("Write content to command");
            }

            mem::drop(child.stdin.take());
        }

        let stdout = child.stdout.take().unwrap();
        let status = child.wait().context("Wait command done")?;

        Ok(ShellResult {
            code: status.code(),
            stdout,
        })
    }

    fn show_desc(&self) {
        if self.mute {
            return;
        }
        match self.desc {
            Some(desc) => show_exec!("{}", desc),
            None => {
                let mut desc_args = Vec::with_capacity(1);
                desc_args.push(self.cmd.get_program().to_str().unwrap());
                let args = self.cmd.get_args();
                for arg in args {
                    desc_args.push(arg.to_str().unwrap());
                }
                let desc = desc_args.join(" ");
                show_exec!("{}", desc);
            }
        }
    }
}

pub fn search<S>(keys: &Vec<S>) -> Result<usize>
where
    S: AsRef<str>,
{
    let mut input = String::with_capacity(keys.len());
    for key in keys {
        input.push_str(key.as_ref());
        input.push_str("\n");
    }

    let mut fzf = Shell::new("fzf");
    fzf.set_mute(true).with_input(input);

    let mut result = fzf.execute()?;
    match result.code {
        Some(0) => {
            let output = result.read()?;
            match keys.iter().position(|s| s.as_ref() == output) {
                Some(idx) => Ok(idx),
                None => bail!("could not find key {}", output),
            }
        }
        Some(1) => bail!("fzf no match found"),
        Some(2) => bail!("fzf returned an error"),
        Some(130) => bail!(SilentExit { code: 130 }),
        Some(128..=254) | None => bail!("fzf was terminated"),
        _ => bail!("fzf returned an unknown error"),
    }
}

pub fn ensure_no_uncommitted() -> Result<()> {
    let mut git = Shell::git(&["status", "-s"]);
    let lines = git.execute()?.checked_lines()?;
    if !lines.is_empty() {
        let (word, call) = if lines.len() == 1 {
            ("change", "it")
        } else {
            ("changes", "them")
        };
        bail!(
            "you have {} uncommitted {}, please handle {} first",
            lines.len(),
            word,
            call
        );
    }
    Ok(())
}

#[derive(Debug)]
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

#[derive(Debug)]
pub struct GitBranch {
    pub name: String,
    pub status: BranchStatus,

    pub current: bool,
}

impl GitBranch {
    const BRANCH_REGEX: &str = r"^(\*)*[ ]*([^ ]*)[ ]*([^ ]*)[ ]*(\[[^\]]*\])*[ ]*(.*)$";
    const HEAD_BRANCH_PREFIX: &str = "HEAD branch:";

    pub fn list() -> Result<Vec<GitBranch>> {
        let re = Regex::new(Self::BRANCH_REGEX).expect("parse git branch regex");
        let lines = Shell::git(&["branch", "-vv"])
            .with_desc("List git branch info")
            .execute()?
            .checked_lines()?;
        let mut branches: Vec<GitBranch> = Vec::with_capacity(lines.len());
        for line in lines {
            let branch = Self::parse(&re, line)?;
            branches.push(branch);
        }

        Ok(branches)
    }

    pub fn default() -> Result<String> {
        Self::default_by_remote("origin")
    }

    pub fn default_by_remote(remote: &str) -> Result<String> {
        info!("Get default branch for {}", remote);
        let head_ref = format!("refs/remotes/{}/HEAD", remote);
        let remote_ref = format!("refs/remotes/{}/", remote);

        let mut git = Shell::git(&["symbolic-ref", &head_ref]);
        if let Ok(out) = git.execute()?.checked_read() {
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
        let mut git = Shell::git(&["remote", "show", remote]);
        let lines = git.execute()?.checked_lines()?;
        for line in lines {
            if let Some(branch) = line.trim().strip_prefix(Self::HEAD_BRANCH_PREFIX) {
                let branch = branch.trim();
                if branch.is_empty() {
                    bail!("Default branch returned by git remote show is empty");
                }
                return Ok(branch.to_string());
            }
        }

        bail!("No default branch returned by git remote show, please check your git command");
    }

    pub fn current() -> Result<String> {
        Shell::git(&["branch", "--show-current"])
            .with_desc("Get current branch info")
            .execute()?
            .checked_read()
    }

    fn parse(re: &Regex, line: impl AsRef<str>) -> Result<GitBranch> {
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
        if let Some(_) = caps.get(1) {
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
        let lines = Shell::git(&["remote"])
            .with_desc("List git remotes")
            .execute()?
            .checked_lines()?;
        Ok(lines.iter().map(|s| GitRemote(s.to_string())).collect())
    }

    pub fn select(upstream: bool) -> Result<GitRemote> {
        if !upstream {
            return Ok(GitRemote(String::from("origin")));
        }

        let remotes = Self::list()?;
        let upstream_remote = remotes
            .into_iter()
            .find(|remote| remote.0.as_str() == "upstream");
        if let Some(remote) = upstream_remote {
            return Ok(remote);
        }

        let db = Database::read()?;
        let repo = db.must_current()?;
        let remote = config::must_get_remote(repo.remote.as_str())?;
        let provider = api::init_provider(&remote)?;

        info!("Get upstream for {}", repo.full_name());
        let api_repo = provider.get_repo(&repo.owner, &repo.name)?;
        if let None = api_repo.upstream {
            bail!(
                "Repo {} is not forked, so it has not an upstream",
                repo.full_name()
            );
        }
        let api_upstream = api_repo.upstream.unwrap();
        let upstream = Repo::from_upstream(&remote.name, api_upstream);
        let url = upstream.clone_url(&remote);

        confirm!(
            "Do you want to set upstream to {}: {}",
            upstream.long_name(),
            url
        );

        Shell::git(&["remote", "add", "upstream", url.as_str()])
            .execute()?
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
        Shell::git(&["fetch", self.0.as_str(), branch.as_str()])
            .execute()?
            .check()?;
        Ok(target)
    }
}

pub struct GitTag(String);

impl std::fmt::Display for GitTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl GitTag {
    const NUM_REGEX: &str = r"\d+";
    const PLACEHOLDER_REGEX: &str = r"\{(\d+|%[yYmMdD])(\+)*}";

    pub fn from_str(s: impl AsRef<str>) -> GitTag {
        GitTag(s.as_ref().to_string())
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub fn list() -> Result<Vec<GitTag>> {
        let tags: Vec<_> = Shell::git(&["tag"])
            .with_desc("Get git tags")
            .execute()?
            .checked_lines()?
            .iter()
            .filter(|line| !line.trim().is_empty())
            .map(|line| GitTag(line.trim().to_string()))
            .collect();
        Ok(tags)
    }

    pub fn latest() -> Result<GitTag> {
        Shell::git(&["fetch", "origin", "--prune-tags"])
            .with_desc("Fetch tags")
            .execute()?
            .check()?;
        let output = Shell::git(&["describe", "--tags", "--abbrev=0"])
            .with_desc("Get latest tag")
            .execute()?
            .checked_read()?;
        if output.is_empty() {
            bail!("no latest tag")
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
