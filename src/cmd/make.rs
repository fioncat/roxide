use std::collections::HashMap;
use std::fs::File;
use std::io;

use anyhow::{bail, Context, Result};
use clap::Args;

use crate::batch::Task;
use crate::cmd::{Completion, CompletionResult, Run};
use crate::config::{Config, WorkflowConfig};
use crate::repo::database::Database;
use crate::repo::Repo;
use crate::workflow::Workflow;

/// Run make workflow for current repository (.roxmake.yml)
#[derive(Args)]
pub struct MakeArgs {
    /// The workflow name, define in '.roxmake.yml'
    #[clap(default_value = "default")]
    name: String,
}

impl Run for MakeArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
        let db = Database::load(cfg)?;

        let repo = db.must_get_current()?;
        let workflows = Self::load_workflow_cfg(cfg, &repo)?;
        let workflow_cfg = Config::get_workflow_from_map(&workflows, &self.name)?;
        let workflow = Workflow::new(cfg, &repo, workflow_cfg, true);
        workflow.run()
    }
}

impl MakeArgs {
    const WORKFLOW_FILE_NAME: &'static str = ".roxmake.yml";

    fn load_workflow_cfg(cfg: &Config, repo: &Repo) -> Result<HashMap<String, WorkflowConfig>> {
        let path = repo.get_path(cfg).join(Self::WORKFLOW_FILE_NAME);

        match File::open(&path) {
            Ok(file) => {
                serde_yaml::from_reader(file).context("invalid yaml in your roxide make file")
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => bail!(
                "could not find file '{}' in your repo, please create it first",
                Self::WORKFLOW_FILE_NAME
            ),
            Err(err) => {
                Err(err).with_context(|| format!("read file '{}'", Self::WORKFLOW_FILE_NAME))
            }
        }
    }

    pub fn completion() -> Completion {
        Completion {
            args: |cfg, args| -> Result<CompletionResult> {
                match args.len() {
                    0 | 1 => {
                        let db = Database::load(cfg)?;
                        let repo = db.must_get_current()?;
                        let workflows = Self::load_workflow_cfg(cfg, &repo)?;
                        let mut items: Vec<String> = workflows.into_keys().collect();
                        items.sort_unstable();
                        Ok(CompletionResult::from(items))
                    }
                    _ => Ok(CompletionResult::empty()),
                }
            },
            flags: None,
        }
    }
}
