use anyhow::Result;
use async_trait::async_trait;
use clap::{Arg, Args, ValueHint};

use crate::config::context::ConfigContext;
use crate::debug;
use crate::secret::{decrypt_single, encrypt_single, get_password};

use super::Command;

#[derive(Debug, Args)]
pub struct SecretCommand {
    pub src: Option<String>,

    pub dest: Option<String>,

    #[arg(long, short)]
    pub update_password: bool,

    #[arg(long, short, default_value = "4096")]
    pub buffer_size: usize,

    #[arg(long, short)]
    pub encrypt: bool,

    #[arg(long, short)]
    pub decrypt: bool,
}

#[async_trait]
impl Command for SecretCommand {
    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Run secret command: {:?}", self);

        if self.update_password {
            get_password(&ctx, true)?;
            return Ok(());
        }

        let password = get_password(&ctx, false)?;
        if self.encrypt {
            return encrypt_single(self.src, self.dest, &password, self.buffer_size).await;
        }

        if self.decrypt {
            return decrypt_single(self.src, self.dest, &password).await;
        }

        Ok(())
    }

    fn complete_command() -> clap::Command {
        clap::Command::new("secret").args([
            Arg::new("src").value_hint(ValueHint::FilePath),
            Arg::new("dest").value_hint(ValueHint::FilePath),
        ])
    }
}
