use std::process::{Command, Stdio};

use anyhow::{bail, Result};
use clap::Args;
use console::style;

use crate::cmd::{self, Completion, Run};
use crate::config::Config;
use crate::errors::SilentExit;
use crate::term::Cmd;
use crate::{confirm, exec, stderrln};

/// Squash multiple commits into one
#[derive(Args)]
pub struct SquashArgs {
    /// Rebase source (optional), default will use HEAD branch
    pub target: Option<String>,

    /// Upstream mode, only used for forked repo
    #[clap(short)]
    pub upstream: bool,

    /// Commit message
    #[clap(short)]
    pub message: Option<String>,

    /// When calling the remote API, ignore caches that are not expired.
    #[clap(short)]
    pub force: bool,
}

impl Run for SquashArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
        let remote = cmd::get_git_remote(cfg, self.upstream, self.force)?;

        let branch = match &self.target {
            Some(target) => Some(target.as_str()),
            None => None,
        };
        let commits = remote.commits_between(branch)?;
        if commits.is_empty() {
            stderrln!("No commit to squash");
            return Ok(());
        }
        if commits.len() == 1 {
            stderrln!("Only found one commit ahead target, no need to squash");
            return Ok(());
        }

        stderrln!();
        stderrln!("Found {} commits ahead:", style(commits.len()).yellow());
        for commit in commits.iter() {
            stderrln!("  * {}", commit);
        }

        confirm!("Continue");

        let set = format!("HEAD~{}", commits.len());
        Cmd::git(&["reset", "--soft", set.as_str()])
            .with_display_cmd()
            .execute_check()?;

        let mut args = vec!["commit"];
        if let Some(msg) = &self.message {
            args.push("-m");
            args.push(msg);
        }

        exec!("git commit");
        let mut cmd = Command::new("git");
        cmd.stdout(Stdio::inherit());
        cmd.stderr(Stdio::inherit());
        cmd.stdin(Stdio::inherit());
        cmd.args(args);

        match cmd.output() {
            Ok(_) => {}
            Err(_) => bail!(SilentExit { code: 101 }),
        }

        Ok(())
    }
}

impl SquashArgs {
    pub fn completion() -> Completion {
        Completion {
            args: Completion::branch_args,
            flags: None,
        }
    }
}
