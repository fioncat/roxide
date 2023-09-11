use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::rc::Rc;

use anyhow::{bail, Context, Result};
use clap::Args;
use reqwest::Url;

use crate::cmd::Run;
use crate::config::types::Remote;
use crate::repo::database::Database;
use crate::repo::query::{parse_owner, Query, SelectOptions};
use crate::repo::tmp_mark::TmpMark;
use crate::repo::types::Repo;
use crate::term::{Cmd, Workflow};
use crate::{config, confirm};

/// Print the home path of a repo, recommand to use `zz` command instead.
#[derive(Args)]
pub struct HomeArgs {
    /// The repo query, format is `[remote] [owner[/[name]]]`
    pub query: Vec<String>,

    /// If true, use search instead of fuzzy matching.
    #[clap(long, short)]
    pub search: bool,

    /// If true, the cache will not be used when calling the API search.
    #[clap(long, short)]
    pub force: bool,

    /// Use url to locate repo.
    #[clap(long, short)]
    pub url: Option<String>,

    /// Mark repo as tmp.
    #[clap(long, short)]
    pub tmp: bool,
}

impl Run for HomeArgs {
    fn run(&self) -> Result<()> {
        let mut db = Database::read()?;

        let (remote, repo, exists) = match self.url.as_ref() {
            Some(url) => self.select_from_url(&db, url)?,
            None => {
                let query = Query::new(&db, self.query.clone());
                query.select(
                    SelectOptions::new()
                        .with_force(self.force)
                        .with_search(self.search),
                )?
            }
        };

        if !exists {
            confirm!(
                "Do you want to create {}/{}",
                repo.owner.as_ref(),
                repo.name.as_ref()
            );
        }
        if self.tmp {
            let mut tmp_mark = TmpMark::read()?;
            tmp_mark.mark(&repo);
            tmp_mark.save()?;
        }

        let dir = repo.get_path();
        match fs::read_dir(&dir) {
            Ok(_) => {}
            Err(err) if err.kind() == ErrorKind::NotFound => {
                self.create_dir(&remote, &repo, &dir)?;
            }
            Err(err) => {
                return Err(err).with_context(|| format!("Read repo directory {}", dir.display()));
            }
        }

        println!("{}", dir.display());
        db.update(repo);

        db.close()
    }
}

impl HomeArgs {
    const GITHUB_DOMAIN: &str = "github.com";

    fn create_dir(&self, remote: &Remote, repo: &Rc<Repo>, dir: &PathBuf) -> Result<()> {
        if let Some(_) = remote.clone {
            return self.clone(remote, repo, dir);
        }
        fs::create_dir_all(dir)
            .with_context(|| format!("Create repo directory {}", dir.display()))?;
        let path = format!("{}", dir.display());
        Cmd::git(&["-C", path.as_str(), "init"])
            .with_desc("Git init")
            .execute()?
            .check()?;
        if let Some(owner) = remote.owners.get(repo.owner.as_str()) {
            if let Some(workflow_names) = &owner.on_create {
                for workflow_name in workflow_names.iter() {
                    let wf = Workflow::new(workflow_name)?;
                    wf.execute_repo(repo)?;
                }
            }
        }

        Ok(())
    }

    fn clone(&self, remote: &Remote, repo: &Rc<Repo>, dir: &PathBuf) -> Result<()> {
        let url = repo.clone_url(&remote);
        let path = format!("{}", dir.display());
        Cmd::git(&["clone", url.as_str(), path.as_str()])
            .with_desc(format!("Clone {}", repo.full_name()))
            .execute()?
            .check()?;

        if let Some(user) = &remote.user {
            Cmd::git(&["-C", path.as_str(), "config", "user.name", user.as_str()])
                .with_desc(format!("Set user to {}", user))
                .execute()?
                .check()?;
        }
        if let Some(email) = &remote.email {
            Cmd::git(&["-C", path.as_str(), "config", "user.email", email.as_str()])
                .with_desc(format!("Set email to {}", email))
                .execute()?
                .check()?;
        }
        Ok(())
    }

    fn select_from_url(&self, db: &Database, url: &String) -> Result<(Remote, Rc<Repo>, bool)> {
        let url = Url::parse(url).with_context(|| format!("Invalid url {url}"))?;
        let domain = match url.domain() {
            Some(domain) => domain,
            None => bail!("Missing domain in url"),
        };
        let remote_names = config::list_remotes();

        let mut target_remote: Option<Remote> = None;
        let mut is_gitlab = false;
        for name in remote_names {
            let remote = config::must_get_remote(name)?;
            let remote_domain = match remote.clone.as_ref() {
                Some(domain) => domain,
                None => continue,
            };

            if remote_domain != domain {
                continue;
            }

            if remote_domain != Self::GITHUB_DOMAIN {
                is_gitlab = true;
            }
            target_remote = Some(remote);
            break;
        }

        let remote = match target_remote {
            Some(remote) => remote,
            None => bail!("Could not find remote match domain {domain}"),
        };

        let path_iter = match url.path_segments() {
            Some(iter) => iter,
            None => bail!("Invalid url, path could not be empty"),
        };

        let mut parts = Vec::new();
        for part in path_iter {
            if is_gitlab {
                if part == "-" {
                    break;
                }
                parts.push(part);
                continue;
            }

            if parts.len() == 2 {
                break;
            }
            parts.push(part);
        }

        if parts.len() < 2 {
            bail!("Invalid url, should be in a repo");
        }

        let query = parts.join("/");
        let (owner, name) = parse_owner(&query);

        match db.get(&remote.name, &owner, &name) {
            Some(repo) => Ok((remote, repo, true)),
            None => {
                let repo = Repo::new(&remote.name, &owner, &name, None);
                Ok((remote, repo, false))
            }
        }
    }
}
