use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::rc::Rc;

use anyhow::{bail, Context, Result};
use clap::Args;

use crate::cmd::Run;
use crate::repo::database::Database;
use crate::repo::query;
use crate::repo::types::Repo;
use crate::{config, info, shell, utils};

/// Use workspace to recover database
#[derive(Args)]
pub struct RecoverArgs {}

impl Run for RecoverArgs {
    fn run(&self) -> Result<()> {
        let mut db = Database::read()?;

        let workspace_repos = self.scan_workspace()?;
        let to_add: Vec<Rc<Repo>> = workspace_repos
            .into_iter()
            .filter(|repo| {
                if let Some(_) = db.get(
                    repo.remote.as_str(),
                    repo.owner.as_str(),
                    repo.name.as_str(),
                ) {
                    return false;
                }
                true
            })
            .collect();

        if to_add.is_empty() {
            info!("Nothing to recover");
            return Ok(());
        }
        let items: Vec<_> = to_add.iter().map(|repo| repo.full_name()).collect();
        shell::must_confirm_items(&items, "add", "addition", "Repo", "Repos")?;
        info!("Add {} to database done", utils::plural(&to_add, "repo"));
        for repo in to_add {
            db.update(repo);
        }

        db.close()
    }
}

impl RecoverArgs {
    fn scan_workspace(&self) -> Result<Vec<Rc<Repo>>> {
        info!("Scanning workspace");
        let mut repos = Vec::new();
        let dir = PathBuf::from(&config::base().workspace);
        let workspace = dir.clone();

        utils::walk_dir(dir, |path, meta| {
            if !meta.is_dir() {
                return Ok(false);
            }
            let git_dir = path.join(".git");
            match fs::read_dir(&git_dir) {
                Ok(_) => {}
                Err(err) if err.kind() == ErrorKind::NotFound => return Ok(true),
                Err(err) => {
                    return Err(err).with_context(|| format!("Read git dir {}", git_dir.display()))
                }
            }
            if !path.starts_with(&workspace) {
                return Ok(true);
            }
            let rel = path.strip_prefix(&workspace).with_context(|| {
                format!(
                    "Strip prefix for dir {}, workspace {}",
                    path.display(),
                    workspace.display()
                )
            })?;
            let mut iter = rel.iter();
            let remote = match iter.next() {
                Some(s) => match s.to_str() {
                    Some(s) => s,
                    None => return Ok(true),
                },
                None => bail!(
                    "Scan found invalid rel path {}, missing remote",
                    rel.display()
                ),
            };

            let query = format!("{}", iter.collect::<PathBuf>().display());
            let query = query.trim_matches('/');
            let (owner, name) = query::parse_owner(query);
            if owner.is_empty() || name.is_empty() {
                bail!(
                    "Scan found invalid rel path {}, missing owner or name",
                    rel.display()
                );
            }

            repos.push(Rc::new(Repo {
                remote: Rc::new(remote.to_string()),
                owner: Rc::new(owner),
                name: Rc::new(name),
                path: None,
                last_accessed: 0,
                accessed: 0.0,
            }));

            Ok(false)
        })?;

        Ok(repos)
    }
}
