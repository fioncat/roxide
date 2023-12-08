use anyhow::Result;
use clap::{Args, ValueEnum};
use strum::{EnumVariantNames, VariantNames};

use crate::cmd::{Completion, CompletionResult, Run};
use crate::config::Config;

/// Print the init script.
#[derive(Args)]
pub struct InitArgs {
    /// The shell type.
    pub shell: Shell,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, EnumVariantNames)]
#[strum(serialize_all = "kebab-case")]
pub enum Shell {
    Bash,
    Zsh,
}

impl Run for InitArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
        let init_bytes = include_bytes!("../../scripts/init.sh");
        let init_script = String::from_utf8_lossy(init_bytes).to_string();

        let complete_bytes = match self.shell {
            Shell::Bash => include_bytes!("../../scripts/complete_bash.sh").as_slice(),
            Shell::Zsh => include_bytes!("../../scripts/complete_zsh.zsh").as_slice(),
        };
        let complete_script = String::from_utf8_lossy(complete_bytes).to_string();

        let mut script = [complete_script, init_script].join("\n");
        if !cfg.cmd.is_empty() {
            script = script.replace("_roxide_base", &cfg.cmd);
        }
        println!("{script}");

        Ok(())
    }
}

impl InitArgs {
    pub fn completion() -> Completion {
        Completion {
            args: Self::completion_shell,
            flags: None,
        }
    }

    fn completion_shell(_cfg: &Config, _args: &[&str]) -> Result<CompletionResult> {
        let items: Vec<String> = Shell::VARIANTS
            .iter()
            .map(|item| item.to_string())
            .collect();
        Ok(CompletionResult::from(items))
    }
}
