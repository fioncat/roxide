pub mod context;
pub mod hook;
pub mod remote;

use std::collections::HashMap;
use std::io;
use std::mem::take;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::debug;
use crate::exec::Cmd;
use crate::repo::ensure_dir;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Config {
    #[serde(default)]
    pub workspace: String,

    #[serde(default)]
    pub data_dir: String,

    #[serde(default)]
    pub mirrors_dir: String,

    #[serde(default = "Config::default_branch")]
    pub default_branch: String,

    #[serde(default = "Config::default_fzf")]
    pub fzf: CmdConfig,

    #[serde(default = "Config::default_git")]
    pub git: CmdConfig,

    #[serde(default = "Config::default_bash")]
    pub bash: CmdConfig,

    #[serde(default = "Config::default_edit")]
    pub edit: CmdConfig,

    #[serde(default)]
    pub stats_ignore: Vec<String>,

    #[serde(skip)]
    pub remotes: Vec<remote::RemoteConfig>,

    #[serde(skip)]
    remotes_index: Option<HashMap<String, usize>>,

    #[serde(skip)]
    pub hooks: hook::HooksConfig,

    #[serde(skip)]
    pub dir: PathBuf,

    #[serde(skip)]
    pub path: PathBuf,

    #[serde(skip)]
    pub remotes_dir: PathBuf,

    #[serde(skip)]
    pub hooks_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CmdConfig {
    pub name: String,

    #[serde(default)]
    pub args: Vec<String>,
}

impl Config {
    const REMOTES_INDEX_THRESHOLD: usize = 10;

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
        cfg.path = file_path;
        cfg.dir = path;
        cfg.remotes_dir = cfg.dir.join("remotes");
        cfg.hooks_dir = cfg.dir.join("hooks");

        let hooks = hook::HooksConfig::read(&cfg.hooks_dir)?;
        let remotes = remote::RemoteConfig::read(&cfg.remotes_dir, &hooks)?;

        cfg.hooks = hooks;
        cfg.remotes = remotes;
        if cfg.remotes.len() > Self::REMOTES_INDEX_THRESHOLD {
            let remotes_index = cfg
                .remotes
                .iter()
                .enumerate()
                .map(|(i, r)| (r.name.clone(), i))
                .collect();
            debug!(
                "[config] Remotes length exceeded threshold, build remotes index: {remotes_index:?}"
            );
            cfg.remotes_index = Some(remotes_index);
        }

        cfg.validate(&home_dir)
            .context("failed to validate base config")?;

        debug!("[config] Load config done");
        Ok(cfg)
    }

    pub fn contains_remote(&self, name: &str) -> bool {
        self.remotes.iter().any(|r| r.name == name)
    }

    pub fn get_remote(&self, name: &str) -> Result<&remote::RemoteConfig> {
        match self.get_rempte_optional(name) {
            Some(r) => Ok(r),
            None => bail!("remote {name:?} not found"),
        }
    }

    pub fn get_rempte_optional(&self, name: &str) -> Option<&remote::RemoteConfig> {
        match self.remotes_index {
            Some(ref remotes_index) => {
                let idx = remotes_index.get(name)?;
                Some(&self.remotes[*idx])
            }
            None => self.remotes.iter().find(|r| r.name == name),
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

        self.mirrors_dir = expandenv(take(&mut self.mirrors_dir));
        if self.mirrors_dir.is_empty() {
            if self.get_remote("mirrors").is_ok() {
                bail!(
                    "the mirrors dir needs to occupy the mirrors path under the workspace, but the path is currently occupied by a remote with the same name. You need to manually specify a mirrors path"
                );
            }
            let mirrors_dir = Path::new(&self.workspace).join("mirrors");
            self.mirrors_dir = format!("{}", mirrors_dir.display());
        }
        if Path::new(&self.mirrors_dir).is_relative() {
            bail!("mirrors_dir path {:?} must be absolute", self.mirrors_dir);
        }

        if self.default_branch.is_empty() {
            self.default_branch = Self::default_branch();
        }

        if self.fzf.name.is_empty() {
            bail!("fzf command name cannot be empty");
        }

        if self.git.name.is_empty() {
            bail!("git command name cannot be empty");
        }

        if self.bash.name.is_empty() {
            bail!("bash command name cannot be empty");
        }

        if self.edit.name.is_empty() {
            bail!("edit command name cannot be empty");
        }

        debug!("[config] Config validated: {:?}", self);
        Ok(())
    }

    fn default_branch() -> String {
        String::from("main")
    }

    pub fn default_fzf() -> CmdConfig {
        CmdConfig {
            name: "fzf".to_string(),
            args: vec![],
        }
    }

    pub fn default_git() -> CmdConfig {
        CmdConfig {
            name: "git".to_string(),
            args: vec![],
        }
    }

    pub fn default_bash() -> CmdConfig {
        CmdConfig {
            name: "bash".to_string(),
            args: vec![],
        }
    }

    pub fn default_edit() -> CmdConfig {
        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());
        CmdConfig {
            name: editor,
            args: vec![],
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            workspace: String::new(),
            data_dir: String::new(),
            mirrors_dir: String::new(),
            default_branch: Self::default_branch(),
            fzf: Self::default_fzf(),
            git: Self::default_git(),
            bash: Self::default_bash(),
            edit: Self::default_edit(),
            stats_ignore: vec![],
            remotes: vec![],
            remotes_index: None,
            hooks: hook::HooksConfig::default(),
            path: PathBuf::new(),
            dir: PathBuf::new(),
            remotes_dir: PathBuf::new(),
            hooks_dir: PathBuf::new(),
        }
    }
}

impl CmdConfig {
    pub fn new_cmd(&self) -> Cmd {
        Cmd::new(&self.name).args(&self.args)
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
        let cfg = Config::read(Some(path.clone())).unwrap();
        let expect = Config {
            workspace: format!("{}/dev", home_dir.display()),
            data_dir: format!("{}/.local/share/roxide", home_dir.display()),
            mirrors_dir: format!("{}/dev/mirrors", home_dir.display()),
            default_branch: "main".to_string(),
            fzf: Config::default_fzf(),
            git: Config::default_git(),
            bash: CmdConfig {
                name: "/bin/bash".to_string(),
                args: vec!["-e".to_string()],
            },
            edit: Config::default_edit(),
            stats_ignore: vec![],
            remotes: super::remote::tests::expect_remotes(),
            remotes_index: None,
            hooks: super::hook::tests::expect_hooks(),
            dir: path.clone(),
            path: path.join("config.toml"),
            remotes_dir: path.join("remotes"),
            hooks_dir: path.join("hooks"),
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
            ("$NONEXISTENT_ENV", "$NONEXISTENT_ENV".to_string()),
            ("${NONEXISTENT_ENV}", "${NONEXISTENT_ENV}".to_string()),
        ];

        for (input, expected) in test_cases {
            assert_eq!(expandenv(input.to_string()), expected);
        }
    }
}
