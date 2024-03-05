use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;

use anyhow::{bail, Result};
use clap::{Args, ValueEnum};
use serde::Serialize;
use strum::VariantNames;

use crate::cmd::{Completion, CompletionResult, Run};
use crate::config::{Config, RemoteConfig, WorkflowConfig};
use crate::{term, utils};

/// Edit config file in terminal.
#[derive(Args)]
pub struct ConfigArgs {
    /// The config type.
    pub config_type: Option<ConfigType>,

    /// The config name.
    pub name: Option<String>,

    /// Show the config as json.
    #[clap(short, long)]
    pub show: bool,
}

#[derive(Clone, ValueEnum, VariantNames)]
#[strum(serialize_all = "kebab-case")]
pub enum ConfigType {
    Remotes,
    Workflows,
}

#[derive(Debug, Serialize)]
struct ConfigDisplay<'a> {
    config: &'a Config,

    remotes: &'a HashMap<String, RemoteConfig>,

    workflows: &'a HashMap<String, WorkflowConfig>,
}

impl Run for ConfigArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
        if self.show {
            let display = ConfigDisplay {
                config: cfg,
                remotes: &cfg.remotes,
                workflows: &cfg.workflows,
            };
            return term::show_json(display);
        }

        let root = Config::get_path()?;

        let path = if self.config_type.is_none() {
            root.join("config.toml")
        } else {
            let cfg_type = match self.config_type.as_ref().unwrap() {
                ConfigType::Remotes => "remotes",
                ConfigType::Workflows => "workflows",
            };
            let dir = root.join(cfg_type);
            let name = match self.name.as_ref() {
                Some(name) => Cow::Borrowed(name),
                None => Cow::Owned(self.select_config_name(&dir)?),
            };
            dir.join(format!("{name}.toml"))
        };

        utils::ensure_dir(&path)?;
        let editor = term::get_editor()?;
        term::edit_file(editor, &path)
    }
}

impl ConfigArgs {
    fn select_config_name(&self, dir: &Path) -> Result<String> {
        let mut names = self.config_type.as_ref().unwrap().list_names(dir)?;

        if names.is_empty() {
            bail!(
                "no config item under '{}' to select, please create one first",
                dir.display()
            );
        }

        let idx = term::fzf_search(&names)?;

        Ok(names.remove(idx))
    }

    pub fn completion() -> Completion {
        Completion {
            args: |_cfg, args| match args.len() {
                0 | 1 => {
                    let items: Vec<String> = ConfigType::VARIANTS
                        .iter()
                        .map(|item| item.to_string())
                        .collect();
                    Ok(CompletionResult::from(items))
                }
                2 => {
                    let root = Config::get_path()?;
                    let config_type = match ConfigType::from_str(args[0], false) {
                        Ok(t) => t,
                        Err(_) => return Ok(CompletionResult::empty()),
                    };

                    let dir = root.join(args[0]);
                    let names = config_type.list_names(&dir)?;

                    Ok(CompletionResult::from(names))
                }
                _ => Ok(CompletionResult::empty()),
            },
            flags: None,
        }
    }
}

impl ConfigType {
    fn list_names(&self, dir: &Path) -> Result<Vec<String>> {
        let mut names: Vec<String> = match self {
            ConfigType::Remotes => Config::load_remotes(dir)?.into_keys().collect(),
            ConfigType::Workflows => Config::load_workflows(dir)?.into_keys().collect(),
        };
        names.sort_unstable();
        Ok(names)
    }
}
