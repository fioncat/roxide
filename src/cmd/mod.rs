#![allow(dead_code)]

mod attach;
mod branch;
mod complete;
mod detach;
mod get;
mod home;
mod init;
mod merge;
mod open;
mod rebase;
mod remove;
mod reset;
mod run;
mod squash;
mod sync;

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use clap::{Parser, Subcommand};
use strum::EnumVariantNames;

use crate::config::Config;
use crate::repo::database::{self, Database};
use crate::term::{GitBranch, GitRemote};
use crate::{api, hashmap, term};

#[derive(Parser)]
#[command(author, version = env!("ROXIDE_VERSION"), about)]
pub struct App {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, EnumVariantNames)]
#[strum(serialize_all = "kebab-case")]
pub enum Commands {
    Attach(attach::AttachArgs),
    Branch(branch::BranchArgs),
    Complete(complete::CompleteArgs),
    Detach(detach::DetachArgs),
    Get(get::GetArgs),
    Home(home::HomeArgs),
    Init(init::InitArgs),
    Merge(merge::MergeArgs),
    Open(open::OpenArgs),
    Rebase(rebase::RebaseArgs),
    Remove(remove::RemoveArgs),
    Reset(reset::ResetArgs),
    Run(run::RunArgs),
    Squash(squash::SquashArgs),
    Sync(sync::SyncArgs),
}

impl Commands {
    pub fn get_completions() -> HashMap<&'static str, Completion> {
        hashmap![
            "attach" => attach::AttachArgs::completion(),
            "branch" => branch::BranchArgs::completion(),
            "get" => get::GetArgs::completion(),
            "home" => home::HomeArgs::completion(),
            "init" => init::InitArgs::completion(),
            "merge" => merge::MergeArgs::completion(),
            "rebase" => rebase::RebaseArgs::completion(),
            "remove" => remove::RemoveArgs::completion(),
            "reset" => reset::ResetArgs::completion(),
            "run" => run::RunArgs::completion(),
            "squash" => squash::SquashArgs::completion(),
            "sync" => sync::SyncArgs::completion()
        ]
    }
}

impl Run for App {
    fn run(&self, cfg: &Config) -> Result<()> {
        match &self.command {
            Commands::Attach(args) => args.run(cfg),
            Commands::Branch(args) => args.run(cfg),
            Commands::Complete(args) => args.run(cfg),
            Commands::Detach(args) => args.run(cfg),
            Commands::Get(args) => args.run(cfg),
            Commands::Home(args) => args.run(cfg),
            Commands::Init(args) => args.run(cfg),
            Commands::Merge(args) => args.run(cfg),
            Commands::Open(args) => args.run(cfg),
            Commands::Rebase(args) => args.run(cfg),
            Commands::Remove(args) => args.run(cfg),
            Commands::Reset(args) => args.run(cfg),
            Commands::Run(args) => args.run(cfg),
            Commands::Squash(args) => args.run(cfg),
            Commands::Sync(args) => args.run(cfg),
        }
    }
}

pub trait Run {
    fn run(&self, cfg: &Config) -> Result<()>;
}

pub struct CompletionResult {
    pub items: Vec<String>,

    pub no_space: bool,
}

impl From<Vec<String>> for CompletionResult {
    fn from(items: Vec<String>) -> Self {
        CompletionResult {
            items,
            no_space: false,
        }
    }
}

impl CompletionResult {
    pub fn empty() -> CompletionResult {
        CompletionResult {
            items: vec![],
            no_space: false,
        }
    }

    pub fn no_space(mut self) -> Self {
        self.no_space = true;
        self
    }

    pub fn show(&self) {
        if self.no_space {
            println!("1");
        } else {
            println!("0");
        }
        for item in self.items.iter() {
            println!("{}", item);
        }
    }
}

pub struct Completion {
    pub args: fn(&Config, &[&str]) -> Result<CompletionResult>,

    pub flags: Option<fn(&Config, &char, &str) -> Result<Option<CompletionResult>>>,
}

impl Completion {
    pub fn repo_args(cfg: &Config, args: &[&str]) -> Result<CompletionResult> {
        match args.len() {
            0 | 1 => {
                let remotes = cfg.list_remote_names();
                Ok(CompletionResult::from(remotes))
            }
            2 => {
                let db = Database::load(cfg)?;

                let remote_name = &args[0];
                let query = &args[1];

                if !query.contains("/") {
                    let owners = db.list_owners(remote_name);
                    let items: Vec<_> = owners
                        .into_iter()
                        .map(|owner| format!("{}/", owner))
                        .collect();
                    return Ok(CompletionResult::from(items).no_space());
                }

                let (owner, _) = database::parse_owner(query);
                let repos = db.list_by_remote(remote_name, &None);
                let items: Vec<_> = repos
                    .into_iter()
                    .filter(|repo| repo.owner.name.as_str() == owner.as_str())
                    .map(|repo| format!("{}", repo.name_with_owner()))
                    .collect();
                Ok(CompletionResult::from(items))
            }
            _ => Ok(CompletionResult::empty()),
        }
    }

