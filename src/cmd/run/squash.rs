use std::process::{Command, Stdio};

use anyhow::{bail, Result};
use clap::Args;
use console::style;

use crate::cmd::Run;
use crate::errors::SilentExit;
use crate::repo::database::Database;
use crate::term::{self, Cmd, GitRemote};
use crate::{api, confirm};
use crate::{config, exec};

/// Squash multiple commits into one
#[derive(Args)]
pub struct SquashArgs {
    /// Rebase source (optional), default will use HEAD branch
    pub target: Option<String>,

    /// Upstream mode, only used for forked repo
    #[clap(long, short)]
    pub upstream: bool,

    /// Commit message
    #[clap(long, short)]
    pub message: Option<String>,

    /// If true, the cache will not be used when calling the API search.
    #[clap(long, short)]
    pub force: bool,
}

impl Run for SquashArgs {
    fn run(&self) -> Result<()> {
        term::ensure_no_uncommitted()?;
        let remote = if self.upstream {
            let db = Database::read()?;
            let repo = db.must_current()?;
            let remote = config::must_get_remote(repo.remote.as_str())?;
            let provider = api::init_provider(&remote, self.force)?;

            GitRemote::from_upstream(&remote, &repo, &provider)?
        } else {
            GitRemote::new()
        };

        let branch = match &self.target {
            Some(target) => Some(target.as_str()),
            None => None,
        };
        let commits = remote.commits_between(branch)?;
        if commits.is_empty() {
            println!("No commit to squash");
            return Ok(());
        }
        if commits.len() == 1 {
            println!("Only found one commit ahead target, no need to squash");
            return Ok(());
        }

        println!();
        println!("Found {} commits ahead:", style(commits.len()).yellow());
        for commit in commits.iter() {
            println!("  * {}", commit);
        }
        confirm!("Continue");

        let set = format!("HEAD~{}", commits.len());
        Cmd::git(&["reset", "--soft", set.as_str()])
            .execute()?
            .check()?;

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
