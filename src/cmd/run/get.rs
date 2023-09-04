use std::rc::Rc;

use anyhow::Result;
use clap::Args;
use serde::Serialize;

use crate::cmd::Run;
use crate::repo::database::Database;
use crate::repo::query::Query;
use crate::repo::types::Repo;
use crate::shell::Table;
use crate::utils;
use crate::{info, shell};

/// Get or list repo info.
#[derive(Args)]
pub struct GetArgs {
    /// The remote name.
    pub remote: Option<String>,

    /// The repo query, format is `owner[/[name]]`.
    pub query: Option<String>,

    /// Show size in list info. If your workspace is large, this can cause
    /// command to take too long to execute.
    #[clap(long, short)]
    pub size: bool,

    /// Show current repo info.
    #[clap(long, short)]
    pub current: bool,
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
}

impl RepoInfo {
    fn from_repo(repo: Rc<Repo>) -> Result<Self> {
        let workspace = match repo.path {
            Some(_) => false,
            None => true,
        };
        let path = repo.get_path();
        let path = format!("{}", path.display());
        let score = format!("{:.2}", repo.score());
        let size = utils::dir_size(repo.get_path())?;
        Ok(RepoInfo {
            remote: format!("{}", repo.remote),
            owner: format!("{}", repo.owner),
            name: format!("{}", repo.name),
            accessed: repo.accessed as u64,
            last_accessed: utils::format_time(repo.last_accessed)?,
            score,
            path,
            workspace,
            size: utils::human_bytes(size),
        })
    }
}

impl Run for GetArgs {
    fn run(&self) -> Result<()> {
        let db = Database::read()?;
        if self.current {
            let repo = db.must_current()?;
            let info = RepoInfo::from_repo(repo)?;
            return shell::show_json(info);
        }

        let query = Query::from_args(&db, &self.remote, &self.query);
        let (mut repos, level) = query.list_local(false)?;
        if repos.is_empty() {
            info!("No repo to show");
            return Ok(());
        }

        if repos.len() == 1 {
            let repo = repos.into_iter().next().unwrap();
            let info = RepoInfo::from_repo(repo)?;
            return shell::show_json(info);
        }
        let mut table = Table::with_capacity(1 + repos.len());
        let mut titles = vec![
            String::from("NAME"),
            String::from("ACCESS"),
            String::from("LAST_ACCESS"),
            String::from("SCORE"),
        ];

        let mut size_vec: Option<Vec<u64>> = None;
        if self.size {
            titles.push(String::from("SIZE"));
            let mut repos_with_size = Vec::with_capacity(repos.len());
            for repo in repos {
                let path = repo.get_path();
                let size = utils::dir_size(path)?;
                repos_with_size.push((size, repo));
            }
            repos_with_size.sort_unstable_by(|(size1, _), (size2, _)| size2.cmp(size1));
            size_vec = Some(repos_with_size.iter().map(|(size, _)| *size).collect());
            repos = repos_with_size.into_iter().map(|(_, repo)| repo).collect();
        }
        table.add(titles);

        for (idx, repo) in repos.iter().enumerate() {
            let name = repo.as_string(&level);
            let access = format!("{}", repo.accessed as u64);
            let last_access = utils::format_since(repo.last_accessed);
            let score = format!("{:.2}", repo.score());

            let mut row = vec![name, access, last_access, score];
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