    pub fn owner_args(cfg: &Config, args: &[&str]) -> Result<CompletionResult> {
        match args.len() {
            0 | 1 => {
                let remotes = cfg.list_remote_names();
                let items: Vec<_> = remotes
                    .into_iter()
                    .map(|remote| remote.to_string())
                    .collect();
                Ok(CompletionResult::from(items))
            }
            2 => {
                let remote_name = &args[0];
                let db = Database::load(&cfg)?;
                let owners = db.list_owners(remote_name);
                let items: Vec<_> = owners
                    .into_iter()
                    .map(|owner| format!("{}/", owner))
                    .collect();
                Ok(CompletionResult::from(items).no_space())
            }
            _ => Ok(CompletionResult::empty()),
        }
    }

    pub fn remote_args(cfg: &Config, args: &[&str]) -> Result<CompletionResult> {
        match args.len() {
            0 | 1 => {
                let remotes = cfg.list_remote_names();
                let items: Vec<_> = remotes
                    .into_iter()
                    .map(|remote| remote.to_string())
                    .collect();
                Ok(CompletionResult::from(items))
            }
            _ => Ok(CompletionResult::empty()),
        }
    }

    pub fn labels(
        cfg: &Config,
        flag: &char,
        to_complete: &str,
    ) -> Result<Option<CompletionResult>> {
        match flag {
            'l' => Self::labels_flag(cfg, to_complete),
            _ => Ok(None),
        }
    }

    pub fn labels_flag(cfg: &Config, to_complete: &str) -> Result<Option<CompletionResult>> {
        let mut exists_labels = HashSet::new();
        for (_, remote_cfg) in cfg.remotes.iter() {
            if let Some(remote_labels) = remote_cfg.labels.as_ref() {
                for label in remote_labels.iter() {
                    exists_labels.insert(label.clone());
                }
            }

            for (_, owner_cfg) in remote_cfg.owners.iter() {
                if let Some(owner_labels) = owner_cfg.labels.as_ref() {
                    for label in owner_labels.iter() {
                        exists_labels.insert(label.clone());
                    }
                }
            }
        }

        let db = Database::load(cfg)?;
        let repos = db.list_all(&None);
        for repo in repos {
            if let Some(labels) = repo.labels.as_ref() {
                for label in labels.iter() {
                    exists_labels.insert(label.to_string());
                }
            }
        }

        Self::multiple_values_flag(to_complete, exists_labels)
    }

    pub fn multiple_values_flag(
        to_complete: &str,
        mut candidates: HashSet<String>,
    ) -> Result<Option<CompletionResult>> {
        let current_items: HashSet<&str> = to_complete.split(',').collect();
        for current_label in current_items {
            candidates.remove(current_label);
        }

        let mut items = Vec::with_capacity(candidates.len());
        for item in candidates {
            let item = if to_complete != "" {
                if to_complete.ends_with(",") {
                    format!("{to_complete}{item}")
                } else {
                    format!("{to_complete},{item}")
                }
            } else {
                item
            };
            items.push(item);
        }

        Ok(Some(CompletionResult::from(items).no_space()))
    }

    pub fn branch_args(_cfg: &Config, args: &[&str]) -> Result<CompletionResult> {
        match args.len() {
            0 | 1 => {
                let branches = GitBranch::list()?;
                let items: Vec<_> = branches
                    .into_iter()
                    .filter(|branch| !branch.current)
                    .map(|branch| branch.name)
                    .collect();
                Ok(CompletionResult::from(items))
            }
            _ => Ok(CompletionResult::empty()),
        }
    }
}

pub fn get_git_remote(cfg: &Config, upstream: bool, force: bool) -> Result<GitRemote> {
    term::ensure_no_uncommitted()?;
    if upstream {
        let db = Database::load(cfg)?;
        let repo = db.must_get_current()?;
        let provider = api::build_provider(cfg, &repo.remote, force)?;

        GitRemote::from_upstream(cfg, &repo, &provider)
    } else {
        Ok(GitRemote::new())
    }
}
