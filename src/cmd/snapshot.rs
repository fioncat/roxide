use std::collections::HashSet;

use anyhow::Result;
use clap::Args;
use console::style;

use crate::cmd::{Completion, CompletionResult, Run};
use crate::config::Config;
use crate::repo::database::{self, Database};
use crate::repo::snapshot::Snapshot;
use crate::{confirm, info};

/// Snapshot operations for workspace
#[derive(Args)]
pub struct SnapshotArgs {
    /// The snapshot name.
    pub name: Option<String>,

    /// Recover database with snapshot.
    #[clap(short, long)]
    pub restore: bool,

    /// Use database to create a snapshot.
    #[clap(short, long)]
    pub create: bool,

    /// Use the labels to filter repo.
    #[clap(short, long)]
    pub labels: Option<String>,

    /// Display snapshot with json format.
    #[clap(short = 'J')]
    pub json: bool,

    /// Save snapshot with pretty json.
    #[clap(short, long)]
    pub pretty: bool,
}

impl Run for SnapshotArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
        if self.name.is_none() {
            let names = Snapshot::list(cfg)?;
            for name in names {
                println!("{name}");
            }
            return Ok(());
        }

        let name = self.name.clone().unwrap();
        if self.restore {
            return self.restore(cfg, name);
        }
        if self.create {
            return self.create(cfg, name);
        }

        let snapshot = Snapshot::load(cfg, name)?;
        snapshot.display(self.json)
    }
}

impl SnapshotArgs {
    fn restore(&self, cfg: &Config, name: String) -> Result<()> {
        let snapshot = Snapshot::load(cfg, name)?;
        snapshot.display(self.json)?;
        confirm!("Continue to restore");

        database::backup_replace(cfg, "snapshot")?;

        info!("Restore database with snapshot {}", snapshot.name);
        let db = Database::load(cfg)?;
        snapshot.restore(db)?;

        println!();
        println!("Restore done, you should use the {} and {} commands to take the effects to the workspace.", style("sync").cyan().bold(), style("gc").cyan().bold());

        Ok(())
    }

    fn create(&self, cfg: &Config, name: String) -> Result<()> {
        let set: HashSet<_> = Snapshot::list(cfg)?.into_iter().collect();
        if set.contains(&name) {
            confirm!("Replace exists snapshot '{}'", name);
        }

        let db = Database::load(cfg)?;
        let snapshot = Snapshot::take(cfg, db, name);
        snapshot.save(self.pretty)?;

        snapshot.display(self.json)
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
