use anyhow::{bail, Context, Result};
use clap::Args;
use console::style;

use crate::cmd::{Completion, Run};
use crate::config::Config;
use crate::term::{BranchStatus, Cmd, GitBranch};
use crate::{stderr, term};

/// Git branch operations
#[derive(Args)]
pub struct BranchArgs {
    /// Branch name, optional
    pub name: Option<String>,

    /// Show all info, include branch status
    #[clap(short)]
    pub all: bool,

    /// Sync branch with remote
    #[clap(short)]
    pub sync: bool,

    /// Create a new branch
    #[clap(short)]
    pub create: bool,

    /// Delete branch
    #[clap(short)]
    pub delete: bool,

    /// Push change (create or delete) to remote
    #[clap(short)]
    pub push: bool,

    /// Search and switch to remote branch
    #[clap(short)]
    pub remote: bool,
}

enum SyncBranchTask<'a> {
    Sync(&'a str, &'a str),
    Delete(&'a str),
}

impl Run for BranchArgs {
    fn run(&self, _cfg: &Config) -> Result<()> {
        if self.sync {
            term::ensure_no_uncommitted()?;
            self.fetch()?;
        }
        if self.remote {
            return self.search_and_switch_remote();
        }
        let branches = GitBranch::list().context("list branch")?;
        if self.sync {
            return self.sync(&branches);
        }
        if self.delete {
            return self.delete(&branches);
        }
        if !self.create && self.push {
            return self.push(&branches);
        }
        if let None = &self.name {
            self.show(&branches);
            return Ok(());
        }
        let name = self.name.as_ref().unwrap();
        if self.create {
            Cmd::git(&["checkout", "-b", name.as_str()])
                .with_display_cmd()
                .execute_check()?;
        } else {
            Cmd::git(&["checkout", name.as_str()])
                .with_display_cmd()
                .execute_check()?;
        }
        if self.push {
            Cmd::git(&["push", "--set-upstream", "origin", name.as_str()])
                .with_display_cmd()
                .execute_check()?;
        }

        Ok(())
    }
}

impl BranchArgs {
    fn show(&self, branches: &Vec<GitBranch>) {
        if branches.is_empty() {
            return;
        }
        if !self.all {
            for branch in branches {
                println!("{}", branch.name);
            }
            return;
        }
        for branch in branches {
            println!("{} {}", branch.name.as_str(), branch.status.display(),);
        }
    }

    fn sync(&self, branches: &Vec<GitBranch>) -> Result<()> {
        let default = GitBranch::default().context("Get default branch")?;

        let mut back = &default;
        let mut tasks: Vec<SyncBranchTask> = vec![];
        let mut current: &str = "";
        for branch in branches {
            if branch.current {
                current = branch.name.as_str();
                match branch.status {
                    BranchStatus::Gone => {}
                    _ => back = &branch.name,
                }
            }
            let task = match branch.status {
                BranchStatus::Ahead => Some(SyncBranchTask::Sync("push", branch.name.as_str())),
                BranchStatus::Behind => Some(SyncBranchTask::Sync("pull", branch.name.as_str())),
                BranchStatus::Gone => {
                    if branch.name == default {
                        // we cannot delete default branch
                        continue;
                    }
                    Some(SyncBranchTask::Delete(branch.name.as_str()))
                }
                _ => None,
            };
            if let Some(task) = task {
                tasks.push(task);
            }
        }
        if tasks.is_empty() {
            stderr!("Nothing to sync");
            return Ok(());
        }

        stderr!("Backup branch is {}", style(back).magenta());
        let mut items = Vec::with_capacity(tasks.len());
        for task in &tasks {
            let msg = match task {
                SyncBranchTask::Sync(op, branch) => match *op {
                    "push" => format!("↑{branch}"),
                    "pull" => format!("↓{branch}"),
                    _ => format!("?{branch}"),
                },
                SyncBranchTask::Delete(branch) => format!("-{branch}"),
            };
            items.push(msg);
        }
        term::must_confirm_items(&items, "sync", "synchronization", "Branch", "Branches")?;

        for task in tasks {
            match task {
                SyncBranchTask::Sync(op, branch) => {
                    if current != branch {
                        // checkout to this branch to perform push/pull
                        Cmd::git(&["checkout", branch])
                            .with_display_cmd()
                            .execute_check()?;
                        current = branch;
                    }
                    Cmd::git(&[op]).with_display_cmd().execute_check()?;
                }
                SyncBranchTask::Delete(branch) => {
                    if current == branch {
                        // we cannot delete branch when we are inside it, checkout
                        // to default branch first.
                        let default = default.as_str();
                        Cmd::git(&["checkout", default])
                            .with_display_cmd()
                            .execute_check()?;
                        current = default;
                    }
                    Cmd::git(&["branch", "-D", branch])
                        .with_display_cmd()
                        .execute_check()?;
                }
            }
        }
        if current != back {
            Cmd::git(&["checkout", back])
                .with_display_cmd()
                .execute_check()?;
        }

        Ok(())
    }

    fn fetch(&self) -> Result<()> {
        Cmd::git(&["fetch", "origin", "--prune"])
            .with_display_cmd()
            .execute_check()
    }

    fn delete(&self, branches: &Vec<GitBranch>) -> Result<()> {
        let branch = self.get_branch_or_current(branches)?;

        if branch.current {
            term::ensure_no_uncommitted()?;
            let default = GitBranch::default()?;
            if branch.name.eq(&default) {
                bail!("could not delete default branch");
            }
            Cmd::git(&["checkout", default.as_str()])
                .with_display_cmd()
                .execute_check()?;
        }

        Cmd::git(&["branch", "-D", &branch.name])
            .with_display_cmd()
            .execute_check()?;
        if self.push {
            Cmd::git(&["push", "origin", "--delete", &branch.name])
                .with_display_cmd()
                .execute_check()?;
        }
        Ok(())
    }

    fn push(&self, branches: &Vec<GitBranch>) -> Result<()> {
        let branch = self.get_branch_or_current(branches)?;
        Cmd::git(&["push", "--set-upstream", "origin", branch.name.as_ref()])
            .with_display_cmd()
            .execute_check()
    }

    fn get_branch_or_current<'a>(&self, branches: &'a Vec<GitBranch>) -> Result<&'a GitBranch> {
        match &self.name {
            Some(name) => match branches.iter().find(|b| b.name.eq(name)) {
                Some(b) => Ok(b),
                None => bail!("could not find branch '{}'", style(&name).yellow()),
            },
            None => Self::must_get_current_branch(branches),
        }
    }

    fn must_get_current_branch(branches: &Vec<GitBranch>) -> Result<&GitBranch> {
        match branches.iter().find(|b| b.current) {
            Some(b) => Ok(b),
            None => bail!("could not find current branch"),
        }
    }

    fn search_and_switch_remote(&self) -> Result<()> {
        self.fetch()?;
        let branches = GitBranch::list_remote("origin")?;
        if branches.is_empty() {
            stderr!("No remote branch to switch");
            return Ok(());
        }

        let idx = term::fzf_search(&branches)?;
        let target = branches[idx].as_str();

        Cmd::git(&["checkout", target]).execute()?.check()
    }

    pub fn completion() -> Completion {
        Completion {
            args: Completion::branch_args,
            flags: None,
        }
    }
}
