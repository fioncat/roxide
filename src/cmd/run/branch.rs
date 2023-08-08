use anyhow::{bail, Context, Result};
use clap::Args;
use console::style;

use crate::cmd::Run;
use crate::confirm;
use crate::shell::{self, BranchStatus, GitBranch, Shell};

/// Git branch operations
#[derive(Args)]
pub struct BranchArgs {
    /// Branch name, optional
    pub name: Option<String>,

    /// Show all info, include branch status
    #[clap(long, short)]
    pub all: bool,

    /// Sync branch with remote
    #[clap(long, short)]
    pub sync: bool,

    /// Create a new branch
    #[clap(long, short)]
    pub create: bool,

    /// Delete branch
    #[clap(long, short)]
    pub delete: bool,

    /// Push change (create or delete) to remote
    #[clap(long, short)]
    pub push: bool,

    /// Search and switch to remote branch
    #[clap(long, short)]
    pub remote: bool,
}

enum SyncBranchTask<'a> {
    Sync(&'a str, &'a str),
    Delete(&'a str),
}

impl Run for BranchArgs {
    fn run(&self) -> Result<()> {
        if self.sync {
            shell::ensure_no_uncommitted()?;
            self.fetch()?;
        }
        if self.remote {
            return self.search_and_switch_remote();
        }
        let branches = GitBranch::list().context("List branch")?;
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
            Shell::git(&["checkout", "-b", name.as_str()])
                .execute()?
                .check()?;
        } else {
            Shell::git(&["checkout", name.as_str()])
                .execute()?
                .check()?;
        }
        if self.push {
            Shell::git(&["push", "--set-upstream", "origin", name.as_str()])
                .execute()?
                .check()?;
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

        println!();
        if tasks.is_empty() {
            println!("Nothing to do");
            return Ok(());
        }

        println!("Backup branch is {}", style(back).magenta());
        let word = if tasks.len() == 1 { "task" } else { "tasks" };
        println!("Sync {} ({}):", word, tasks.len());
        for task in &tasks {
            match task {
                SyncBranchTask::Sync(op, branch) => {
                    println!("{} {} {} ", style("+").green(), op, style(branch).magenta())
                }
                SyncBranchTask::Delete(branch) => {
                    println!("{} delete {} ", style("-").red(), style(branch).magenta())
                }
            }
        }
        confirm!("Continue");

        println!();
        for task in tasks {
            match task {
                SyncBranchTask::Sync(op, branch) => {
                    if current != branch {
                        // checkout to this branch to perform push/pull
                        Shell::git(&["checkout", branch]).execute()?.check()?;
                        current = branch;
                    }
                    Shell::git(&[op]).execute()?.check()?;
                }
                SyncBranchTask::Delete(branch) => {
                    if current == branch {
                        // we cannot delete branch when we are inside it, checkout
                        // to default branch first.
                        let default = default.as_str();
                        Shell::git(&["checkout", default]).execute()?.check()?;
                        current = default;
                    }
                    Shell::git(&["branch", "-D", branch]).execute()?.check()?;
                }
            }
        }
        if current != back {
            Shell::git(&["checkout", back]).execute()?.check()?;
        }

        Ok(())
    }

    fn fetch(&self) -> Result<()> {
        let mut git = Shell::git(&["fetch", "origin", "--prune"]);
        git.execute()?.check()?;
        Ok(())
    }

    fn delete(&self, branches: &Vec<GitBranch>) -> Result<()> {
        let branch = self.get_branch_or_current(branches)?;

        if branch.current {
            shell::ensure_no_uncommitted()?;
            let default = GitBranch::default()?;
            if branch.name.eq(&default) {
                bail!("Could not delete default branch");
            }
            Shell::git(&["checkout", default.as_str()])
                .execute()?
                .check()?;
        }

        Shell::git(&["branch", "-D", &branch.name])
            .execute()?
            .check()?;
        if self.push {
            Shell::git(&["push", "origin", "--delete", &branch.name])
                .execute()?
                .check()?;
        }
        Ok(())
    }

    fn push(&self, branches: &Vec<GitBranch>) -> Result<()> {
        let branch = self.get_branch_or_current(branches)?;
        Shell::git(&["push", "--set-upstream", "origin", branch.name.as_ref()])
            .execute()?
            .check()?;
        Ok(())
    }

    fn get_branch_or_current<'a>(&self, branches: &'a Vec<GitBranch>) -> Result<&'a GitBranch> {
        match &self.name {
            Some(name) => match branches.iter().find(|b| b.name.eq(name)) {
                Some(b) => Ok(b),
                None => bail!("Could not find branch {}", style(&name).yellow()),
            },
            None => Self::must_get_current_branch(branches),
        }
    }

    fn must_get_current_branch(branches: &Vec<GitBranch>) -> Result<&GitBranch> {
        match branches.iter().find(|b| b.current) {
            Some(b) => Ok(b),
            None => bail!("Could not find current branch"),
        }
    }

    fn search_and_switch_remote(&self) -> Result<()> {
        self.fetch()?;
        let branches = GitBranch::list_remote("origin")?;
        if branches.is_empty() {
            println!("No remote branch to switch");
            return Ok(());
        }

        let idx = shell::search(&branches)?;
        let target = branches[idx].as_str();

        Shell::git(&["checkout", target]).execute()?.check()
    }
}
