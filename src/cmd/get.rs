use std::rc::Rc;

use anyhow::Result;
use clap::Args;
use serde::Serialize;

use crate::cmd::{Completion, Run};
use crate::config::Config;
use crate::repo::database::{Database, SelectOptions, Selector};
use crate::repo::{NameLevel, Repo};
use crate::term::Table;
use crate::{term, utils};

/// Show repository info.
#[derive(Args)]
pub struct GetArgs {
    /// Repository selection head.
    pub head: Option<String>,

    /// Repository selection query.
    pub query: Option<String>,

    /// Show size in list info. If your workspace is large, this can cause
    /// command to take too long to execute.
    #[clap(short)]
    pub size: bool,

    /// Show current repo info.
    #[clap(short)]
    pub current: bool,

    /// Show result as json format.
    #[clap(short = 'J')]
    pub json: bool,

    /// Use the labels to filter repo.
    #[clap(short)]
    pub labels: Option<String>,
}

#[derive(Debug, Serialize)]
struct RepoInfo {
    remote: String,
    owner: String,
    name: String,

    accessed: u64,
    last_accessed: String,
    score: String,

    path: String,
    workspace: bool,

    size: String,

    labels: Vec<String>,
}

impl RepoInfo {
    fn from_repo(cfg: &Config, repo: Rc<Repo>) -> Result<Self> {
        let workspace = match repo.path {
            Some(_) => false,
            None => true,
        };
        let path = repo.get_path(cfg);
        let path = format!("{}", path.display());
        let score = format!("{:.2}", repo.score(cfg));
        let size = utils::dir_size(repo.get_path(cfg))?;
        let mut labels: Vec<String> = match repo.labels.as_ref() {
            Some(labels) => labels.iter().map(|label| label.to_string()).collect(),
            None => Vec::new(),
        };
        labels.sort();
        Ok(RepoInfo {
            remote: repo.remote.name.to_string(),
            owner: repo.owner.name.to_string(),
            name: repo.name.to_string(),
            accessed: repo.accessed as u64,
            last_accessed: utils::format_time(repo.last_accessed)?,
            score,
            path,
            workspace,
            size: utils::human_bytes(size),
            labels,
        })
    }
}

impl Run for GetArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
        let db = Database::load(cfg)?;
        let (mut repos, level) = if self.current {
            let repo = db.must_get_current()?;
            (vec![repo], NameLevel::Remote)
        } else {
            let filter_labels = utils::parse_labels(&self.labels);

            let opts = SelectOptions::default().with_filter_labels(filter_labels);
            let selector = Selector::from_args(&db, &self.head, &self.query, opts);

            selector.many_local()?
        };

        if repos.is_empty() {
            if self.json {
                println!("{{}}");
            } else {
                println!("No repo in database");
            }
            return Ok(());
        }

        if self.json {
            let mut infos = Vec::with_capacity(repos.len());
            for repo in repos {
                infos.push(RepoInfo::from_repo(cfg, repo)?);
            }
            return term::show_json(infos);
        }

        let mut table = Table::with_capacity(1 + repos.len());
        let mut titles = vec![
            String::from("NAME"),
            String::from("LABELS"),
            String::from("ACCESS"),
            String::from("LAST_ACCESS"),
            String::from("SCORE"),
        ];

        let mut size_vec: Option<Vec<u64>> = None;
        if self.size {
            titles.push(String::from("SIZE"));
            let mut repos_with_size = Vec::with_capacity(repos.len());
            for repo in repos {
                let path = repo.get_path(cfg);
                let size = utils::dir_size(path)?;
                repos_with_size.push((size, repo));
            }
            repos_with_size.sort_unstable_by(|(size1, _), (size2, _)| size2.cmp(size1));
            size_vec = Some(repos_with_size.iter().map(|(size, _)| *size).collect());
            repos = repos_with_size.into_iter().map(|(_, repo)| repo).collect();
        }
        table.add(titles);

        for (idx, repo) in repos.iter().enumerate() {
            let name = repo.to_string(&level);
            let labels = match repo.labels_string() {
                Some(s) => s,
                None => String::from("<none>"),
            };
            let access = format!("{}", repo.accessed as u64);
            let last_access = utils::format_since(cfg, repo.last_accessed);
            let score = format!("{:.2}", repo.score(cfg));

            let mut row = vec![name, labels, access, last_access, score];
            if let Some(size_vec) = size_vec.as_ref() {
                let size = utils::human_bytes(size_vec[idx]);
                row.push(size);
            }

            table.add(row);
        }

        table.show();

        if let Some(size_vec) = size_vec {
            let total: u64 = size_vec.into_iter().sum();
            println!();
            println!("Total size: {}", utils::human_bytes(total));
        }

        Ok(())
    }
}

impl GetArgs {
    pub fn completion() -> Completion {
        Completion {
            args: Completion::repo_args,
            flags: Some(Completion::labels),
        }
    }
}
