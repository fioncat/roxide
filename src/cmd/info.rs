use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{bail, Result};
use clap::Args;
use serde::Serialize;

use crate::cmd::Run;
use crate::config::Config;
use crate::repo::database::Database;
use crate::term::{self, Cmd};
use crate::utils;

/// Show some global info
#[derive(Args)]
pub struct InfoArgs {}

impl Run for InfoArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
        let db = Database::load(cfg)?;

        let git = Self::convert_component(Self::git_info());
        let fzf = Self::convert_component(Self::fzf_info());

        let config = Self::config(cfg)?;

        let repos = db.list_all(&None);

        let mut total_size: u64 = 0;
        let mut workspace_count: u32 = 0;
        let mut workspace_size: u64 = 0;
        let mut standalone_count: u32 = 0;
        let mut standalone_size: u64 = 0;
        let mut remotes: BTreeMap<String, RemoteInfo> = BTreeMap::new();

        let mut scan_file: u64 = 0;

        for repo in repos {
            let path = repo.get_path(cfg);
            let mut size: u64 = 0;
            utils::walk_dir(path, |_file, meta| {
                if meta.is_file() {
                    scan_file += 1;
                    size += meta.len();
                }
                Ok(true)
            })?;

            total_size += size;
            match repo.path.as_ref() {
                Some(_) => {
                    standalone_count += 1;
                    standalone_size += size;
                }
                None => {
                    workspace_count += 1;
                    workspace_size += size;
                }
            };

            let remote_info = match remotes.get_mut(repo.remote.as_ref()) {
                Some(remote_info) => remote_info,
                None => {
                    let remote = repo.remote.to_string();
                    let clone = match repo.remote_cfg.clone.as_ref() {
                        Some(_) => true,
                        None => false,
                    };
                    let info = RemoteInfo {
                        clone,
                        size: None,
                        repo_count: 0,
                        owner_count: 0,
                        owners: BTreeMap::new(),
                        size_u64: 0,
                    };
                    remotes.insert(remote.clone(), info);
                    remotes.get_mut(&remote).unwrap()
                }
            };
            remote_info.size_u64 += size;
            remote_info.repo_count += 1;

            let owner_info = match remote_info.owners.get_mut(repo.owner.as_ref()) {
                Some(owner_info) => owner_info,
                None => {
                    let info = OwnerInfo {
                        repo_count: 0,
                        size: None,
                        size_u64: 0,
                    };

                    remote_info.owners.insert(repo.owner.to_string(), info);
                    remote_info.owners.get_mut(repo.owner.as_ref()).unwrap()
                }
            };
            owner_info.repo_count += 1;
            owner_info.size_u64 += size;
        }

        for (_, remote) in remotes.iter_mut() {
            remote.size = Some(utils::human_bytes(remote.size_u64));
            remote.owner_count = remote.owners.len() as u32;
            for (_, owner) in remote.owners.iter_mut() {
                owner.size = Some(utils::human_bytes(owner.size_u64));
            }
        }

        let info = Info {
            git,
            fzf,
            total_repo_size: utils::human_bytes(total_size),
            files_count: scan_file,
            workspace: RepoInfo {
                count: workspace_count,
                size: utils::human_bytes(workspace_size),
            },
            standalone: RepoInfo {
                count: standalone_count,
                size: utils::human_bytes(standalone_size),
            },
            config,
            remote_count: remotes.len() as u32,
            remotes,
        };

        term::show_json(info)
    }
}

impl InfoArgs {
    fn git_info() -> Result<ComponentInfo> {
        let path = Self::get_component_path("git")?;
        let version = Cmd::git(&["version"]).read()?;
        let version = match version.trim().strip_prefix("git version") {
            Some(v) => v.trim(),
            None => version.as_str(),
        };
        Ok(ComponentInfo {
            enable: true,
            path,
            version: String::from(version),
        })
    }

    fn fzf_info() -> Result<ComponentInfo> {
        let path = Self::get_component_path("fzf")?;
        let version = Cmd::with_args("fzf", &["--version"]).read()?;
        Ok(ComponentInfo {
            enable: true,
            path,
            version,
        })
    }

    fn get_component_path(cmd: &str) -> Result<String> {
        let path = Cmd::with_args("which", &[cmd]).read()?;
        let path = path.trim();
        if path.is_empty() {
            bail!("'{cmd}' path is empty");
        }
        Ok(String::from(path))
    }

    fn convert_component(r: Result<ComponentInfo>) -> ComponentInfo {
        r.unwrap_or_else(|_| ComponentInfo {
            enable: false,
            path: String::new(),
            version: String::new(),
        })
    }

    fn config(cfg: &Config) -> Result<ConfigInfo> {
        let path = match Config::get_path()? {
            Some(path) => format!("{}", path.display()),
            None => "N/A".to_string(),
        };
        let meta_dir = format!("{}", cfg.get_meta_dir().display());
        let meta_size = utils::dir_size(PathBuf::from(&meta_dir))?;

        let db_path = cfg.get_meta_dir().join("database");
        let db_meta = fs::metadata(&db_path)?;
        let db_size = utils::human_bytes(db_meta.len());

        Ok(ConfigInfo {
            path,
            meta_path: meta_dir,
            meta_size: utils::human_bytes(meta_size),
            database_size: db_size,
        })
    }
}

#[derive(Debug, Serialize)]
struct Info {
    pub git: ComponentInfo,
    pub fzf: ComponentInfo,
    pub total_repo_size: String,
    pub files_count: u64,
    pub workspace: RepoInfo,
    pub standalone: RepoInfo,
    pub config: ConfigInfo,
    pub remote_count: u32,
    pub remotes: BTreeMap<String, RemoteInfo>,
}

#[derive(Debug, Serialize)]
struct ComponentInfo {
    pub enable: bool,
    pub path: String,
    pub version: String,
}

#[derive(Debug, Serialize)]
struct RepoInfo {
    pub count: u32,
    pub size: String,
}

#[derive(Debug, Serialize)]
struct ConfigInfo {
    pub path: String,
    pub meta_path: String,
    pub meta_size: String,
    pub database_size: String,
}

#[derive(Debug, Serialize)]
struct RemoteInfo {
    pub clone: bool,
    pub size: Option<String>,
    pub repo_count: u32,
    pub owner_count: u32,
    pub owners: BTreeMap<String, OwnerInfo>,

    #[serde(skip)]
    size_u64: u64,
}

#[derive(Debug, Serialize)]
struct OwnerInfo {
    pub repo_count: u32,
    pub size: Option<String>,

    #[serde(skip)]
    size_u64: u64,
}
