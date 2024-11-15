use anyhow::{bail, Context, Result};
use clap::Args;
use console::style;

use crate::cmd::{Completion, Run};
use crate::config::Config;
use crate::exec::{self, Cmd};
use crate::git::{self, BranchStatus, GitBranch};
use crate::table::{Table, TableCell, TableCellColor};
use crate::term;

/// Git branch operations
#[derive(Args)]
pub struct BranchArgs {
    /// Branch name, optional
    pub name: Option<String>,

    /// Including remote branches for switching and listing
    #[clap(short, long)]
    pub all: bool,

    /// Only remote branches for switching and listing
    #[clap(short, long)]
    pub remote: bool,

    /// Sync branch with remote
    #[clap(short, long)]
    pub sync: bool,

    /// Create a new branch
    #[clap(short, long)]
    pub create: bool,

    /// Delete branch
    #[clap(short, long)]
    pub delete: bool,

    /// Push change (create or delete) to remote
    #[clap(short, long)]
    pub push: bool,

    /// List branch
    #[clap(short, long)]
    pub list: bool,
}

enum SyncBranchTask<'a> {
    Sync(&'a str, &'a str),
    Delete(&'a str),
}

impl Run for BranchArgs {
    fn run(&self, _cfg: &Config) -> Result<()> {
        if self.sync {
            git::ensure_no_uncommitted()?;
            self.fetch(false)?;
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
        match self.name.as_ref() {
            Some(name) => {
                if self.create {
                    Cmd::git(&["checkout", "-b", name.as_str()])
                        .with_display_cmd()
                        .execute()?;
                } else {
                    Cmd::git(&["checkout", name.as_str()])
                        .with_display_cmd()
                        .execute()?;
                }
                if self.push {
                    Cmd::git(&["push", "--set-upstream", "origin", name.as_str()])
                        .with_display_cmd()
                        .execute()?;
                }
            }

            None => {
                if self.list {
                    self.show(&branches)?;
                    return Ok(());
                }
                self.search_and_switch(&branches)?;
            }
        }

        Ok(())
    }
}

impl BranchArgs {
    fn show(&self, branches: &Vec<GitBranch>) -> Result<()> {
        if branches.is_empty() {
            eprintln!("No branch to list");
            return Ok(());
        }

        if self.remote {
            self.fetch(true)?;
            let remote_branches = GitBranch::list_remote("origin")?;
            for branch in remote_branches {
                println!("{branch}");
            }
            return Ok(());
        }

        let mut table = Table::with_capacity(branches.len() + 1);
        table.add(vec![
            String::from(""),
            String::from("Name"),
            String::from("Status"),
        ]);
        for branch in branches {
            let cur = if branch.current {
                String::from("*")
            } else {
                String::new()
            };
            let status = match branch.status {
                BranchStatus::Sync => {
                    TableCell::with_color(String::from("sync"), TableCellColor::Green)
                }
                BranchStatus::Gone => {
                    TableCell::with_color(String::from("gone"), TableCellColor::Red)
                }
                BranchStatus::Ahead => {
                    TableCell::with_color(String::from("ahead"), TableCellColor::Yellow)
                }
                BranchStatus::Behind => {
                    TableCell::with_color(String::from("behind"), TableCellColor::Yellow)
                }
                BranchStatus::Conflict => {
                    TableCell::with_color(String::from("conflict"), TableCellColor::Yellow)
                }
                BranchStatus::Detached => {
                    TableCell::with_color(String::from("detached"), TableCellColor::Red)
                }
            };
            let row = vec![
                TableCell::no_color(cur),
                TableCell::no_color(branch.name.clone()),
                status,
            ];
            table.add_color(row);
        }

        if self.all {
            self.fetch(true)?;
            let remote_branches = GitBranch::list_remote("origin")?;
            let status = format!("{}", BranchStatus::Detached.display());
            for branch in remote_branches {
                let row = vec![String::new(), branch, status.clone()];
                table.add(row);
            }
        }

        table.show();
        Ok(())
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
            eprintln!("Nothing to sync");
            return Ok(());
        }

