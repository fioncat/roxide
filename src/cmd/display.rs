use std::borrow::Cow;
use std::path::PathBuf;

use anyhow::{bail, Result};
use clap::Args;

use crate::cmd::Run;
use crate::config::Config;
use crate::repo::database::Database;

/// Display repository name.
#[derive(Args)]
pub struct DisplayArgs {
    /// The path to display, default is current path.
    pub path: Option<String>,
}

impl Run for DisplayArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
        let path = self
            .path
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or(cfg.get_current_dir().clone());

        let db = Database::load(cfg)?;
        let repos = db.list_all(&None);

        let repo = repos.into_iter().find(|repo| {
            let repo_path = repo.get_path(cfg);
            path.starts_with(repo_path)
        });
        if repo.is_none() {
            bail!("no repo to display");
        }
        let repo = repo.unwrap();

        let icon = repo
            .remote_cfg
            .icon
            .as_ref()
            .map(Cow::Borrowed)
            .unwrap_or(Cow::Owned(format!("<{}>", repo.remote)));

        let format = cfg.display_format.as_str();

        // TODO: Support color
        let output = format.replace("{icon}", icon.as_ref());
        let output = output.replace("{remote}", repo.remote.as_ref());
        let output = output.replace("{owner}", repo.owner.as_ref());
        let output = output.replace("{name}", repo.name.as_ref());

        println!("{output}");

        Ok(())
    }
}
