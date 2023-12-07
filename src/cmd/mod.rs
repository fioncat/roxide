#![allow(dead_code)]

mod complete;
mod get;
mod home;
mod init;

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use clap::{Parser, Subcommand};
use strum::EnumVariantNames;

use crate::config::Config;
use crate::hashmap;
use crate::repo::database::{self, Database};

#[derive(Parser)]
#[command(author, version = env!("ROXIDE_VERSION"), about)]
pub struct App {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, EnumVariantNames)]
#[strum(serialize_all = "kebab-case")]
pub enum Commands {
    Complete(complete::CompleteArgs),
    Home(home::HomeArgs),
    Get(get::GetArgs),
    Init(init::InitArgs),
}

impl Commands {
    pub fn get_completions() -> HashMap<&'static str, Completion> {
        hashmap![
            "home" => home::HomeArgs::completion(),
            "get" => get::GetArgs::completion(),
            "init" => init::InitArgs::completion()
        ]
    }
}

impl Run for App {
    fn run(&self, cfg: &Config) -> Result<()> {
        match &self.command {
            Commands::Complete(args) => args.run(cfg),
            Commands::Home(args) => args.run(cfg),
            Commands::Get(args) => args.run(cfg),
            Commands::Init(args) => args.run(cfg),
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

        let current_labels: HashSet<&str> = to_complete.split(',').collect();
        for current_label in current_labels {
            exists_labels.remove(current_label);
        }

        let mut items = Vec::with_capacity(exists_labels.len());
        for label in exists_labels {
            let item = if to_complete != "" {
                if to_complete.ends_with(",") {
                    format!("{to_complete}{label}")
                } else {
                    format!("{to_complete},{label}")
                }
            } else {
                label
            };
            items.push(item);
        }

        Ok(Some(CompletionResult::from(items).no_space()))
    }
}
