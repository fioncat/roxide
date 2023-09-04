use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Args;

use crate::cmd::Run;
use crate::repo::database::Database;
use crate::{config, info, shell, utils};

/// Remove unused path in workspace
#[derive(Args)]
pub struct GcArgs {
    /// Run gc under remote
    pub remote: Option<String>,

    /// Run gc under owner
    pub owner: Option<String>,
}

impl Run for GcArgs {
    fn run(&self) -> Result<()> {
        let db = Database::read()?;
        let mut root = PathBuf::from(&config::base().workspace);
        if let Some(remote) = self.remote.as_ref() {
            root = root.join(remote);
            if let Some(owner) = self.owner.as_ref() {
                root = root.join(owner);
            }
        }

        let repo_set: HashSet<PathBuf> = db.list_all().iter().map(|repo| repo.get_path()).collect();

        let mut items: Vec<String> = Vec::new();
        let mut dirs: Vec<PathBuf> = Vec::new();
        let mut files: Vec<PathBuf> = Vec::new();

        info!("Scanning orphans under {}", root.display());
        utils::walk_dir(root.clone(), |path, meta| {
            let path = path.clone();
            if !meta.is_dir() {
                let rel_path = path.strip_prefix(&root).unwrap();
                items.push(format!("{}", rel_path.display()));
                files.push(path);
                return Ok(false);
            }

            if let Some(_) = repo_set.get(&path) {
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
            info!("No orphan to remove");
            return Ok(());
        }

        shell::must_confirm_items(&items, "remove", "removal", "Orphan", "Orphans")?;
        for file in files {
            info!("Remove file {}", file.display());
            fs::remove_file(&file).with_context(|| format!("Remove file {}", file.display()))?;
        }

        for dir in dirs {
            utils::remove_dir_recursively(dir)?;
        }

        Ok(())
    }
}
