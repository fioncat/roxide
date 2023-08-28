use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Args;
use regex::Regex;

use crate::batch::{self, Reporter, Task};
use crate::cmd::Run;
use crate::config::types::Remote;
use crate::repo::database::Database;
use crate::repo::query::Query;
use crate::repo::types::Repo;
use crate::shell::{BranchStatus, GitBranch, GitTask, Shell};
use crate::{config, info, utils};

/// Sync database and workspace.
#[derive(Args)]
pub struct SyncArgs {
    /// The remote name.
    pub remote: Option<String>,

    /// The repo query, format is `owner[/[name]]`.
    pub query: Option<String>,

    /// Delete gone branch in local.
    #[clap(long, short)]
    pub delete: bool,

    /// Push detached branch to remote.
    #[clap(long, short)]
    pub add: bool,

    /// Force push conflict branch to remote.
    #[clap(long, short)]
    pub force: bool,

    /// Commit message if have uncommitted changes in current branch.
    #[clap(long, short)]
    pub message: Option<String>,

    /// Enable all options, if you are in an emergency situation and need to sync
    /// the remote quickly, use it. This also skips confirmation.
    #[clap(long)]
    pub fire: bool,

    /// Filter repo
    #[clap(long)]
    pub filter: bool,
}

struct SyncTask {
    remote: Arc<Remote>,
    owner: String,

    name: String,

    path: PathBuf,

    show_name: String,

    add: bool,
    delete: bool,
    force: bool,

    branch_re: Arc<Regex>,

    message: Arc<Option<String>>,
}

impl Task<()> for SyncTask {
    fn run(&self, rp: &Reporter<()>) -> Result<()> {
        if let None = self.remote.clone.as_ref() {
            return Ok(());
        }
        let clone = match fs::read_dir(&self.path) {
            Ok(_) => false,
            Err(err) if err.kind() == ErrorKind::NotFound => true,
            Err(err) => {
                return Err(err).with_context(|| format!("Read dir {}", self.path.display()))
            }
        };

        let path = format!("{}", self.path.display());
        let git = GitTask::new(path.as_str());

        let url = Repo::get_clone_url(self.owner.as_str(), self.name.as_str(), &self.remote);
        if clone {
            rp.message(format!("Cloning {}...", self.show_name));
            Shell::exec_git_mute(&["clone", url.as_str(), path.as_str()])?;
        } else {
            rp.message(format!("Fetching {}...", self.show_name));
            git.exec(&["remote", "set-url", "origin", url.as_str()])?;
            git.exec(&["fetch", "--all"])?;
        }
        if let Some(user) = &self.remote.user {
            git.exec(&["config", "user.name", user.as_str()])?;
        }
        if let Some(email) = &self.remote.email {
            git.exec(&["config", "user.email", email.as_str()])?;
        }

        rp.message(format!("Ensuring changes for {}", self.show_name));
        let lines = git.lines(&["status", "-s"])?;
        let mut stash = false;
        if !lines.is_empty() {
            if let Some(msg) = self.message.as_ref() {
                git.exec(&["commit", "-m", msg.as_str()])?;
            } else {
                git.exec(&["stash", "push"])?;
                stash = true;
            }
        }

        rp.message(format!("Getting default branch for {}", self.show_name));
        let lines = git.lines(&["remote", "show", "origin"])?;
        let default_branch = GitBranch::parse_default_branch(lines)?;
        let mut backup_branch = default_branch.clone();

        let lines = git.lines(&["branch", "-vv"])?;
        for line in lines {
            let branch = GitBranch::parse(&self.branch_re, line.as_str())?;
            if branch.current {
                match branch.status {
                    BranchStatus::Gone => {}
                    _ => backup_branch = branch.name.clone(),
                }
            }
            match branch.status {
                BranchStatus::Ahead => {
                    rp.message(format!("Pushing {} for {}", branch.name, self.show_name));
                    git.exec(&["checkout", &branch.name])?;
                    git.exec(&["push"])?;
                }
                BranchStatus::Behind => {
                    rp.message(format!("Pulling {} for {}", branch.name, self.show_name));
                    git.exec(&["checkout", &branch.name])?;
                    git.exec(&["pull"])?;
                }
                BranchStatus::Gone => {
                    if !self.delete {
                        continue;
                    }
                    if branch.name == default_branch {
                        continue;
                    }
                    rp.message(format!("Deleting {} for {}", branch.name, self.show_name));
                    git.exec(&["checkout", &default_branch])?;
                    git.exec(&["branch", "-D", &branch.name])?;
                }
                BranchStatus::Conflict => {
                    if !self.force {
                        continue;
                    }
                    rp.message(format!(
                        "Force pushing {} for {}",
                        branch.name, self.show_name
                    ));
                    git.exec(&["checkout", &branch.name])?;
                    git.exec(&["push", "-f"])?;
                }
                BranchStatus::Detached => {
                    if !self.add {
                        continue;
                    }
                    rp.message(format!(
                        "Setupstream pushing {} for {}",
                        branch.name, self.show_name
                    ));
                    git.exec(&["checkout", &branch.name])?;
                    git.exec(&["push", "--set-upstream", "origin", &branch.name])?;
                }
                BranchStatus::Sync => {}
            }
        }

        git.exec(&["checkout", &backup_branch])?;
        if stash {
            git.exec(&["stash", "pop"])?;
        }
        Ok(())
    }

    fn message_done(&self, result: &Result<()>) -> String {
        match result {
            Ok(_) => format!("Sync {} done", self.show_name),
            Err(err) => format!("Sync {} error: {}", self.show_name, err),
        }
    }
}

impl Run for SyncArgs {
    fn run(&self) -> Result<()> {
        let db = Database::read()?;

        let query = Query::from_args(&db, &self.remote, &self.query);
        let (repos, level) = query.list_local(self.filter)?;
        if repos.is_empty() {
            info!("Nothing to sync");
            return Ok(());
        }

        if !self.fire {
            let items: Vec<String> = repos.iter().map(|repo| repo.as_string(&level)).collect();
            utils::confirm_items(&items, "sync", "synchronization", "Repo", "Repos")?;
        }

        let add = if self.fire { true } else { self.add };
        let delete = if self.fire { true } else { self.delete };
        let force = if self.fire { true } else { self.force };
        let message = if self.fire {
            Arc::new(Some(String::from("fire commit")))
        } else {
            Arc::new(self.message.clone())
        };

        let branch_re = Arc::new(GitBranch::get_regex());

        let mut tasks = Vec::with_capacity(repos.len());
        let mut remotes: HashMap<String, Arc<Remote>> = HashMap::new();
        for repo in repos {
            let remote = match remotes.get(repo.remote.as_str()) {
                Some(remote) => remote.clone(),
                None => {
                    let remote = config::must_get_remote(repo.remote.as_str())?;
                    let remote = Arc::new(remote);
                    let ret = remote.clone();
                    remotes.insert(format!("{}", repo.remote), remote);
                    ret
                }
            };
            tasks.push(SyncTask {
                remote,
                owner: format!("{}", repo.owner),
                name: format!("{}", repo.name),
                path: repo.get_path(),
                show_name: repo.as_string(&level),
                branch_re: branch_re.clone(),
                add,
                delete,
                force,
                message: message.clone(),
            });
        }
        let _ = batch::run("Sync", tasks);

        Ok(())
    }
}
