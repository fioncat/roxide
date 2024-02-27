use std::sync::Arc;

use anyhow::Result;
use clap::Args;

use crate::batch::{self, Task};
use crate::cmd::{Completion, CompletionResult, Run};
use crate::config::Config;
use crate::repo::database::{Database, SelectOptions, Selector};
use crate::workflow::Workflow;
use crate::{term, utils};

/// Run workflow in repository.
#[derive(Args)]
pub struct RunArgs {
    /// Repository selection head.
    pub head: Option<String>,

    /// Repository selection query.
    pub query: Option<String>,

    /// Use editor to filter items before running.
    #[clap(short, long)]
    pub edit: bool,

    /// Run workflow for current repository.
    #[clap(short, long)]
    pub current: bool,

    /// The workflow name to execute.
    #[clap(short, default_value = "default")]
    pub name: String,

    /// Use the labels to filter repository.
    #[clap(short, long)]
    pub labels: Option<String>,
}

impl Run for RunArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
        let db = Database::load(cfg)?;

        if self.current {
            let repo = db.must_get_current()?;
            let workflow = Workflow::load(&self.name, cfg, &repo)?;
            return workflow.run();
        }

        let filter_labels = utils::parse_labels(&self.labels);
        let opts = SelectOptions::default()
            .with_filter_labels(filter_labels)
            .with_many_edit(self.edit);
        let selector = Selector::from_args(&self.head, &self.query, opts);

        let (repos, level) = selector.many_local(&db)?;
        if repos.is_empty() {
            eprintln!("No repo to run");
            return Ok(());
        }

        let items: Vec<_> = repos.iter().map(|repo| repo.to_string(&level)).collect();
        term::must_confirm_items(&items, "run workflow", "workflow", "Repo", "Repos")?;

        let level = Arc::new(level);
        let mut tasks = Vec::with_capacity(repos.len());
        let workflow_cfg = Arc::new(cfg.get_workflow(&self.name)?.into_owned());

        for repo in repos {
            let show_name = repo.to_string(&level);
            let workflow = Workflow::load_for_batch(cfg, &repo, Arc::clone(&workflow_cfg));
            tasks.push((show_name, workflow))
        }

        batch::must_run("Run", tasks)?;
        Ok(())
    }
}

impl RunArgs {
    pub fn completion() -> Completion {
        Completion {
            args: Completion::repo_args,
            flags: Some(|cfg, flag, to_complete| match flag {
                'n' => {
                    let mut names: Vec<String> =
                        cfg.workflows.keys().map(|key| key.to_string()).collect();
                    names.sort();
                    Ok(Some(CompletionResult::from(names)))
                }
                'l' => Completion::labels_flag(cfg, to_complete),
                _ => Ok(None),
            }),
        }
    }
}
