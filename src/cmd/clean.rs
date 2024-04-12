use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Args;

use crate::cmd::Run;
use crate::config::Config;
use crate::repo::database::Database;
use crate::{api, info, term, utils};

/// Collect and remove unused garbage.
#[derive(Args)]
pub struct CleanArgs {
    /// Skip cleaning remote api.
    #[clap(long)]
    pub no_remote: bool,

    /// When calling the remote API, ignore caches that are not expired.
    #[clap(long, short)]
    pub force: bool,
}

impl Run for CleanArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
        if !self.no_remote {
            self.clean_remote(cfg)?;
        }

        self.clean_orphan(cfg)
    }
}

impl CleanArgs {
    fn clean_remote(&self, cfg: &Config) -> Result<()> {
        let mut db = Database::load(cfg)?;

        let remotes = cfg.list_remotes();
        let mut to_remove = Vec::new();
        for remote_name in remotes {
            let remote = match cfg.get_remote(&remote_name) {
                Some(remote) => remote,
                None => continue,
            };

            if remote.provider.is_none() {
                continue;
            }

            let provider = api::build_provider(cfg, &remote, self.force)
                .with_context(|| format!("build provider for remote '{remote_name}'"))?;

            let repos = db.list_by_remote(&remote_name, &None);

            for repo in repos {
                info!("Check remote api for {}", repo.name_with_remote());
                if let Err(err) = provider.get_repo(&repo.owner, &repo.name) {
                    eprintln!("Repo {} has remote error: {err:#}", repo.name_with_remote());
                    let ok = term::confirm("Do you want to remove it")?;
                    if ok {
                        to_remove.push(repo.update());
                    }
                }
            }
        }
        if to_remove.is_empty() {
            eprintln!("No repo to remove");
            return Ok(());
        }

        for repo in to_remove {
            let path = repo.get_path(cfg);
            utils::remove_dir_recursively(path)?;
            db.remove(repo);
        }

        db.save()
    }

    fn clean_orphan(&self, cfg: &Config) -> Result<()> {
        let db = Database::load(cfg)?;
        let root = cfg.get_workspace_dir().clone();

        let repo_set: HashSet<PathBuf> = db
            .list_all(&None)
            .iter()
            .map(|repo| repo.get_path(cfg))
            .collect();

        let mut items: Vec<String> = Vec::new();
        let mut dirs: Vec<PathBuf> = Vec::new();
        let mut files: Vec<PathBuf> = Vec::new();

        info!("Scan orphan under '{}'", root.display());
        utils::walk_dir(root.clone(), |path, meta| {
            let path = path.clone();
            if !meta.is_dir() {
                let rel_path = path.strip_prefix(&root).unwrap();
                items.push(format!("{}", rel_path.display()));
                files.push(path);
                return Ok(false);
            }

            if repo_set.get(&path).is_some() {
                return Ok(false);
            }

            for repo_path in repo_set.iter() {
                if repo_path.starts_with(&path) {
                    return Ok(true);
                }
            }

            let rel_path = path.strip_prefix(&root).unwrap();
            items.push(format!("{}/", rel_path.display()));
            dirs.push(path);

            Ok(false)
        })?;

        if items.is_empty() {
            eprintln!("No orphan to remove");
            return Ok(());
        }

        term::must_confirm_items(&items, "remove", "removal", "Orphan", "Orphans")?;
        for file in files {
            info!("Remove file {}", file.display());
            fs::remove_file(&file).with_context(|| format!("remove file '{}'", file.display()))?;
        }

        for dir in dirs {
            utils::remove_dir_recursively(dir)?;
        }

        Ok(())
    }
}
