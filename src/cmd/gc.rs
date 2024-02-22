use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Args;

use crate::cmd::Run;
use crate::config::Config;
use crate::repo::database::Database;
use crate::{info, term, utils};

/// Remove unused file(s) and dir(s) in workspace
#[derive(Args)]
pub struct GcArgs {}

impl Run for GcArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
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

        info!("Scanning orphans under '{}'", root.display());
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
