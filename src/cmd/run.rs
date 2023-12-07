use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use clap::Args;

use crate::batch::{self, Task};
use crate::cmd::{Completion, CompletionResult, Run};
use crate::config::Config;
use crate::repo::database::{Database, SelectOptions, Selector};
use crate::term::Workflow;
use crate::{confirm, stderr, term, utils};

/// Run workflow
#[derive(Args)]
pub struct RunArgs {
    pub head: Option<String>,
    pub query: Option<String>,

    /// Use editor to filter items.
    #[clap(short)]
    pub edit: bool,

    /// If true, run workflow for current repo.
    #[clap(short)]
    pub current: bool,

    /// The workflow name to execute.
    #[clap(short, default_value = "default")]
    pub name: String,

    /// Use the labels to filter repo.
    #[clap(short)]
    pub labels: Option<String>,
}

impl Run for RunArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
        let workflow = Workflow::new(cfg, self.name.as_str())?;
        let db = Database::load(cfg)?;

        if self.current {
            let repo = db.must_get_current()?;
            confirm!(
                "Run workflow '{}' for repo '{}'",
                self.name,
                repo.name_with_remote()
            );
            return workflow.execute_repo(&repo);
        }

        let filter_labels = utils::parse_labels(&self.labels);
        let opts = SelectOptions::default()
            .with_filter_labels(filter_labels)
            .with_many_edit(self.edit);
        let selector = Selector::from_args(&db, &self.head, &self.query, opts);

        let (repos, level) = selector.many_local()?;
        if repos.is_empty() {
            stderr!("No repo to run");
            return Ok(());
        }

        let items: Vec<_> = repos.iter().map(|repo| repo.to_string(&level)).collect();
        term::must_confirm_items(&items, "run workflow", "workflow", "Repo", "Repos")?;

        let level = Arc::new(level);
        let mut tasks = Vec::with_capacity(repos.len());
        let workflow = Arc::new(workflow);
        for repo in repos {
            let dir = repo.get_path(cfg);
            let show_name = repo.to_string(&level);
            tasks.push((
                show_name,
                RunTask {
                    workflow: Arc::clone(&workflow),
                    dir,
                    remote: repo.remote.name.to_string(),
                    owner: repo.owner.name.to_string(),
                    name: repo.name.clone(),
                },
            ))
        }

        batch::must_run("Run", tasks)?;
        Ok(())
    }
}

impl RunArgs {
    pub fn completion() -> Completion {
        Completion {
            args: Completion::repo_args,
            flags: Some(|cfg, flag, _to_complete| match flag {
                'n' => {
                    let mut names: Vec<String> = cfg
                        .workflows
                        .iter()
                        .map(|(key, _)| key.to_string())
                        .collect();
                    names.sort();
                    Ok(Some(CompletionResult::from(names)))
                }
                _ => Ok(None),
            }),
        }
    }
}

struct RunTask {
    workflow: Arc<Workflow>,
    dir: PathBuf,

    remote: String,
    owner: String,
    name: String,
}

impl Task<()> for RunTask {
    fn run(&self) -> Result<()> {
        self.workflow
            .execute_task(&self.dir, &self.remote, &self.owner, &self.name)
    }
}
