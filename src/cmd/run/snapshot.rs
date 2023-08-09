use std::path::PathBuf;

use anyhow::{bail, Result};
use clap::Args;
use console::style;

use crate::batch::{self, Reporter, Task};
use crate::cmd::Run;
use crate::repo::database::Database;
use crate::repo::snapshot::{Item, Snapshot};
use crate::shell::Shell;
use crate::{info, utils};

/// Snapshot operations for workspace
#[derive(Args)]
pub struct SnapshotArgs {
    /// The snapshot name.
    pub name: Option<String>,

    /// Recover worksapce with snapshot, without this, we will use current worksapce
    /// to create a snapshot.
    #[clap(long, short)]
    pub restore: bool,

    /// Only available in recover mode, with this, we will skip clear step.
    #[clap(long)]
    pub skip_clear: bool,

    /// Only available in recover mode, with this, we will skip checkout step.
    #[clap(long)]
    pub skip_checkout: bool,
}

impl Run for SnapshotArgs {
    fn run(&self) -> Result<()> {
        let name = match self.name.as_ref() {
            Some(name) => name.clone(),
            None => {
                let names = Snapshot::list()?;
                for name in names {
                    println!("{name}");
                }
                return Ok(());
            }
        };
        if !self.restore {
            return self.create(name);
        }
        self.restore(name)
    }
}

impl SnapshotArgs {
    fn create(&self, name: String) -> Result<()> {
        let db = Database::read()?;
        let repos = db.list_all();
        let items: Vec<_> = repos.iter().map(|repo| repo.full_name()).collect();
        utils::confirm_items(items, "take snapshot", "snapshot", "repo", "repos")?;

        let mut tasks: Vec<CheckSnapshotTask> = Vec::with_capacity(repos.len());
        for repo in repos.iter() {
            let path = repo.get_path();
            let name = repo.full_name();
            tasks.push(CheckSnapshotTask {
                path,
                show_name: name,
            });
        }
        let results = batch::run("Check", tasks);
        println!();

        let mut failed = Vec::new();
        for (idx, result) in results.iter().enumerate() {
            if let Err(err) = result {
                let name = repos[idx].full_name();
                let msg = format!("  * {}: {}", style(name).cyan(), err);
                failed.push(msg);
            }
        }
        if !failed.is_empty() {
            println!(
                "Found {} that failed the checks:",
                utils::plural(&failed, "repo")
            );
            for msg in failed {
                println!("{msg}");
            }
            println!();
            bail!("Please handle them first");
        }

        let mut items: Vec<Item> = Vec::with_capacity(repos.len());
        for (idx, result) in results.into_iter().enumerate() {
            let branch = result.unwrap();
            let repo = &repos[idx];
            items.push(Item {
                remote: format!("{}", repo.remote),
                owner: format!("{}", repo.owner),
                name: format!("{}", repo.name),
                branch,
                path: match repo.path.as_ref() {
                    Some(path) => Some(format!("{}", path)),
                    None => None,
                },
                acceseed: repo.accessed,
                last_accessed: repo.last_accessed,
            })
        }

        info!("Save snapshot {}", name);
        let snapshot = Snapshot::new(name, items);
        snapshot.save()
    }

    fn restore(&self, name: String) -> Result<()> {
        let _snapshot = Snapshot::read(name)?;
        todo!()
    }
}

struct CheckSnapshotTask {
    path: PathBuf,
    show_name: String,
}

impl Task<String> for CheckSnapshotTask {
    fn run(&self, rp: &Reporter<String>) -> Result<String> {
        rp.message(format!("Checking {}...", self.show_name));

        let path = format!("{}", self.path.display());
        let lines = Shell::exec_git_mute_lines(&["-C", path.as_str(), "status", "-s"])?;
        if !lines.is_empty() {
            bail!("Found {}", utils::plural(&lines, "uncommitted change"));
        }

        Shell::exec_git_mute(&["-C", path.as_str(), "fetch", "--all"])?;
        let branch = Shell::exec_git_mute_read(&["-C", path.as_str(), "branch", "--show-current"])?;
        if branch.is_empty() {
            bail!("Please switch to an existing branch");
        }
        let origin_target = format!("origin/{branch}");
        let compare = format!("{origin_target}..HEAD");
        let lines = Shell::exec_git_mute_lines(&[
            "-C",
            path.as_str(),
            "log",
            "--oneline",
            compare.as_str(),
        ])?;
        if !lines.is_empty() {
            bail!(
                "Found {}",
                utils::plural(&lines, "unpushed/unpulled commit")
            );
        }

        Ok(branch)
    }

    fn message_done(&self, _result: &Result<String>) -> String {
        format!("Check {} done", self.show_name)
    }
}
