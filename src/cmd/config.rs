use std::fs;

use anyhow::{Result, bail};
use async_trait::async_trait;
use clap::{Args, ValueEnum};
use serde::{Deserialize, Serialize};

use crate::cmd::complete;
use crate::config::Config;
use crate::config::context::ConfigContext;
use crate::config::hook::HooksConfig;
use crate::config::remote::RemoteConfig;
use crate::debug;

use super::Command;

#[derive(Debug, Args)]
pub struct ConfigCommand {
    pub config_type: Option<ConfigType>,

    pub name: Option<String>,

    #[arg(long, short)]
    pub edit: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ConfigType {
    Remote,
    Hook,
}

#[async_trait]
impl Command for ConfigCommand {
    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Run config command: {:?}", self);

        let Some(config_type) = self.config_type else {
            if self.edit {
                return ctx.edit(&ctx.cfg.path);
            }
            return print_json(&ctx.cfg);
        };

        match config_type {
            ConfigType::Remote => {
                let Some(name) = self.name else {
                    return print_json(&ctx.cfg.remotes);
                };
                if self.edit {
                    let path = ctx.cfg.remotes_dir.join(format!("{name}.toml"));
                    return ctx.edit(&path);
                }
                let remote = ctx.cfg.get_remote(&name)?;
                print_json(&remote)
            }
            ConfigType::Hook => {
                let Some(name) = self.name else {
                    let mut hooks = ctx.cfg.hooks.hooks.into_keys().collect::<Vec<_>>();
                    hooks.sort_unstable();
                    return print_json(&hooks);
                };
                if self.edit {
                    let path = ctx.cfg.hooks_dir.join(format!("{name}.sh"));
                    return ctx.edit(&path);
                }
                let Some(path) = ctx.cfg.hooks.get(&name) else {
                    bail!("hook {name:?} not found");
                };
                let text = fs::read_to_string(path)?;
                print!("# {path}\n{text}");
                Ok(())
            }
        }
    }

    fn complete_command() -> clap::Command {
        clap::Command::new("config")
            .args([complete::config_type_arg(), complete::config_name_arg()])
    }
}

fn print_json<T>(value: &T) -> Result<()>
where
    T: Serialize,
{
    let json = serde_json::to_string_pretty(value)?;
    println!("{json}");
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
struct ConfigDisplay {
    pub base: Config,

    pub remotes: Vec<RemoteConfig>,

    pub hooks: HooksConfig,
}
