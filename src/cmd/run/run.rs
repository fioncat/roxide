use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use anyhow::Result;
use clap::Args;

use crate::batch::{self, Task};
use crate::cmd::Run;
use crate::repo::database::Database;
use crate::repo::query::Query;
use crate::repo::types::{NameLevel, Repo};
use crate::term::{self, Workflow};
use crate::{confirm, info};

/// Run workflow
#[derive(Args)]
pub struct RunArgs {
    /// The workflow name.
    pub workflow: String,

    /// The remote name.
    pub remote: Option<String>,

    /// The repo query.
    pub query: Option<String>,

    /// If true, filter repos.
    #[clap(long, short)]
    pub filter: bool,

    /// If true, run workflow for current repo.
    #[clap(long, short)]
    pub current: bool,
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

impl Run for RunArgs {
    fn run(&self) -> Result<()> {
        let workflow = Workflow::new(self.workflow.as_str())?;
        let workflow = Arc::new(workflow);
        let (repos, level) = self.select_repos()?;
        if repos.is_empty() {
            println!("No repo to run");
            return Ok(());
        }
        if repos.len() == 1 {
            let repo = repos.into_iter().next().unwrap();
            confirm!(
                "Run workflow {} for repo {}",
                self.workflow,
                repo.full_name()
            );
            workflow.execute_repo(&repo)?;
            return Ok(());
        }

        if repos.is_empty() {
            info!("Nothing to run");
            return Ok(());
        }
        let items: Vec<_> = repos.iter().map(|repo| repo.as_string(&level)).collect();
        term::must_confirm_items(&items, "run workflow", "workflow", "Repo", "Repos")?;

        let level = Arc::new(level);
        let mut tasks = Vec::with_capacity(repos.len());
        for repo in repos {
            let dir = repo.get_path();
            let show_name = repo.as_string(&level);
            tasks.push((
                show_name,
                RunTask {
                    workflow: workflow.clone(),
                    dir,
                    remote: format!("{}", repo.remote),
                    owner: format!("{}", repo.owner),
                    name: format!("{}", repo.name),
                },
            ))
        }

        batch::must_run("Run", tasks)?;
        Ok(())
    }
}

impl RunArgs {
    fn select_repos(&self) -> Result<(Vec<Rc<Repo>>, NameLevel)> {
        let db = Database::read()?;
        if self.current {
            let repo = db.must_current()?;
            return Ok((vec![repo], NameLevel::Name));
        }
        let query = Query::from_args(&db, &self.remote, &self.query);
        let (repos, level) = query.list_local(self.filter)?;

        Ok((repos, level))
    }
}
