use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::{fs, io};

use anyhow::{bail, Context, Result};
use clap::Args;
use regex::Regex;

use crate::batch::{self, Task};
use crate::cmd::{Completion, Run};
use crate::config::Config;
use crate::repo::database::{Database, SelectOptions, Selector};
use crate::repo::{NameLevel, Remote, Repo};
use crate::term::{self, BranchStatus, Cmd, GitBranch, GitCmd};
use crate::{hashset_strings, stderrln, utils};

/// Sync repos git branches.
#[derive(Args)]
pub struct SyncArgs {
    pub head: Option<String>,

    pub query: Option<String>,

    /// Commit message if have uncommitted changes in current branch.
    #[clap(short)]
    pub message: Option<String>,

    #[clap(short)]
    pub edit: bool,

    /// Only show effects, skip running
    #[clap(short)]
    pub dry_run: bool,

    #[clap(short, default_value = "push,pull,add,delete")]
    pub ops: String,

    /// Force sync, ignore label "sync".
    #[clap(short)]
    pub force: bool,

    #[clap(short)]
    pub labels: Option<String>,
}

impl Run for SyncArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
        let ops: HashSet<String> = self.ops.split(",").map(|s| s.to_string()).collect();
        if ops.is_empty() {
            bail!("invalid ops, could not be empty");
        }
        let all_ops = Self::get_all_ops();
        for op in ops.iter() {
            if !all_ops.contains(op) {
                let mut all_ops: Vec<String> = all_ops.into_iter().collect();
                all_ops.sort();
                let all_ops = all_ops.join(",");
                bail!("unsupported op '{op}', require one of '{all_ops}'");
            }
        }

        let db = Database::load(cfg)?;

        let filter_labels = utils::parse_labels(&self.labels);
        let filter_labels = if self.force {
            filter_labels
        } else {
            match filter_labels {
                Some(labels) => {
                    let mut new_labels = HashSet::with_capacity(labels.len() + 1);
                    new_labels.insert(String::from("sync"));
                    new_labels.extend(labels);
                    Some(new_labels)
                }
                None => Some(hashset_strings!["sync"]),
            }
        };

        let opts = SelectOptions::default()
            .with_filter_labels(filter_labels)
            .with_many_edit(self.edit);
        let selector = Selector::from_args(&db, &self.head, &self.query, opts);

        let (repos, level) = selector.many_local()?;
        if repos.is_empty() {
            stderrln!("No repo to sync");
            return Ok(());
        }

        let items: Vec<String> = repos.iter().map(|repo| repo.to_string(&level)).collect();
        term::must_confirm_items(&items, "sync", "synchronization", "Repo", "Repos")?;

        let tasks = self.build_tasks(cfg, repos, ops, &level)?;

        if self.dry_run {
            let results = batch::must_run::<_, Option<String>>("DryRun", tasks)?;
            Self::show_dry_run(results);
            return Ok(());
        }

        batch::must_run::<_, ()>("Sync", tasks)?;
        Ok(())
    }
}

impl SyncArgs {
    fn get_all_ops() -> HashSet<String> {
        hashset_strings!["push", "pull", "add", "delete", "force"]
    }

