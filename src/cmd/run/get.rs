use std::rc::Rc;

use anyhow::{bail, Context, Result};
use clap::Args;
use serde::Serialize;

use crate::cmd::Run;
use crate::repo::database::Database;
use crate::repo::types::{NameLevel, Repo};
use crate::utils::{self, Table};

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
        Ok(RepoInfo {
            remote: format!("{}", repo.remote),
            owner: format!("{}", repo.owner),
            name: format!("{}", repo.name),
            accessed: repo.accessed as u64,
            last_accessed: utils::format_time(repo.last_accessed)?,
            score,
            path,
            workspace,
            size: utils::dir_size(repo.get_path())?,
        })
    }
}

impl Run for GetArgs {
    fn run(&self) -> Result<()> {
        let db = Database::read()?;
        if let None = self.remote {
            return self.list(&db, None, None);
        }
        let remote = self.remote.as_ref().unwrap().as_str();
        if let None = self.query {
            return self.list(&db, Some(remote), None);
        }
        let query = self.query.as_ref().unwrap().as_str();
        if query.ends_with("/") {
            let owner = query.trim_end_matches("/");
            return self.list(&db, Some(remote), Some(owner));
        }

        let (owner, name) = utils::parse_query(query);
        if owner.is_empty() || name.is_empty() {
            bail!("Invalid query {query}, you should provide owner and name");
        }

        let repo = db.must_get(remote, owner.as_str(), name.as_str())?;
        let info = RepoInfo::from_repo(repo)?;
        let yaml = serde_yaml::to_string(&info).context("Encode info yaml")?;
        print!("{yaml}");
        Ok(())
    }
}

impl GetArgs {
    fn list(&self, db: &Database, remote: Option<&str>, owner: Option<&str>) -> Result<()> {
        let (repos, level) = match remote {
            Some(remote) => match owner {
                Some(owner) => (db.list_by_owner(remote, owner), NameLevel::Name),
                None => (db.list_by_remote(remote), NameLevel::Owner),
            },
            None => (db.list_all(), NameLevel::Full),
        };
        if repos.is_empty() {
            println!("Nothing to show");
            return Ok(());
        }

        let mut table = Table::with_capacity(1 + repos.len());
        let mut titles = vec![
            String::from("NAME"),
            String::from("ACCESS"),
            String::from("LAST_ACCESS"),
            String::from("SCORE"),
        ];
        if self.size {
            titles.push(String::from("SIZE"));
        }
        table.add(titles);

        for repo in repos {
            let name = repo.as_string(&level);
            let access = format!("{}", repo.accessed as u64);
            let last_access = utils::format_time(repo.last_accessed)?;
            let score = format!("{:.2}", repo.score());

            let mut row = vec![name, access, last_access, score];
            if self.size {
                row.push(utils::dir_size(repo.get_path())?);
            }

            table.add(row);
        }

        table.show();
        Ok(())
    }
}
