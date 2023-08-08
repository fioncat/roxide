use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use anyhow::{bail, Result};
use clap::Args;
use console::style;

use crate::batch::{self, Reporter, Task};
use crate::cmd::Run;
use crate::repo::database::Database;
use crate::repo::types::{NameLevel, Repo};
use crate::shell::Workflow;
use crate::{config, utils};

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
    name_level: Arc<NameLevel>,
    dir: PathBuf,

    remote: String,
    owner: String,
    name: String,

    show_name: String,
}

impl Task<()> for RunTask {
    fn run(&self, rp: &Reporter<()>) -> Result<()> {
        self.workflow.execute_task(
            &self.name_level,
            rp,
            &self.dir,
            &self.remote,
            &self.owner,
            &self.name,
        )
    }

    fn message_done(&self, result: &Result<()>) -> String {
        match result {
            Ok(_) => format!("Run workflow for {} done", self.show_name),
            Err(_) => {
                let msg = format!("Run workflow for {} failed", self.show_name);
                format!("{}", style(msg).red())
            }
        }
    }
}

impl Run for RunArgs {
    fn run(&self) -> Result<()> {
        let workflow = Workflow::new(self.workflow.as_str())?;
        let workflow = Arc::new(workflow);
        let (mut repos, level) = self.select_repos()?;
        if repos.is_empty() {
            println!("No repo to run");
            return Ok(());
        }
        if repos.len() == 1 {
            let repo = repos.into_iter().next().unwrap();
            workflow.execute_repo(&repo)?;
            return Ok(());
        }
        if self.filter {
            let items: Vec<String> = repos.iter().map(|repo| repo.as_string(&level)).collect();
            let items = utils::edit_items(items)?;
            let filter_set: HashSet<String> = items.into_iter().collect();
            repos = repos
                .into_iter()
                .filter(|repo| filter_set.contains(&repo.as_string(&level)))
                .collect();
            if repos.is_empty() {
                println!("No repo to run after filter");
                return Ok(());
            }
        }

        let level = Arc::new(level);
        let mut tasks = Vec::with_capacity(repos.len());
        for repo in repos {
            let dir = repo.get_path();
            let show_name = repo.as_string(&level);
            tasks.push(RunTask {
                workflow: workflow.clone(),
                name_level: level.clone(),
                dir,
                remote: format!("{}", repo.remote),
                owner: format!("{}", repo.owner),
                name: format!("{}", repo.name),
                show_name,
            })
        }

        let desc = format!("Run workflow {}", self.workflow);
        let _ = batch::run(desc.as_str(), tasks);
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
        if let None = &self.remote {
            return Ok((db.list_all(), NameLevel::Full));
        }
        let remote = self.remote.as_ref().unwrap().as_str();
        if let None = &self.query {
            return Ok((db.list_by_remote(remote), NameLevel::Owner));
        }
        let query = self.query.as_ref().unwrap().as_str();
        let query = query.trim_matches('/');
        if query.is_empty() {
            bail!("Invalid query {}", self.query.as_ref().unwrap());
        }
        if !query.contains("/") {
            return Ok((db.list_by_owner(remote, query), NameLevel::Name));
        }
        let remote = config::must_get_remote(remote)?;
        let (owner, name) = utils::parse_query(&remote, query);
        let repo = db.must_get(&remote.name, &owner, &name)?;
        Ok((vec![repo], NameLevel::Name))
    }
}
