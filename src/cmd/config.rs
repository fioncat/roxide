use anyhow::Result;
use async_trait::async_trait;
use clap::Args;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::config::hook::HooksConfig;
use crate::config::remote::RemoteConfig;

use super::{Command, ConfigArgs};

#[derive(Debug, Args)]
pub struct ConfigCommand {
    #[clap(flatten)]
    pub config: ConfigArgs,
}

#[async_trait]
impl Command for ConfigCommand {
    async fn run(self) -> Result<()> {
        let cfg = self.config.build_config()?;

        let remotes = cfg.remotes.clone();
        let hooks = cfg.hooks.clone();
        let display = ConfigDisplay {
            base: cfg,
            remotes,
            hooks,
        };
        let json = serde_json::to_string_pretty(&display)?;
        println!("{json}");
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ConfigDisplay {
    pub base: Config,

    pub remotes: Vec<RemoteConfig>,

    pub hooks: HooksConfig,
}
