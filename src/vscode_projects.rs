use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::Serialize;

use crate::config::context::ConfigContext;
use crate::db::DatabaseHandle;
use crate::db::repo::{QueryOptions, Repository};
use crate::info;

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

pub fn sync_json_if_enabled(ctx: &ConfigContext) -> Result<()> {
    if !ctx.cfg.sync_vscode_projects {
        return Ok(());
    }
    info!("Generating vscode projects.json file");
    sync_json(ctx).context("sync vscode projects")
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::config::context;

    use super::*;

    #[test]
    fn test_sync_json_if_enabled() {
        let mut ctx = context::tests::build_test_context("vscode_projects_sync_json_if_enabled");
        let path = Path::new(&ctx.cfg.workspace).join("projects.json");

        sync_json_if_enabled(&ctx).unwrap();
        assert!(!path.exists());

        ctx.cfg.sync_vscode_projects = true;
        sync_json_if_enabled(&ctx).unwrap();

        let data = fs::read_to_string(path).unwrap();
        let projects: serde_json::Value = serde_json::from_str(&data).unwrap();
        let projects = projects.as_array().unwrap();
        assert_eq!(projects.len(), 6);
        assert_eq!(projects[0]["name"], "github/kubernetes/kubernetes");
        assert_eq!(projects[0]["tags"], serde_json::json!(["roxide"]));
        assert_eq!(projects[0]["enabled"], true);
    }
}
