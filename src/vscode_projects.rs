use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::Serialize;

use crate::config::context::ConfigContext;
use crate::db::DatabaseHandle;
use crate::db::repo::{QueryOptions, Repository};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Project {
    pub name: String,
    #[serde(rename = "rootPath")]
    pub root_path: String,
    pub paths: Vec<String>,
    pub tags: Vec<String>,
    pub enabled: bool,
    pub profile: String,
}

pub fn sync_json(ctx: &ConfigContext) -> Result<()> {
    let repos: Vec<Repository> = ctx
        .get_db()?
        .with_transaction(|tx: &DatabaseHandle<'_>| tx.repo().query(QueryOptions::default()))?;
    let mut projects = Vec::with_capacity(repos.len());
    for repo in repos {
        let path = repo.get_path(&ctx.cfg.workspace);
        let project = Project {
            name: format!("{}/{}/{}", repo.remote, repo.owner, repo.name),
            root_path: format!("{}", path.display()),
            paths: Vec::new(),
            tags: vec![String::from("roxide")],
            enabled: true,
            profile: String::new(),
        };
        projects.push(project);
    }

    let json: String = serde_json::to_string_pretty(&projects)?;
    let path = PathBuf::from(&ctx.cfg.workspace).join("projects.json");

    fs::write(&path, json).context("write json to projects file")?;
    Ok(())
}