    fn build_tasks(
        &self,
        cfg: &Config,
        repos: Vec<Rc<Repo>>,
        ops: HashSet<String>,
        level: &NameLevel,
    ) -> Result<Vec<(String, SyncTask)>> {
        let message = Arc::new(self.message.clone());
        let branch_re = Arc::new(GitBranch::get_regex());
        let ops = Arc::new(ops);

        let mut tasks = Vec::with_capacity(repos.len());
        let mut remotes: HashMap<String, Arc<Remote>> = HashMap::new();
        let mut owners: HashMap<String, Arc<String>> = HashMap::new();

        for repo in repos {
            let remote = match remotes.get(repo.remote.name.as_str()) {
                Some(remote) => Arc::clone(remote),
                None => {
                    let remote_cfg = cfg.must_get_remote(repo.remote.name.as_str())?;
                    let remote = Remote {
                        name: repo.remote.name.to_string(),
                        cfg: remote_cfg.clone(),
                    };
                    let remote = Arc::new(remote);
                    let ret = Arc::clone(&remote);
                    remotes.insert(repo.remote.name.to_string(), remote);
                    ret
                }
            };

            let owner = match owners.get(repo.owner.name.as_str()) {
                Some(owner) => Arc::clone(owner),
                None => {
                    let owner = repo.owner.name.to_string();
                    let owner_arc = Arc::new(owner.clone());
                    let ret = Arc::clone(&owner_arc);
                    owners.insert(owner, owner_arc);
                    ret
                }
            };

            let path = repo.get_path(cfg);

            tasks.push((
                repo.to_string(level),
                SyncTask {
                    remote,
                    owner,
                    name: repo.name.clone(),
                    path,
                    ops: Arc::clone(&ops),
                    branch_re: Arc::clone(&branch_re),
                    message: Arc::clone(&message),
                },
            ));
        }
        Ok(tasks)
    }

    fn show_dry_run(results: Vec<Option<String>>) {
        let mut count: usize = 0;
        for result in results.iter() {
            if let Some(_) = result {
                count += 1;
            }
        }
        if count == 0 {
            println!();
            println!("Nothing to sync");
            return;
        }

        let items: Vec<_> = results
            .into_iter()
            .filter(|s| match s {
                Some(_) => true,
                None => false,
            })
            .map(|s| s.unwrap())
            .collect();
        println!();
        println!("DryRun: {} to perform", utils::plural(&items, "repo"));

        for item in items {
            println!("  {item}");
        }
    }

    pub fn completion() -> Completion {
        Completion {
            args: Completion::repo_args,
            flags: Some(|cfg, flag, to_complete| match flag {
                'l' => Completion::labels_flag(cfg, to_complete),
                'o' => Completion::multiple_values_flag(to_complete, Self::get_all_ops()),
                _ => Ok(None),
            }),
        }
    }
}

struct SyncTask {
    remote: Arc<Remote>,
    owner: Arc<String>,
    name: String,

    path: PathBuf,

    ops: Arc<HashSet<String>>,

    branch_re: Arc<Regex>,

    message: Arc<Option<String>>,
}

impl Task<()> for SyncTask {
    fn run(&self) -> Result<()> {
        if let None = self.remote.cfg.clone {
            return Ok(());
        }

        let need_clone = match fs::read_dir(&self.path) {
            Ok(_) => false,
            Err(err) if err.kind() == io::ErrorKind::NotFound => true,
            Err(err) => {
                return Err(err).with_context(|| format!("read repo dir '{}'", self.path.display()))
            }
        };

        let path = format!("{}", self.path.display());
        let git = GitCmd::with_path(&path);

        let url = Repo::get_clone_url(self.owner.as_str(), self.name.as_str(), &self.remote);
        if need_clone {
            Cmd::git(&["clone", url.as_str(), path.as_str()]).execute_check()?;
        } else {
            git.exec(&["remote", "set-url", "origin", url.as_str()])?;
            git.exec(&["fetch", "origin", "--prune"])?;
        }

        if let Some(user) = &self.remote.cfg.user {
            git.exec(&["config", "user.name", user.as_str()])?;
        }
        if let Some(email) = &self.remote.cfg.email {
            git.exec(&["config", "user.email", email.as_str()])?;
        }

        let lines = git.lines(&["status", "-s"])?;
        if !lines.is_empty() {
            if let Some(msg) = self.message.as_ref() {
                git.exec(&["commit", "-m", msg.as_str()])?;
            } else {
                bail!("have uncommitted change(s), skip synchronization");
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
            git.checkout(&default_branch)?;
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
                    if !self.ops.contains("push") {
                        continue;
                    }
                    git.checkout(&branch.name)?;
                    git.exec(&["push"])?;
                }
                BranchStatus::Behind => {
                    if !self.ops.contains("pull") {
                        continue;
                    }
                    git.checkout(&branch.name)?;
                    git.exec(&["pull"])?;
                }
                BranchStatus::Gone => {
                    if !self.ops.contains("delete") {
                        continue;
                    }
                    if branch.name == default_branch {
                        continue;
                    }
                    git.checkout(&default_branch)?;
                    git.exec(&["branch", "-D", &branch.name])?;
                }
                BranchStatus::Conflict => {
                    if !self.ops.contains("force") {
                        continue;
                    }
                    git.checkout(&branch.name)?;
                    git.exec(&["push", "-f"])?;
                }
                BranchStatus::Detached => {
                    if !self.ops.contains("add") {
                        continue;
                    }
                    git.checkout(&branch.name)?;
                    git.exec(&["push", "--set-upstream", "origin", &branch.name])?;
                }
                BranchStatus::Sync => {}
            }
        }

