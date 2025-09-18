use anyhow::Result;
use async_trait::async_trait;
use clap::Args;

use crate::config::context::ConfigContext;
use crate::db::repo::{DisplayLevel, Repository};
use crate::debug;
use crate::repo::current::get_current_repo;
use crate::repo::mirror::get_current_mirror;

use super::Command;

/// Display the name of the current repository (or mirror).
#[derive(Debug, Args)]
pub struct WhichCommand {}

#[async_trait]
impl Command for WhichCommand {
    fn name() -> &'static str {
        "which"
    }

    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Run which command: {:?}", self);

        if let Some((repo, mirror)) = get_current_mirror(&ctx)? {
            let mirror_repo = mirror.into_repo();
            let remote_icon = get_remote_icon(&ctx, &mirror_repo)?;
            let mirror_icon = ctx.cfg.mirror_icon.as_deref().unwrap_or("mirror");
            let text = format!(
                "{mirror_icon} {remote_icon} {} ({})",
                mirror_repo.display_name(DisplayLevel::Owner),
                repo.name
            );
            println!("{text}");
            return Ok(());
        };

        let repo = get_current_repo(&ctx)?;
        let icon = get_remote_icon(&ctx, &repo)?;
        println!("{icon} {}", repo.display_name(DisplayLevel::Owner));
        Ok(())
    }
}

fn get_remote_icon<'a>(ctx: &'a ConfigContext, repo: &Repository) -> Result<&'a str> {
    let remote = ctx.cfg.get_remote(&repo.remote)?;
    match remote.icon {
        Some(ref icon) => Ok(icon),
        None => Ok(remote.name.as_str()),
    }
}
