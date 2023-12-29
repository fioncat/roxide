use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::{fs, io};

use anyhow::{bail, Context, Result};
use clap::Args;
use console::style;

use crate::batch::{self, Task};
use crate::cmd::{Completion, CompletionResult, Run};
use crate::config::{Config, RemoteConfig};
use crate::repo::database::Database;
use crate::repo::snapshot::{Item, Snapshot};
use crate::repo::Repo;
use crate::term::{self, Cmd, GitCmd};
use crate::{hashset_strings, info, stderrln, utils};

/// Snapshot operations for workspace
#[derive(Args)]
pub struct SnapshotArgs {
    /// The snapshot name.
    pub name: Option<String>,

    /// Recover workspace with snapshot, without this, we will use current workspace
    /// to create a snapshot.
    #[clap(short)]
    pub restore: bool,

    /// Only available in recover mode, with this, we will skip checkout step.
    #[clap(short)]
    pub skip_checkout: bool,

    /// Use the labels to filter repo.
    #[clap(short)]
    pub labels: Option<String>,

    /// Force restore, ignore "pin" label.
    #[clap(short)]
    pub force: bool,
}

impl Run for SnapshotArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
        let name = match self.name.as_ref() {
            Some(name) => name.clone(),
            None => {
                let names = Snapshot::list(cfg)?;
                for name in names {
                    println!("{name}");
                }
                return Ok(());
            }
        };
        if !self.restore {
            return self.create(cfg, name);
        }
        self.restore(cfg, name)
    }
}

impl SnapshotArgs {
    fn create(&self, cfg: &Config, name: String) -> Result<()> {
        let db = Database::load(cfg)?;
        let labels = utils::parse_labels(&self.labels);
        let repos: Vec<_> = db
            .list_all(&labels)
            .into_iter()
            .filter_map(|repo| {
                if let None = repo.remote_cfg.clone {
                    return None;
                }
                Some(repo)
            })
            .collect();

        if repos.is_empty() {
            stderrln!("No repo to create snapshot");
            return Ok(());
        }
        let items: Vec<_> = repos.iter().map(|repo| repo.name_with_remote()).collect();
        term::must_confirm_items(&items, "take snapshot", "snapshot", "repo", "repos")?;

        let mut tasks = Vec::with_capacity(repos.len());
        for repo in repos.iter() {
            let path = repo.get_path(cfg);
            let name = repo.name_with_remote();
            tasks.push((name, CheckSnapshotTask { path }));
        }
        let results = batch::run("Check", tasks, false);
        stderrln!();

        let mut failed = Vec::new();
        for (idx, result) in results.iter().enumerate() {
            if let Err(err) = result {
                let name = repos[idx].name_with_remote();
                let msg = format!("  * {}: {}", style(name).cyan(), err);
                failed.push(msg);
            }
        }
        if !failed.is_empty() {
            stderrln!(
                "Found {} that failed the checks:",
                utils::plural(&failed, "repo")
            );
            for msg in failed {
                stderrln!("{}", msg);
            }
            stderrln!();
            bail!("Please handle them first");
        }

        let mut items: Vec<Item> = Vec::with_capacity(repos.len());
        for (idx, result) in results.into_iter().enumerate() {
            let branch = result.unwrap();
            let repo = &repos[idx];
            items.push(Item {
                remote: repo.remote.to_string(),
                owner: repo.owner.to_string(),
                name: repo.name.to_string(),
                branch,
                path: repo.path.as_ref().map(|path| path.to_string()),
                accessed: repo.accessed,
                last_accessed: repo.last_accessed,
                labels: repo
                    .labels
                    .as_ref()
                    .map(|labels| labels.iter().map(|label| label.to_string()).collect()),
            })
        }

        info!("Save snapshot {}", name);
        let snapshot = Snapshot::new(cfg, name, items);
        snapshot.save()
    }

