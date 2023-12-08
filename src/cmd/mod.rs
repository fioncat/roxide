mod attach;
mod branch;
mod check;
mod complete;
mod config;
mod copy;
mod detach;
mod gc;
mod get;
mod home;
mod import;
mod info;
mod init;
mod label;
mod merge;
mod open;
mod rebase;
mod recover;
mod remove;
mod reset;
mod run;
mod snapshot;
mod squash;
mod sync;
mod tag;
mod update;
mod version;

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
    Check(check::CheckArgs),
    Complete(complete::CompleteArgs),
    Config(config::ConfigArgs),
    Copy(copy::CopyArgs),
    Detach(detach::DetachArgs),
    Gc(gc::GcArgs),
    Get(get::GetArgs),
    Home(home::HomeArgs),
    Import(import::ImportArgs),
    Info(info::InfoArgs),
    Init(init::InitArgs),
    Merge(merge::MergeArgs),
    Open(open::OpenArgs),
    Rebase(rebase::RebaseArgs),
    Recover(recover::RecoverArgs),
    Remove(remove::RemoveArgs),
    Reset(reset::ResetArgs),
    Label(label::LabelArgs),
    Run(run::RunArgs),
    Snapshot(snapshot::SnapshotArgs),
    Squash(squash::SquashArgs),
    Sync(sync::SyncArgs),
    Tag(tag::TagArgs),
    Update(update::UpdateArgs),
    Version(version::VersionArgs),
}

impl Commands {
    pub fn get_completions() -> HashMap<&'static str, Completion> {
        hashmap![
            "attach" => attach::AttachArgs::completion(),
            "branch" => branch::BranchArgs::completion(),
            "copy" => copy::CopyArgs::completion(),
            "get" => get::GetArgs::completion(),
            "home" => home::HomeArgs::completion(),
            "import" => import::ImportArgs::completion(),
            "init" => init::InitArgs::completion(),
            "merge" => merge::MergeArgs::completion(),
            "rebase" => rebase::RebaseArgs::completion(),
            "remove" => remove::RemoveArgs::completion(),
            "reset" => reset::ResetArgs::completion(),
            "run" => run::RunArgs::completion(),
            "label" => label::LabelArgs::completion(),
            "snapshot" => snapshot::SnapshotArgs::completion(),
            "squash" => squash::SquashArgs::completion(),
            "sync" => sync::SyncArgs::completion(),
            "tag" => tag::TagArgs::completion()
        ]
    }
}

impl Run for App {
    fn run(&self, cfg: &Config) -> Result<()> {
        match &self.command {
            Commands::Attach(args) => args.run(cfg),
            Commands::Branch(args) => args.run(cfg),
            Commands::Check(args) => args.run(cfg),
            Commands::Complete(args) => args.run(cfg),
            Commands::Label(args) => args.run(cfg),
            Commands::Config(args) => args.run(cfg),
            Commands::Copy(args) => args.run(cfg),
            Commands::Detach(args) => args.run(cfg),
            Commands::Gc(args) => args.run(cfg),
            Commands::Get(args) => args.run(cfg),
            Commands::Home(args) => args.run(cfg),
            Commands::Import(args) => args.run(cfg),
            Commands::Info(args) => args.run(cfg),
            Commands::Init(args) => args.run(cfg),
            Commands::Merge(args) => args.run(cfg),
            Commands::Open(args) => args.run(cfg),
            Commands::Rebase(args) => args.run(cfg),
            Commands::Recover(args) => args.run(cfg),
            Commands::Remove(args) => args.run(cfg),
            Commands::Reset(args) => args.run(cfg),
            Commands::Run(args) => args.run(cfg),
            Commands::Snapshot(args) => args.run(cfg),
            Commands::Squash(args) => args.run(cfg),
            Commands::Sync(args) => args.run(cfg),
            Commands::Tag(args) => args.run(cfg),
            Commands::Update(args) => args.run(cfg),
            Commands::Version(args) => args.run(cfg),
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
        if to_complete.ends_with(",") {
            for item in candidates {
                let item = format!("{to_complete}{item}");
                items.push(item);
            }
        } else {
            let last_to_complete = to_complete.split(",").last();
            match last_to_complete {
                Some(last) => {
                    for item in candidates {
                        if let Some(suffix) = item.strip_prefix(last) {
                            let item = format!("{to_complete}{suffix}");
                            items.push(item);
                        }
                    }
                }
                None => items = candidates.into_iter().collect(),
            }
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
