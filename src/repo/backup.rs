use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::config::context::ConfigContext;
use crate::db::DatabaseHandle;
use crate::db::mirror::Mirror;
use crate::db::repo::Repository;
use crate::repo::mirror::MirrorSelector;

#[derive(Debug, Serialize, Deserialize)]
pub struct BackupData {
    pub repos: Vec<Repository>,
    pub mirrors: Vec<Mirror>,
}

pub struct BackupDryRunResult {
    pub new_repos: Vec<Repository>,
    pub update_repos: Vec<Repository>,

    pub new_mirrors: Vec<Mirror>,
    pub update_mirrors: Vec<Mirror>,
}

impl BackupData {
    pub fn load(ctx: &ConfigContext, repos: Vec<Repository>) -> Result<Self> {
        let mut mirrors = Vec::new();
        for repo in repos.iter() {
            let selector = MirrorSelector::new(ctx, repo);
            let repo_mirrors = selector.select_many()?;
            mirrors.extend(repo_mirrors);
        }
        Ok(Self { repos, mirrors })
    }

    pub fn restore(&self, _handle: &DatabaseHandle) -> Result<()> {
        todo!()
    }

    pub fn dry_run(&self, _handle: &DatabaseHandle) -> Result<BackupDryRunResult> {
        todo!()
    }
}