    fn restore(&self, cfg: &Config, name: String) -> Result<()> {
        let mut db = Database::load(cfg)?;
        let snapshot = Snapshot::read(cfg, name)?;
        let items = snapshot.items;

        if items.is_empty() {
            bail!("no item in snapshot, please check it");
        }
        let show_items: Vec<_> = items
            .iter()
            .map(|item| format!("{}:{}/{}", item.remote, item.owner, item.name))
            .collect();
        term::must_confirm_items(&show_items, "restore snapshot", "snapshot", "Item", "Items")?;
        let items_set: HashSet<_> = show_items.into_iter().collect();

        let mut tasks = Vec::with_capacity(items.len());
        let mut remotes: HashMap<String, Arc<RemoteConfig>> = HashMap::new();
        for item in items.iter() {
            let remote_cfg = match remotes.get(item.remote.as_str()) {
                Some(remote) => remote.clone(),
                None => {
                    let remote_cfg = cfg.must_get_remote(item.remote.as_str())?;
                    let remote_cfg = Arc::new(remote_cfg.into_owned());
                    let ret = Arc::clone(&remote_cfg);
                    remotes.insert(item.remote.clone(), remote_cfg);
                    ret
                }
            };
            let path = match item.path.as_ref() {
                Some(path) => PathBuf::from(path),
                None => Repo::get_workspace_path(cfg, &item.remote, &item.owner, &item.name),
            };
            let show_name = format!("{}:{}/{}", item.remote, item.owner, item.name);
            tasks.push((
                show_name,
                RestoreSnapshotTask {
                    remote_cfg,
                    path,
                    branch: if self.skip_checkout {
                        None
                    } else {
                        Some(item.branch.clone())
                    },
                    name: item.name.clone(),
                    owner: item.owner.clone(),
                },
            ));
        }

        batch::must_run("Restore", tasks)?;
        stderrln!();

        let mut skip_remote = HashSet::new();
        for remote_name in cfg.list_remote_names() {
            let remote_cfg = cfg.must_get_remote(&remote_name)?;
            if let None = remote_cfg.clone.as_ref() {
                skip_remote.insert(remote_name);
            }
        }

        let pin_labels = if self.force {
            None
        } else {
            Some(hashset_strings!["pin"])
        };
        let to_remove: Vec<Repo> = db
            .list_all(&pin_labels)
            .iter()
            .filter(|repo| !skip_remote.contains(repo.remote.as_ref()))
            .filter(|repo| !items_set.contains(&repo.name_with_remote()))
            .map(|repo| repo.clone())
            .collect();

        if !to_remove.is_empty() {
            let remove_items: Vec<_> = to_remove
                .iter()
                .map(|repo| repo.name_with_remote())
                .collect();
            if term::confirm_items(&remove_items, "remove", "removal", "Repo", "Repos")? {
                let mut update_repos = Vec::with_capacity(to_remove.len());
                for repo in to_remove {
                    let path = repo.get_path(cfg);
                    utils::remove_dir_recursively(path)?;
                    update_repos.push(repo.update());
                }
                for repo in update_repos {
                    db.remove(repo);
                }
            } else {
                drop(to_remove);
            }
        }

        for item in items {
            let Item {
                remote,
                owner,
                name,
                branch: _,
                path,
                last_accessed,
                accessed,
                labels,
            } = item;
            let mut repo = Repo::new(
                cfg,
                Cow::Owned(remote),
                Cow::Owned(owner),
                Cow::Owned(name),
                path,
            )?;
            repo.last_accessed = last_accessed;
            repo.accessed = accessed;
            repo.append_labels(labels);
            info!("Restore repo {} database", repo.name_with_remote());
            db.upsert(repo);
        }

        db.save()
    }

    pub fn completion() -> Completion {
        Completion {
            args: |cfg, args| match args.len() {
                0 | 1 => {
                    let names = Snapshot::list(cfg)?;
                    Ok(CompletionResult::from(names))
                }
                _ => Ok(CompletionResult::empty()),
            },

            flags: Some(Completion::labels),
        }
    }
}

struct CheckSnapshotTask {
    path: PathBuf,
}

impl Task<String> for CheckSnapshotTask {
    fn run(&self) -> Result<String> {
        let path = format!("{}", self.path.display());
        let git = GitCmd::with_path(path.as_str());

        let lines = git.lines(&["status", "-s"])?;
        if !lines.is_empty() {
            bail!("found {}", utils::plural(&lines, "uncommitted change"));
        }

        git.exec(&["fetch", "--all"])?;
        let branch = git.read(&["branch", "--show-current"])?;
        if branch.is_empty() {
            bail!("please switch to an existing branch");
        }
        let origin_target = format!("origin/{branch}");
        let compare = format!("{origin_target}..HEAD");
        let lines = git.lines(&["log", "--oneline", compare.as_str()])?;
        if !lines.is_empty() {
            bail!(
                "found {}",
                utils::plural(&lines, "un-pushed/un-pulled commit")
            );
        }

        Ok(branch)
    }
}

struct RestoreSnapshotTask {
    remote_cfg: Arc<RemoteConfig>,
    path: PathBuf,
    branch: Option<String>,

    owner: String,
    name: String,
}

impl Task<()> for RestoreSnapshotTask {
    fn run(&self) -> Result<()> {
        let path = format!("{}", self.path.display());
        let git = GitCmd::with_path(path.as_str());
        match fs::read_dir(&self.path) {
            Ok(_) => git.exec(&["fetch", "--all"])?,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                let url =
                    Repo::get_clone_url(self.owner.as_str(), self.name.as_str(), &self.remote_cfg);
                Cmd::git(&["clone", url.as_str(), path.as_str()]).execute_check()?;
            }
            Err(err) => return Err(err).with_context(|| format!("read dir {}", path)),
        }
        if let Some(branch) = self.branch.as_ref() {
            git.exec(&["checkout", branch.as_str()])?;
        }
        Ok(())
    }
}
