use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{bail, Result};
use clap::Args;
use serde::Serialize;

use crate::cmd::Run;
use crate::repo::database::Database;
use crate::shell::Shell;
use crate::{config, utils};

/// Show some global info
#[derive(Args)]
pub struct InfoArgs {}

impl Run for InfoArgs {
    fn run(&self) -> Result<()> {
        let db = Database::read()?;

        let git = Self::convert_component(Self::git_info());
        let fzf = Self::convert_component(Self::fzf_info());

        let repos = db.list_all();

        let config = Self::config()?;

        let mut total_size: u64 = 0;
        let mut workspace_count: u32 = 0;
        let mut workspace_size: u64 = 0;
        let mut standalone_count: u32 = 0;
        let mut standalone_size: u64 = 0;
        let mut remotes: BTreeMap<String, RemoteInfo> = BTreeMap::new();

        for repo in repos {
            let size = utils::dir_size(repo.get_path())?;
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

            let remote_info = match remotes.get_mut(repo.remote.as_str()) {
                Some(remote_info) => remote_info,
                None => {
                    let remote = config::must_get_remote(repo.remote.as_str())?;
                    let clone = match remote.clone.as_ref() {
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
                    remotes.insert(String::from(repo.remote.as_str()), info);
                    remotes.get_mut(repo.remote.as_str()).unwrap()
                }
            };
            remote_info.size_u64 += size;
            remote_info.repo_count += 1;

            let owner_info = match remote_info.owners.get_mut(repo.owner.as_str()) {
                Some(owner_info) => owner_info,
                None => {
                    let info = OwnerInfo {
                        repo_count: 0,
                        size: None,
                        size_u64: 0,
                    };

                    remote_info
                        .owners
                        .insert(String::from(repo.owner.as_str()), info);
                    remote_info.owners.get_mut(repo.owner.as_str()).unwrap()
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

        utils::show_json(info)?;
        Ok(())
    }
}

impl InfoArgs {
    fn git_info() -> Result<ComponentInfo> {
        let path = Self::get_component_path("git")?;
        let version = Shell::exec_git_mute_read(&["version"])?;
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
        let version = Shell::with_args("fzf", &["--version"])
            .set_mute(true)
            .piped_stderr()
            .execute()?
            .read()?;
        Ok(ComponentInfo {
            enable: true,
            path,
            version,
        })
    }

    fn get_component_path(cmd: &str) -> Result<String> {
        let path = Shell::with_args("which", &[cmd])
            .set_mute(true)
            .piped_stderr()
            .execute()?
            .read()?;
        let path = path.trim();
        if path.is_empty() {
            bail!("{cmd} path is empty");
        }
        Ok(String::from(path))
    }

    fn convert_component(r: Result<ComponentInfo>) -> ComponentInfo {
        match r {
            Ok(c) => c,
            Err(_) => ComponentInfo {
                enable: false,
                path: String::new(),
                version: String::new(),
            },
        }
    }

    fn config() -> Result<ConfigInfo> {
        let cfg = config::get();
        let config_dir = format!("{}", cfg.dir.display());
        let meta_dir = format!("{}", cfg.base.metadir);
        let meta_size = utils::dir_size(PathBuf::from(&meta_dir))?;

        let db_path = PathBuf::from(&config::base().metadir).join("database");
        let db_meta = fs::metadata(&db_path)?;
        let db_size = utils::human_bytes(db_meta.len());

        Ok(ConfigInfo {
            path: config_dir,
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