        let target = match head.as_ref() {
            Some(commit) => commit,
            None => &backup_branch,
        };
        git.checkout(target)?;

        Ok(())
    }
}

impl Task<Option<String>> for SyncTask {
    fn run(&self) -> Result<Option<String>> {
        match self.dry_run()? {
            Some(msg) => Ok(Some(format!(
                "{}:{}/{} => {}",
                self.remote.name.as_str(),
                self.owner,
                self.name,
                msg
            ))),
            None => Ok(None),
        }
    }
}

impl SyncTask {
    fn dry_run(&self) -> Result<Option<String>> {
        if let None = self.remote.cfg.clone.as_ref() {
            return Ok(None);
        }

        match fs::read_dir(&self.path) {
            Ok(_) => {}
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                return Ok(Some(String::from("clone")))
            }
            Err(err) => {
                return Err(err).with_context(|| format!("Read dir {}", self.path.display()))
            }
        }

        let path = format!("{}", self.path.display());
        let git = GitCmd::with_path(path.as_str());

        let url = Repo::get_clone_url(self.owner.as_str(), self.name.as_str(), &self.remote);
        git.exec(&["remote", "set-url", "origin", url.as_str()])?;
        git.exec(&["fetch", "origin", "--prune"])?;

        let mut actions: Vec<String> = Vec::new();

        let lines = git.lines(&["status", "-s"])?;
        if !lines.is_empty() {
            if let Some(_) = self.message.as_ref() {
                actions.push(String::from("commit change(s)"));
            } else {
                return Ok(Some(String::from("skip due to uncommitted change(s)")));
            }
        }

        let current = git.read(&["branch", "--show-current"])?;
        let head_detached = current.is_empty();

        let lines = git.lines(&["branch", "-vv"])?;
        for line in lines {
            if line.starts_with("*") && head_detached {
                continue;
            }
            let branch = GitBranch::parse(&self.branch_re, line.as_str())?;
            match branch.status {
                BranchStatus::Ahead => {
                    if !self.ops.contains("push") {
                        continue;
                    }
                    actions.push(format!("push {}", branch.name));
                }
                BranchStatus::Behind => {
                    if !self.ops.contains("pull") {
                        continue;
                    }
                    actions.push(format!("pull {}", branch.name));
                }
                BranchStatus::Gone => {
                    if !self.ops.contains("delete") {
                        continue;
                    }
                    actions.push(format!("delete {}", branch.name));
                }
                BranchStatus::Conflict => {
                    if !self.ops.contains("force") {
                        continue;
                    }
                    actions.push(format!("force-push {}", branch.name));
                }
                BranchStatus::Detached => {
                    if !self.ops.contains("add") {
                        continue;
                    }
                    actions.push(format!("push set-upstream {}", branch.name));
                }
                BranchStatus::Sync => {}
            }
        }

        if actions.is_empty() {
            return Ok(None);
        }

        Ok(Some(actions.join(", ")))
    }
}