use anyhow::Result;
use async_trait::async_trait;
use clap::Args;

use crate::config::context::ConfigContext;
use crate::debug;
use crate::secret::{SecretArgs, decrypt_many, decrypt_one};

use super::Command;

/// Decrypt one or more files
#[derive(Debug, Args)]
pub struct DecryptCommand {
    #[clap(flatten)]
    pub secret: SecretArgs,
}

#[async_trait]
impl Command for DecryptCommand {
    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Run decrypt command: {:?}", self);

        let password = self.secret.get_password(&ctx)?;

        if let Some(file) = self.secret.file {
            decrypt_one(self.secret.src, file, &password).await
        } else {
            decrypt_many(self.secret.into_many_base_dir(ctx)?, &password).await
        }
    }

    fn complete_command() -> clap::Command {
        clap::Command::new("decrypt")
    }
}
