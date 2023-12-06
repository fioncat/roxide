#![allow(dead_code)]

mod home;

use anyhow::Result;
use clap::{Parser, Subcommand};
use strum::EnumVariantNames;

use crate::config::Config;
use crate::repo::database::{self, Database};

pub trait Run {
    fn run(&self) -> Result<()>;
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

pub trait Completion {
    fn args(&self, args: &[&str]) -> Result<CompletionResult>;

    fn flag(&self, flag: &str) -> Result<CompletionResult>;
}

pub struct BaseCompletion {
    cfg: Config,
}

impl BaseCompletion {
    pub fn load() -> Result<BaseCompletion> {
        let cfg = Config::load()?;
        Ok(BaseCompletion { cfg })
    }

    pub fn repo_args(&self, args: &[&str]) -> Result<CompletionResult> {
        match args.len() {
            0 | 1 => {
                let remotes = self.cfg.list_remote_names();
                Ok(CompletionResult::from(remotes))
            }
            2 => {
                let db = Database::load(&self.cfg)?;

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

    pub fn owner_args(&self, args: &[&str]) -> Result<CompletionResult> {
        match args.len() {
            0 | 1 => {
                let remotes = self.cfg.list_remote_names();
                let items: Vec<_> = remotes
                    .into_iter()
                    .map(|remote| remote.to_string())
                    .collect();
                Ok(CompletionResult::from(items))
            }
            2 => {
                let remote_name = &args[0];
                let db = Database::load(&self.cfg)?;
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

    pub fn remote_args(&self, args: &[&str]) -> Result<CompletionResult> {
        match args.len() {
            0 | 1 => {
                let remotes = self.cfg.list_remote_names();
                let items: Vec<_> = remotes
                    .into_iter()
                    .map(|remote| remote.to_string())
                    .collect();
                Ok(CompletionResult::from(items))
            }
            _ => Ok(CompletionResult::empty()),
        }
    }
}

#[derive(Parser)]
#[command(author, version = env!("ROXIDE_VERSION"), about)]
pub struct App {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, EnumVariantNames)]
#[strum(serialize_all = "kebab-case")]
pub enum Commands {
    Home(home::HomeArgs),
}

impl Run for App {
    fn run(&self) -> Result<()> {
        match &self.command {
            Commands::Home(args) => args.run(),
        }
    }
}
