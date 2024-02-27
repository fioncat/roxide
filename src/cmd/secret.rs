use anyhow::Result;
use clap::Args;

use crate::cmd::{Completion, CompletionResult, Run};
use crate::config::Config;
use crate::secret;

/// Encrypt/Decrypt secret file
#[derive(Args)]
pub struct SecretArgs {
    /// The file to encrypt/decrypt.
    pub file: String,

    /// Write content to path
    #[clap(short = 'f', long)]
    pub write_path: Option<String>,
}

impl Run for SecretArgs {
    fn run(&self, _: &Config) -> Result<()> {
        secret::handle(&self.file, &self.write_path, None)
    }
}

impl SecretArgs {
    pub fn completion() -> Completion {
        Completion {
            args: Completion::files,
            flags: Some(|_cfg, flag, _to_complete| match flag {
                'f' => Ok(Some(CompletionResult::files())),
                _ => Ok(None),
            }),
        }
    }
}
