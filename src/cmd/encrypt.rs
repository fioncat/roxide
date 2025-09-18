use anyhow::Result;
use async_trait::async_trait;
use clap::Args;

use crate::config::context::ConfigContext;
use crate::debug;
use crate::secret::{BufferArgs, SecretArgs, encrypt_many, encrypt_one};

use super::Command;

/// Encrypt one or more files
#[derive(Debug, Args)]
pub struct EncryptCommand {
    #[clap(flatten)]
    pub secret: SecretArgs,

    #[clap(flatten)]
    pub buffer: BufferArgs,
}

#[async_trait]
impl Command for EncryptCommand {
    fn name() -> &'static str {
        "encrypt"
    }

    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Run encrypt command: {:?}", self);

        let password = self.secret.get_password(&ctx)?;

        if let Some(file) = self.secret.file {
            encrypt_one(self.secret.src, file, &password, self.buffer.size).await
        } else {
            encrypt_many(
                self.secret.into_many_base_dir(ctx)?,
                &password,
                self.buffer.size,
            )
            .await
        }
    }
}
