pub mod context;
pub mod remote;
pub mod script;

use std::io;
use std::mem::take;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::debug;
use crate::exec::bash;
use crate::exec::fzf;
use crate::exec::git;
use crate::repo::ensure_dir;
use crate::term::output;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct Config {
    #[serde(default)]
    pub workspace: String,

    #[serde(default)]
    pub data_dir: String,

    #[serde(default)]
    pub debug: bool,

    pub fzf: Option<CmdConfig>,

    pub git: Option<CmdConfig>,

    pub bash: Option<CmdConfig>,

    #[serde(skip)]
    pub remotes: Vec<remote::RemoteConfig>,

    #[serde(skip)]
    pub scripts: script::ScriptsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CmdConfig {
    pub name: String,

    #[serde(default)]
    pub args: Vec<String>,
}

impl Config {
    pub fn read<P>(path: Option<P>) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        let Some(home_dir) = dirs::home_dir() else {
            bail!("Cannot determine home directory");
        };

        let path = path
            .map(|p| PathBuf::from(p.as_ref()))
            .unwrap_or(home_dir.join(".config").join("roxide"));
        debug!(
            "[config] Read config from {}, home_dir: {}",
            path.display(),
            home_dir.display()
        );

        let file_path = path.join("config.toml");
        let mut cfg = match std::fs::read_to_string(&file_path) {
            Ok(data) => {
                debug!("[config] Base config file found, parse it");
                toml::from_str(&data).with_context(|| {
                    format!("failed to parse config file {}", file_path.display())
                })?
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                debug!("[config] Base config file not found, use default");
                Self::default()
            }
            Err(e) => {
                return Err(e).with_context(|| {
                    format!("failed to read config file {}", file_path.display())
                });
            }
        };
        cfg.validate(&home_dir)
            .context("failed to validate base config")?;

        let scripts = script::ScriptsConfig::read(&path.join("scripts"))?;
        let remotes = remote::RemoteConfig::read(&path.join("remotes"), &scripts)?;

        cfg.scripts = scripts;
        cfg.remotes = remotes;

        debug!("[config] Load config done");
        Ok(cfg)
    }

    pub fn get_remote(&self, name: &str) -> Result<&remote::RemoteConfig> {
        match self.remotes.iter().find(|r| r.name == name) {
            Some(r) => Ok(r),
            None => bail!("remote '{}' not found", name),
        }
    }

    fn validate(&mut self, home_dir: &Path) -> Result<()> {
        debug!("[config] Validate config: {:?}", self);

        let workspace = expandenv(take(&mut self.workspace));
        if workspace.is_empty() {
            let workspace = home_dir.join("dev");
            self.workspace = format!("{}", workspace.display());
        }
        if !Path::new(&self.workspace).is_absolute() {
            bail!("workspace path must be absolute");
        }

        let data_dir = expandenv(take(&mut self.data_dir));
        if data_dir.is_empty() {
            let data_dir = home_dir.join(".local").join("share").join("roxide");
            self.data_dir = format!("{}", data_dir.display());
        }
        if !Path::new(&self.data_dir).is_absolute() {
            bail!("data_dir path must be absolute");
        }
        ensure_dir(&self.data_dir)?;

        if self.debug {
            let debug_path = PathBuf::from(&self.data_dir).join("debug.log");
            output::set_debug(format!("{}", debug_path.display()));
        }

        if let Some(mut fzf) = self.fzf.take() {
            fzf.validate().context("failed to validate fzf config")?;
            fzf::set_cmd(fzf);
        }

        if let Some(mut git) = self.git.take() {
            git.validate().context("failed to validate git config")?;
            git::set_cmd(git);
        }

        if let Some(mut bash) = self.bash.take() {
            bash.validate().context("failed to validate bash config")?;
            bash::set_cmd(bash);
        }

        debug!("[config] Config validated: {:?}", self);
        Ok(())
    }
}

impl CmdConfig {
    fn validate(&mut self) -> Result<()> {
        self.name = expandenv(take(&mut self.name));
        if self.name.is_empty() {
            bail!("command name cannot be empty");
        }

        Ok(())
    }
}

#[inline]
pub fn expandenv(s: String) -> String {
    match shellexpand::full(s.as_str()) {
        Ok(s) => s.to_string(),
        Err(_) => s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config() {
        let home_dir = dirs::home_dir().unwrap();
        let cfg = Config::read(Some("src/config/tests")).unwrap();
        let expect = Config {
            workspace: format!("{}/dev", home_dir.display()),
            data_dir: format!("{}/.local/share/roxide", home_dir.display()),
            debug: false,
            fzf: None,
            git: None,
            bash: None,
            remotes: super::remote::tests::expect_remotes(),
            scripts: super::script::tests::expect_scripts(),
        };
        assert_eq!(cfg, expect);
        assert_eq!(
            bash::get_cmd(),
            &CmdConfig {
                name: "/bin/bash".to_string(),
                args: vec!["-e".to_string()],
            }
        );
        assert_eq!(
            git::get_cmd(),
            &CmdConfig {
                name: "/usr/bin/git".to_string(),
                args: vec![],
            }
        );
    }

    #[test]
    fn test_expandenv() {
        let home = std::env::var("HOME").unwrap();

        let test_cases = [
            ("~/Desktop/", format!("{home}/Desktop/")),
            ("$HOME/Desktop/", format!("{home}/Desktop/")),
            ("${HOME}/Desktop/", format!("{home}/Desktop/")),
            ("NoEnvHere", "NoEnvHere".to_string()),
            ("$NOEXISTENT_ENV", "$NOEXISTENT_ENV".to_string()),
            ("${NOEXISTENT_ENV}", "${NOEXISTENT_ENV}".to_string()),
        ];

        for (input, expected) in test_cases {
            assert_eq!(expandenv(input.to_string()), expected);
        }
    }
}
