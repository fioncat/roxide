use std::fs;

use anyhow::{Result, bail};
use async_trait::async_trait;
use clap::{Args, ValueEnum};
use serde::Serialize;

use crate::cmd::complete::{CompleteArg, CompleteCommand, funcs};
use crate::config::context::ConfigContext;
use crate::debug;

use super::Command;

/// Print configuration in JSON format or directly edit the configuration file.
#[derive(Debug, Args)]
pub struct ConfigCommand {
    /// Configuration file type. If not provided, defaults to base config
    /// ($ROXIDE_CONFIG/config.toml). Use 'remote' for remote config
    /// ($ROXIDE_CONFIG/remotes/{remote}.toml), or 'hook' for hook scripts
    /// ($ROXIDE_CONFIG/hooks/{hook}.sh).
    pub config_type: Option<ConfigType>,

    /// Remote configuration name or hook script name.
    pub name: Option<String>,

    /// Edit the configuration file directly in the default editor.
    #[arg(short)]
    pub edit: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ConfigType {
    Remote,
    Hook,
}

#[async_trait]
impl Command for ConfigCommand {
    fn name() -> &'static str {
        "config"
    }

    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Running config command: {:?}", self);

        let Some(config_type) = self.config_type else {
            if self.edit {
                return ctx.edit(&ctx.cfg.path);
            }
            return print_config(&ctx.cfg);
        };

        match config_type {
            ConfigType::Remote => {
                let Some(name) = self.name else {
                    return print_config(&ctx.cfg.remotes);
                };
                if self.edit {
                    let path = ctx.cfg.remotes_dir.join(format!("{name}.toml"));
                    return ctx.edit(&path);
                }
                let remote = ctx.cfg.get_remote(&name)?;
                print_config(&remote)
            }
            ConfigType::Hook => {
                let Some(name) = self.name else {
                    let mut hooks = ctx.cfg.hook_runs.hooks.into_keys().collect::<Vec<_>>();
                    hooks.sort_unstable();
                    return print_config(&hooks);
                };
                if self.edit {
                    let path = ctx.cfg.hooks_dir.join(format!("{name}.sh"));
                    return ctx.edit(&path);
                }
                let Some(path) = ctx.cfg.hook_runs.get(&name) else {
                    bail!("hook {name:?} not found");
                };
                let text = fs::read_to_string(path)?;
                print!("# {path}\n{text}");
                Ok(())
            }
        }
    }

    fn complete() -> CompleteCommand {
        Self::default_complete()
            .arg(CompleteArg::new().complete(funcs::complete_config_type))
            .arg(CompleteArg::new().complete(funcs::complete_config_name))
            .arg(CompleteArg::new().short('e'))
    }
}

fn print_config<T>(value: &T) -> Result<()>
where
    T: Serialize,
{
    let toml = toml::to_string_pretty(value)?;
    println!("{toml}");
    Ok(())
}
