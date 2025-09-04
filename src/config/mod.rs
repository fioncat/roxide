pub mod context;
pub mod hook;
pub mod remote;

use std::io;
use std::mem::take;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::debug;
use crate::repo::ensure_dir;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct Config {
    #[serde(default)]
    pub workspace: String,

    #[serde(default)]
    pub data_dir: String,

    #[serde(default = "Config::default_branch")]
    pub default_branch: String,

    pub fzf: Option<CmdConfig>,

    pub git: Option<CmdConfig>,

    pub bash: Option<CmdConfig>,

    #[serde(skip)]
    pub remotes: Vec<remote::RemoteConfig>,

    #[serde(skip)]
    pub hooks: hook::HooksConfig,
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
                    format!("failed to parse config file {:?}", file_path.display())
                })?
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                debug!("[config] Base config file not found, use default");
                Self::default()
            }
            Err(e) => {
                return Err(e).with_context(|| {
                    format!("failed to read config file {:?}", file_path.display())
                });
            }
        };
        cfg.validate(&home_dir)
            .context("failed to validate base config")?;

        let hooks = hook::HooksConfig::read(&path.join("hooks"))?;
        let remotes = remote::RemoteConfig::read(&path.join("remotes"), &hooks)?;

        cfg.hooks = hooks;
        cfg.remotes = remotes;

        debug!("[config] Load config done");
        Ok(cfg)
    }

    pub fn contains_remote(&self, name: &str) -> bool {
        self.remotes.iter().any(|r| r.name == name)
    }

    pub fn get_remote(&self, name: &str) -> Result<&remote::RemoteConfig> {
        match self.remotes.iter().find(|r| r.name == name) {
            Some(r) => Ok(r),
            None => bail!("remote {name:?} not found"),
        }
    }

    fn validate(&mut self, home_dir: &Path) -> Result<()> {
        debug!("[config] Validate config: {:?}", self);

        self.workspace = expandenv(take(&mut self.workspace));
        if self.workspace.is_empty() {
            let workspace = home_dir.join("dev");
            self.workspace = format!("{}", workspace.display());
        }
        if Path::new(&self.workspace).is_relative() {
            bail!("workspace path {:?} must be absolute", self.workspace);
        }

        self.data_dir = expandenv(take(&mut self.data_dir));
        if self.data_dir.is_empty() {
            let data_dir = home_dir.join(".local").join("share").join("roxide");
            self.data_dir = format!("{}", data_dir.display());
        }
        if Path::new(&self.data_dir).is_relative() {
            bail!("data_dir path {:?} must be absolute", self.data_dir);
        }
        ensure_dir(&self.data_dir)?;

        if self.default_branch.is_empty() {
            self.default_branch = Self::default_branch();
        }

        if let Some(ref mut fzf) = self.fzf {
            fzf.validate().context("failed to validate fzf config")?;
        }

        if let Some(ref mut git) = self.git {
            git.validate().context("failed to validate git config")?;
        }

        if let Some(ref mut bash) = self.bash {
            bash.validate().context("failed to validate bash config")?;
        }

        debug!("[config] Config validated: {:?}", self);
        Ok(())
    }

    fn default_branch() -> String {
        String::from("main")
    }
}

impl CmdConfig {
    pub fn from_path<P: AsRef<Path>>(path: P) -> Self {
        Self {
            name: format!("{}", path.as_ref().display()),
            args: vec![],
        }
    }

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
    use std::fs;

    use super::*;

    #[test]
    fn test_config() {
        let home_dir = dirs::home_dir().unwrap();
        let path = fs::canonicalize("src/config/tests").unwrap();
        let cfg = Config::read(Some(path)).unwrap();
        let expect = Config {
            workspace: format!("{}/dev", home_dir.display()),
            data_dir: format!("{}/.local/share/roxide", home_dir.display()),
            default_branch: "main".to_string(),
            fzf: None,
            git: None,
            bash: Some(CmdConfig {
                name: "/bin/bash".to_string(),
                args: vec!["-e".to_string()],
            }),
            remotes: super::remote::tests::expect_remotes(),
            hooks: super::hook::tests::expect_hooks(),
        };
        assert_eq!(cfg, expect);
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
