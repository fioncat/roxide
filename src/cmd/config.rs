use anyhow::Result;
use async_trait::async_trait;
use clap::Args;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::config::context::ConfigContext;
use crate::config::hook::HooksConfig;
use crate::config::remote::RemoteConfig;

use super::Command;

#[derive(Debug, Args)]
pub struct ConfigCommand {}

#[async_trait]
impl Command for ConfigCommand {
    async fn run(self, ctx: ConfigContext) -> Result<()> {
        let remotes = ctx.cfg.remotes.clone();
        let hooks = ctx.cfg.hooks.clone();
        let display = ConfigDisplay {
            base: ctx.cfg,
            remotes,
            hooks,
        };
        let json = serde_json::to_string_pretty(&display)?;
        println!("{json}");
        Ok(())
    }

    fn complete_command() -> clap::Command {
        Self::augment_args(clap::Command::new("config"))
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ConfigDisplay {
    pub base: Config,

    pub remotes: Vec<RemoteConfig>,

    pub hooks: HooksConfig,
}
