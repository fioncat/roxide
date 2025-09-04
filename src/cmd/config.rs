use anyhow::Result;
use async_trait::async_trait;
use clap::Args;

use super::{Command, ConfigArgs};

#[derive(Debug, Args)]
pub struct ConfigCommand {
    pub remote: Option<String>,

    #[clap(flatten)]
    pub config: ConfigArgs,
}

#[async_trait]
impl Command for ConfigCommand {
    async fn run(self) -> Result<()> {
        let ctx = self.config.build_ctx()?;

        let Some(remote) = self.remote else {
            let json = serde_json::to_string_pretty(&ctx.cfg)?;
            println!("{json}");
            return Ok(());
        };

        let remote = ctx.cfg.get_remote(&remote)?;
        let json = serde_json::to_string_pretty(remote)?;
        println!("{json}");
        Ok(())
    }
}
