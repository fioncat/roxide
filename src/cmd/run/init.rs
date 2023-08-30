use anyhow::Result;
use clap::{Args, ValueEnum};
use strum::EnumVariantNames;

use crate::cmd::Run;
use crate::config;

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
    fn run(&self) -> Result<()> {
        let base_cmd = match &config::base().command.base {
            Some(base) => base.as_str(),
            None => "ro",
        };
        let home_cmd = match &config::base().command.home {
            Some(home) => home.as_str(),
            None => "rh",
        };

        let init_bytes = include_bytes!("../../../scripts/init.sh");
        let init_script = String::from_utf8_lossy(init_bytes).to_string();

        let complete_bytes = match self.shell {
            Shell::Bash => include_bytes!("../../../scripts/complete_bash.sh").as_slice(),
            Shell::Zsh => include_bytes!("../../../scripts/complete_zsh.zsh").as_slice(),
        };
        let complete_script = String::from_utf8_lossy(complete_bytes).to_string();

        let script = [complete_script, init_script].join("\n");
        let script = script.replace("_roxide_base", base_cmd);
        println!("{script}");

        println!();
        println!("alias {home_cmd}='{base_cmd} home'");
        for (remote, alias) in &config::base().command.remotes {
            println!("alias {alias}='{base_cmd} home {remote}'");
        }

        Ok(())
    }
}