        eprintln!("Backup branch is {}", style(back).magenta());
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
                            .execute()?;
                        current = branch;
                    }
                    Cmd::git(&[op]).with_display_cmd().execute()?;
                }
                SyncBranchTask::Delete(branch) => {
                    if current == branch {
                        // we cannot delete branch when we are inside it, checkout
                        // to default branch first.
                        let default = default.as_str();
                        Cmd::git(&["checkout", default])
                            .with_display_cmd()
                            .execute()?;
                        current = default;
                    }
                    Cmd::git(&["branch", "-D", branch])
                        .with_display_cmd()
                        .execute()?;
                }
            }
        }
        if current != back {
            Cmd::git(&["checkout", back]).with_display_cmd().execute()?;
        }

        Ok(())
    }

    fn fetch(&self, mute: bool) -> Result<()> {
        let mut cmd = Cmd::git(&["fetch", "origin", "--prune"]);
        if !mute {
            cmd = cmd.with_display_cmd();
        }
        cmd.execute()
    }

    fn delete(&self, branches: &[GitBranch]) -> Result<()> {
        let branch = self.get_branch_or_current(branches)?;

        if branch.current {
            git::ensure_no_uncommitted()?;
            let default = GitBranch::default()?;
            if branch.name.eq(&default) {
                bail!("could not delete default branch");
            }
            Cmd::git(&["checkout", default.as_str()])
                .with_display_cmd()
                .execute()?;
        }

        Cmd::git(&["branch", "-D", &branch.name])
            .with_display_cmd()
            .execute()?;
        if self.push {
            Cmd::git(&["push", "origin", "--delete", &branch.name])
                .with_display_cmd()
                .execute()?;
        }
        Ok(())
    }

    fn push(&self, branches: &[GitBranch]) -> Result<()> {
        let branch = self.get_branch_or_current(branches)?;
        Cmd::git(&["push", "--set-upstream", "origin", branch.name.as_ref()])
            .with_display_cmd()
            .execute()
    }

    fn get_branch_or_current<'a>(&self, branches: &'a [GitBranch]) -> Result<&'a GitBranch> {
        match &self.name {
            Some(name) => match branches.iter().find(|b| b.name.eq(name)) {
                Some(b) => Ok(b),
                None => bail!("could not find branch '{}'", style(&name).yellow()),
            },
            None => Self::must_get_current_branch(branches),
        }
    }

    fn must_get_current_branch(branches: &[GitBranch]) -> Result<&GitBranch> {
        match branches.iter().find(|b| b.current) {
            Some(b) => Ok(b),
            None => bail!("could not find current branch"),
        }
    }

    fn search_and_switch(&self, branches: &[GitBranch]) -> Result<()> {
        if self.remote {
            return self.search_and_switch_remote();
        }

        let mut items: Vec<_> = branches
            .iter()
            .filter_map(|b| {
                if b.current {
                    None
                } else {
                    Some(b.name.clone())
                }
            })
            .collect();
        if self.all {
            self.fetch(false)?;
            let remote_branches = GitBranch::list_remote("origin")?;
            items.extend(remote_branches);
        }
        if items.is_empty() {
            eprintln!("No branch to switch");
            return Ok(());
        }

        let idx = exec::fzf_search(&items)?;

        let target = items[idx].as_str();

        Cmd::git(&["checkout", target]).execute()
    }

    fn search_and_switch_remote(&self) -> Result<()> {
        self.fetch(false)?;
        let branches = GitBranch::list_remote("origin")?;
        if branches.is_empty() {
            eprintln!("No remote branch to switch");
            return Ok(());
        }

        let idx = exec::fzf_search(&branches)?;
        let target = branches[idx].as_str();

        Cmd::git(&["checkout", target]).execute()
    }

    pub fn completion() -> Completion {
        Completion {
            args: Completion::branch_args,
            flags: None,
        }
    }
}
