use std::collections::HashMap;

use anyhow::Result;
use clap::Args;

use crate::batch::Task;
use crate::cmd::{Completion, CompletionResult, Run};
use crate::config::{Config, WorkflowConfig};
use crate::repo::database::Database;
use crate::repo::Repo;
use crate::workflow::Workflow;

/// Run workflow for current repository (.roxide/workflows)
#[derive(Args)]
pub struct MakeArgs {
    /// The workflow name, defines in '.roxide/workflows/*.toml'
    #[clap(default_value = "all")]
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
    fn load_workflow_cfg(cfg: &Config, repo: &Repo) -> Result<HashMap<String, WorkflowConfig>> {
        let path = repo.get_path(cfg).join(".roxide").join("workflows");
        Config::load_workflows(&path)
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
