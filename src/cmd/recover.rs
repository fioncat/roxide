use std::path::PathBuf;
use std::rc::Rc;
use std::{fs, io};

use anyhow::{bail, Context, Result};
use clap::Args;

use crate::cmd::Run;
use crate::config::Config;
use crate::repo::database::{self, Database};
use crate::repo::Repo;
use crate::{confirm, info, stderr, term, utils};

/// Recover database
#[derive(Args)]
pub struct RecoverArgs {}

impl Run for RecoverArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
        match Database::load(cfg) {
            Ok(_) => {}
            Err(err) => {
                info!("Database has problem: '{}', replace it with a new one. The old one will be saved to '{}/database.back'", err, cfg.get_meta_dir().display());
                confirm!("Continue");
                Self::replace_database(cfg)?;
            }
        };

        let mut db = Database::load(cfg)?;
        let workspace_repos = Self::scan_workspace(cfg)?;
        let to_add: Vec<Rc<Repo>> = workspace_repos
            .into_iter()
            .filter(|repo| {
                if let Some(_) = db.get(
                    repo.remote.name.as_str(),
                    repo.owner.name.as_str(),
                    repo.name.as_str(),
                ) {
                    return false;
                }
                true
            })
            .collect();

        if to_add.is_empty() {
            stderr!("Nothing to recover");
            return Ok(());
        }

        let items: Vec<_> = to_add.iter().map(|repo| repo.name_with_remote()).collect();
        term::must_confirm_items(&items, "add", "addition", "Repo", "Repos")?;
        info!("Add {} to database done", utils::plural(&to_add, "repo"));
        for repo in to_add {
            db.update(repo, None);
        }

        db.save()
    }
}

impl RecoverArgs {
    fn replace_database(cfg: &Config) -> Result<()> {
        let old_path = cfg.get_meta_dir().join("database");
        let new_path = cfg.get_meta_dir().join("database.back");
        fs::rename(&old_path, &new_path).with_context(|| {
            format!(
                "rename '{}' to '{}'",
                old_path.display(),
                new_path.display()
            )
        })?;
        Ok(())
    }

    fn scan_workspace(cfg: &Config) -> Result<Vec<Rc<Repo>>> {
        info!("Scanning workspace");
        let mut repos = Vec::new();
        let dir = cfg.get_workspace_dir().clone();
        let workspace = dir.clone();

        utils::walk_dir(dir, |path, meta| {
            if !meta.is_dir() {
                return Ok(false);
            }
            let git_dir = path.join(".git");
            match fs::read_dir(&git_dir) {
                Ok(_) => {}
                Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(true),
                Err(err) => {
                    return Err(err).with_context(|| format!("read git dir {}", git_dir.display()))
                }
            }
            if !path.starts_with(&workspace) {
                return Ok(true);
            }
            let rel = path.strip_prefix(&workspace).with_context(|| {
                format!(
                    "strip prefix for dir {}, workspace {}",
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
                    "scan found invalid rel path {}, missing remote",
                    rel.display()
                ),
            };

            let query = format!("{}", iter.collect::<PathBuf>().display());
            let query = query.trim_matches('/');
            let (owner, name) = database::parse_owner(query);
            if owner.is_empty() || name.is_empty() {
                bail!(
                    "Scan found invalid rel path {}, missing owner or name",
                    rel.display()
                );
            }

            let repo = Repo::new(cfg, remote, &owner, &name, None, &None)?;

            repos.push(repo);

            Ok(false)
        })?;

        Ok(repos)
    }
}
