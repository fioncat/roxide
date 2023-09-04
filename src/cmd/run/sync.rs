use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use clap::Args;
use regex::Regex;

use crate::batch::{self, Task};
use crate::cmd::Run;
use crate::config::types::{Remote, SyncAction, SyncRule};
use crate::repo::database::Database;
use crate::repo::query::Query;
use crate::repo::types::{NameLevel, Repo};
use crate::shell::{BranchStatus, GitBranch, GitTask, Shell};
use crate::{config, info, utils};

struct SyncTask {
    remote: Arc<Remote>,
    owner: String,

    name: String,

    path: PathBuf,

    push: bool,
    pull: bool,
    add: bool,
    delete: bool,
    force: bool,

    branch_re: Arc<Regex>,

    message: Arc<Option<String>>,
}

impl SyncTask {
    fn from_repos(
        repos: Vec<Rc<Repo>>,
        add: bool,
        push: bool,
        pull: bool,
        delete: bool,
        force: bool,
        message: Option<String>,
        level: &NameLevel,
    ) -> Result<Vec<(String, SyncTask)>> {
        let message = Arc::new(message);
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
            tasks.push((
                repo.as_string(level),
                SyncTask {
                    remote,
                    owner: format!("{}", repo.owner),
                    name: format!("{}", repo.name),
                    path: repo.get_path(),
                    branch_re: branch_re.clone(),
                    add,
                    push,
                    pull,
                    delete,
                    force,
                    message: message.clone(),
                },
            ));
        }

        Ok(tasks)
    }
}

impl Task<()> for SyncTask {
    fn run(&self) -> Result<()> {
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
            Shell::exec_git_mute(&["clone", url.as_str(), path.as_str()])?;
        } else {
            git.exec(&["remote", "set-url", "origin", url.as_str()])?;
            git.exec(&["fetch", "origin", "--prune"])?;
        }
        if let Some(user) = &self.remote.user {
            git.exec(&["config", "user.name", user.as_str()])?;
        }
        if let Some(email) = &self.remote.email {
            git.exec(&["config", "user.email", email.as_str()])?;
        }

        let lines = git.lines(&["status", "-s"])?;
        if !lines.is_empty() {
            if let Some(msg) = self.message.as_ref() {
                git.exec(&["commit", "-m", msg.as_str()])?;
            } else {
                bail!("Have uncommitted change(s), skip synchronization");
            }
        }

        let lines = git.lines(&["remote", "show", "origin"])?;
        let default_branch = GitBranch::parse_default_branch(lines)?;
        let mut backup_branch = default_branch.clone();

        let current = git.read(&["branch", "--show-current"])?;
        let head = if current.is_empty() {
            let head = git.read(&["rev-parse", "HEAD"])?;
            if head.is_empty() {
                bail!("HEAD commit is empty, please check your git command");
            }
            git.exec(&["checkout", &default_branch])?;
            Some(head)
        } else {
            None
        };

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
                    if !self.push {
                        continue;
                    }
                    git.exec(&["checkout", &branch.name])?;
                    git.exec(&["push"])?;
                }
                BranchStatus::Behind => {
                    if !self.pull {
                        continue;
                    }
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
                    git.exec(&["checkout", &default_branch])?;
                    git.exec(&["branch", "-D", &branch.name])?;
                }
                BranchStatus::Conflict => {
                    if !self.force {
                        continue;
                    }
                    git.exec(&["checkout", &branch.name])?;
                    git.exec(&["push", "-f"])?;
                }
                BranchStatus::Detached => {
                    if !self.add {
                        continue;
                    }
                    git.exec(&["checkout", &branch.name])?;
                    git.exec(&["push", "--set-upstream", "origin", &branch.name])?;
                }
                BranchStatus::Sync => {}
            }
        }

        let target = match head.as_ref() {
            Some(commit) => commit,
            None => &backup_branch,
        };
        git.exec(&["checkout", target])?;

        Ok(())
    }
}

/// Sync repos git branches.
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
            Some(String::from("fire commit"))
        } else {
            self.message.clone()
        };

        let tasks = SyncTask::from_repos(repos, add, true, true, delete, force, message, &level)?;
        batch::must_run("Sync", tasks)?;

        Ok(())
    }
}

/// Use rule in config to sync repos.
#[derive(Args)]
pub struct SyncRuleArgs {
    /// The sync rule name. If empty, use "default".
    pub name: Option<String>,
}

impl Run for SyncRuleArgs {
    fn run(&self) -> Result<()> {
        let rule = self.get_rule()?;
        let repos = self.select_repos(&rule)?;
        if repos.is_empty() {
            info!("Nothing to sync");
            return Ok(());
        }

        let items: Vec<String> = repos
            .iter()
            .map(|repo| repo.as_string(&Self::LEVEL))
            .collect();
        utils::confirm_items(&items, "sync", "synchronization", "Repo", "Repos")?;

        let mut pull = false;
        let mut push = false;
        let mut delete = false;
        let mut add = false;
        let mut force = false;
        for action in rule.actions.iter() {
            match action {
                SyncAction::Pull => pull = true,
                SyncAction::Push => push = true,
                SyncAction::Delete => delete = true,
                SyncAction::Add => add = true,
                SyncAction::Force => force = true,
            }
        }

        let tasks =
            SyncTask::from_repos(repos, add, push, pull, delete, force, None, &Self::LEVEL)?;
        batch::must_run("Sync", tasks)?;

        Ok(())
    }
}

impl SyncRuleArgs {
    const LEVEL: NameLevel = NameLevel::Full;

    fn get_rule(&self) -> Result<&'static SyncRule> {
        match self.name.as_ref() {
            Some(name) => match config::base().sync.get(name) {
                Some(sync) => Ok(sync),
                None => bail!("Could not find sync rule {name}"),
            },
            None => match config::base().sync.get("default") {
                Some(sync) => Ok(sync),
                None => bail!("Could not find default sync rule"),
            },
        }
    }

    fn select_repos(&self, rule: &SyncRule) -> Result<Vec<Rc<Repo>>> {
        let db = Database::read()?;
        if rule.select.len() == 0 {
            return Ok(db.list_all());
        }

        let mut repos = Vec::new();
        for (remote, owners) in rule.select.iter() {
            if owners == "*" {
                repos.extend(db.list_by_remote(remote));
                continue;
            }
            let owners = owners.split(",");
            for owner in owners {
                repos.extend(db.list_by_owner(remote.as_str(), owner));
            }
        }

        Ok(repos)
    }
}
