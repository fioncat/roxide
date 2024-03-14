use anyhow::Result;
use clap::Args;

use crate::cmd::Run;
use crate::config::{Config, ProviderType};
use crate::repo::database::Database;

/// Show current repo full name.
#[derive(Args)]
pub struct CurrentArgs {
    /// Display icon for github repo
    #[clap(long)]
    pub github_icon: Option<String>,

    /// Display icon for gitlab repo
    #[clap(long)]
    pub gitlab_icon: Option<String>,

    /// Display icon for others repo
    #[clap(long)]
    pub default_icon: Option<String>,
}

impl Run for CurrentArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
        let db = Database::load(cfg)?;
        if let Some(repo) = db.get_current() {
            let icon = match repo.remote_cfg.provider.as_ref() {
                Some(provider) => match provider {
                    ProviderType::Github => self.github_icon.as_ref(),
                    ProviderType::Gitlab => self.gitlab_icon.as_ref(),
                },
                None => self.default_icon.as_ref(),
            };

            if let Some(icon) = icon {
                println!("{icon} {} {}", repo.remote, repo.name_with_owner());
            } else {
                println!("{} {}", repo.remote, repo.name_with_owner());
            }
        }

        Ok(())
    }
}
