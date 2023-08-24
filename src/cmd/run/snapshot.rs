use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use clap::Args;
use console::style;

use crate::batch::{self, Reporter, Task};
use crate::cmd::Run;
use crate::config::types::Remote;
use crate::repo::database::Database;
use crate::repo::snapshot::{Item, Snapshot};
use crate::repo::types::Repo;
use crate::shell::Shell;
use crate::{config, info, utils};

/// Snapshot operations for workspace
#[derive(Args)]
pub struct SnapshotArgs {
    /// The snapshot name.
    pub name: Option<String>,

    /// Recover worksapce with snapshot, without this, we will use current worksapce
    /// to create a snapshot.
    #[clap(long, short)]
    pub restore: bool,

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
        let all_repos = db.list_all();
        let mut repos = Vec::with_capacity(all_repos.len());
        let mut remotes: HashMap<String, Rc<Remote>> = HashMap::new();
        for repo in all_repos {
            let remote = match remotes.get(repo.remote.as_str()) {
                Some(remote) => remote.clone(),
                None => {
                    let remote = config::must_get_remote(repo.remote.as_str())?;
                    let remote = Rc::new(remote);
                    let ret = remote.clone();
                    remotes.insert(format!("{}", repo.remote), remote);
                    ret
                }
            };
            if let None = remote.clone.as_ref() {
                continue;
            }
            repos.push(repo);
        }

        if repos.is_empty() {
            info!("No repo to create snapshot");
            return Ok(());
        }
        let items: Vec<_> = repos.iter().map(|repo| repo.full_name()).collect();
        utils::confirm_items(&items, "take snapshot", "snapshot", "repo", "repos")?;

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
        let mut db = Database::read()?;
        let snapshot = Snapshot::read(name)?;
        let items = snapshot.items;

        if items.is_empty() {
            bail!("No item in snapshot, please check it");
        }
        let show_items: Vec<_> = items
            .iter()
            .map(|item| format!("{}:{}/{}", item.remote, item.owner, item.name))
            .collect();
        utils::confirm_items(&show_items, "restore snapshot", "snapshot", "Item", "Items")?;
        let items_set: HashSet<_> = show_items.into_iter().collect();

        let mut tasks = Vec::with_capacity(items.len());
        let mut remotes: HashMap<String, Arc<Remote>> = HashMap::new();
        for item in items.iter() {
            let remote = match remotes.get(item.remote.as_str()) {
                Some(remote) => remote.clone(),
                None => {
                    let remote = config::must_get_remote(item.remote.as_str())?;
                    let remote = Arc::new(remote);
                    let ret = remote.clone();
                    remotes.insert(format!("{}", item.remote), remote);
                    ret
                }
            };
            let path = match item.path.as_ref() {
                Some(path) => PathBuf::from(path),
                None => Repo::get_workspace_path(&item.remote, &item.owner, &item.name),
            };
            let show_name = format!("{}:{}/{}", item.remote, item.owner, item.name);
            tasks.push(RestoreSnapshotTask {
                remote,
                path,
                branch: if self.skip_checkout {
                    None
                } else {
                    Some(item.branch.clone())
                },
                name: item.name.clone(),
                owner: item.owner.clone(),
                show_name,
            });
        }

        if !batch::is_ok(&batch::run("Restore", tasks)) {
            bail!("Restore failed");
        }
        println!();

        let mut skip_remote = HashSet::new();
        for remote in config::list_remotes() {
            let remote_cfg = config::must_get_remote(remote)?;
            if let None = remote_cfg.clone.as_ref() {
                skip_remote.insert(remote);
            }
        }

        let to_remove: Vec<Rc<Repo>> = db
            .list_all()
            .iter()
            .filter(|repo| !skip_remote.contains(repo.remote.as_str()))
            .filter(|repo| !items_set.contains(&repo.full_name()))
            .map(|repo| repo.clone())
            .collect();

        if !to_remove.is_empty() {
            let remove_items: Vec<_> = to_remove.iter().map(|repo| repo.long_name()).collect();
            if utils::confirm_items_weak(&remove_items, "remove", "removal", "Repo", "Repos")? {
                for repo in to_remove {
                    let path = repo.get_path();
                    utils::remove_dir_recursively(path)?;
                    db.remove(repo);
                }
            } else {
                drop(to_remove);
            }
        }

        for item in items {
            let repo = Rc::new(Repo {
                remote: Rc::new(item.remote),
                owner: Rc::new(item.owner),
                name: Rc::new(item.name),
                accessed: item.acceseed,
                last_accessed: item.last_accessed,
                path: item.path,
            });
            info!("Restore repo {} storage", repo.full_name());
            db.add(repo);
        }

        db.close()
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

struct RestoreSnapshotTask {
    remote: Arc<Remote>,
    path: PathBuf,
    branch: Option<String>,

    owner: String,
    name: String,

    show_name: String,
}

impl Task<()> for RestoreSnapshotTask {
    fn run(&self, rp: &Reporter<()>) -> Result<()> {
        let path = format!("{}", self.path.display());
        match fs::read_dir(&self.path) {
            Ok(_) => {
                rp.message(format!("Fetching {}...", self.show_name));
                Shell::exec_git_mute(&["-C", path.as_str(), "fetch", "--all"])?;
            }
            Err(err) if err.kind() == ErrorKind::NotFound => {
                rp.message(format!("Cloning {}...", self.show_name));
                let url =
                    Repo::get_clone_url(self.owner.as_str(), self.name.as_str(), &self.remote);
                Shell::exec_git_mute(&["clone", url.as_str(), path.as_str()])?;
            }
            Err(err) => return Err(err).with_context(|| format!("Read dir {}", path)),
        }
        if let Some(branch) = self.branch.as_ref() {
            Shell::exec_git_mute(&["-C", path.as_str(), "checkout", branch.as_str()])?;
        }
        Ok(())
    }

    fn message_done(&self, result: &Result<()>) -> String {
        match result {
            Ok(_) => format!("Restore {} done", self.name),
            Err(err) => format!("Restore {} error: {}", self.name, err),
        }
    }
}
