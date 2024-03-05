use std::borrow::Cow;
use std::fmt::Display;
use std::path::PathBuf;
use std::{fs, io};

use anyhow::{bail, Context, Result};
use clap::Args;
use console::style;
use serde::Serialize;

use crate::api::{self, ProviderInfo};
use crate::cmd::Run;
use crate::config::{Config, RemoteConfig};
use crate::repo::database::Database;
use crate::term::{self, Cmd};
use crate::utils;

/// Show some global info
#[derive(Args)]
pub struct InfoArgs {
    /// Show the output in json format.
    #[clap(short, long)]
    pub json: bool,
}

impl Run for InfoArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
        let db = Database::load(cfg)?;
        let info = Info::build(cfg, &db)?;

        if self.json {
            return term::show_json(info);
        }

        eprint!("{info}");

        Ok(())
    }
}

#[derive(Debug, Serialize)]
struct Info {
    config: ConfigInfo,
    commands: Vec<CommandInfo>,
    standalone: Vec<StandaloneInfo>,

    meta_disk_usage: MetaDiskUsage,
    repo_disk_usage: Vec<RemoteDiskUsage>,

    remote_api: Vec<RemoteApiInfo>,
}

impl Info {
    fn build(cfg: &Config, db: &Database) -> Result<Self> {
        let config = ConfigInfo::build(cfg)?;
        let commands = vec![CommandInfo::git()?, CommandInfo::fzf()?];
        let standalone = StandaloneInfo::list(db);

        let meta_disk_usage = MetaDiskUsage::build(cfg)?;

        let mut remote_api = Vec::with_capacity(cfg.remotes.len());
        for remote_cfg in cfg.remotes.values() {
            let api_info = RemoteApiInfo::build(remote_cfg)?;
            remote_api.push(api_info);
        }
        remote_api.sort_unstable_by(|a, b| a.name.cmp(&b.name));

        let repo_disk_usage = RemoteDiskUsage::scan(cfg, db)?;

        Ok(Info {
            config,
            commands,
            standalone,
            meta_disk_usage,
            repo_disk_usage,
            remote_api,
        })
    }

    fn show_title(f: &mut std::fmt::Formatter<'_>, title: &str, name: &str) -> std::fmt::Result {
        let start = style(format!("[{title} ")).blue().bold();
        let name = style(name).magenta().bold();
        let end = style("]").blue().bold();
        writeln!(f, "{start}{name}{end}")
    }
}

impl Display for Info {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}", style("[Config]").blue().bold())?;
        writeln!(f, "\tpath: {}", self.config.path)?;
        writeln!(f, "\tmeta: {}", self.config.meta_path)?;
        writeln!(f, "\tis_default: {}", self.config.is_default)?;
        writeln!(f, "\tremotes: {}", self.config.remotes)?;
        writeln!(f, "\tworkflows: {}", self.config.workflows)?;
        writeln!(f)?;

        if !self.commands.is_empty() {
            writeln!(f, "{}", style("[Commands]").blue().bold())?;
            for cmd in self.commands.iter() {
                writeln!(f, "\t{cmd}")?;
            }
            writeln!(f)?;
        }

        if !self.standalone.is_empty() {
            writeln!(f, "{}", style("[Standalone]").blue().bold())?;
            for standalone in self.standalone.iter() {
                writeln!(f, "\t{standalone}")?;
            }
            writeln!(f)?;
        }

        writeln!(f, "{}", style("[Meta Disk Usage]").blue().bold())?;
        let meta_database = utils::human_bytes(self.meta_disk_usage.database);
        writeln!(f, "\tTotal: {}", self.meta_disk_usage.total)?;
        writeln!(f, "\tDatabase: {meta_database}")?;
        writeln!(f)?;

        if !self.repo_disk_usage.is_empty() {
            for remote_usage in self.repo_disk_usage.iter() {
                Self::show_title(f, "Disk Usage", &remote_usage.name)?;
                writeln!(f, "\tTotal: {}", remote_usage.usage)?;
                writeln!(f)?;

                for owner_usage in remote_usage.owners.iter() {
                    let owner_name = format!("{}/{}", remote_usage.name, owner_usage.name);
                    Self::show_title(f, "Disk Usage", &owner_name)?;
                    writeln!(f, "\tTotal: {}", owner_usage.usage)?;
                    for repo_usage in owner_usage.repos.iter() {
                        writeln!(f, "\t{}: {}", repo_usage.name, repo_usage.usage)?;
                    }
                    writeln!(f)?;
                }
            }
        }

        if !self.remote_api.is_empty() {
            writeln!(f, "{}", style("[Remote API]").blue().bold())?;
            for api in self.remote_api.iter() {
                let provider = api
                    .provider
                    .as_ref()
                    .map(|p| Cow::Owned(format!("{p}")))
                    .unwrap_or(Cow::Borrowed("<none>"));
                writeln!(f, "\t{}: {provider}", api.name)?;
            }
            writeln!(f)?;
        }

        Ok(())
    }
}

#[derive(Debug, Serialize)]
struct ConfigInfo {
    path: String,
    meta_path: String,

    is_default: bool,

    remotes: usize,
    workflows: usize,
}

impl ConfigInfo {
    fn build(cfg: &Config) -> Result<Self> {
        let path = Config::get_path()?;
        let path = format!("{}", path.display());
        let meta_path = format!("{}", cfg.get_meta_dir().display());
        Ok(ConfigInfo {
            path,
            meta_path,
            is_default: cfg.is_default,
            remotes: cfg.remotes.len(),
            workflows: cfg.workflows.len(),
        })
    }
}

#[derive(Debug, Serialize)]
struct CommandInfo {
    name: &'static str,
    path: String,
    version: String,
}

impl CommandInfo {
    fn git() -> Result<Self> {
        let path = Self::get_command_path("git")?;
        let version = Cmd::git(&["version"]).read()?;
        let version = version
            .trim()
            .strip_prefix("git version")
            .map(|s| s.trim().to_string())
            .unwrap_or(version);
        Ok(CommandInfo {
            name: "git",
            path,
            version,
        })
    }

    fn fzf() -> Result<Self> {
        let path = Self::get_command_path("fzf")?;
        let version = Cmd::with_args("fzf", &["--version"]).read()?;
        Ok(CommandInfo {
            name: "fzf",
            path,
            version,
        })
    }

    fn get_command_path(cmd: &str) -> Result<String> {
        let path = Cmd::with_args("which", &[cmd]).read()?;
        let path = path.trim();
        if path.is_empty() {
            bail!("'{cmd}' path is empty");
        }
        Ok(String::from(path))
    }
}

impl Display for CommandInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}, {}", self.name, self.path, self.version)
    }
}

#[derive(Debug, Serialize)]
struct StandaloneInfo {
    name: String,
    path: String,
}

impl StandaloneInfo {
    fn list(db: &Database) -> Vec<Self> {
        let repos = db.list_all(&None);
        repos
            .into_iter()
            .filter(|repo| repo.path.is_some())
            .map(|repo| StandaloneInfo {
                name: repo.name_with_remote(),
                path: repo.path.unwrap().to_string(),
            })
            .collect()
    }
}

impl Display for StandaloneInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} -> {}", self.name, self.path)
    }
}

#[derive(Debug, Serialize)]
struct DirectoryUsage {
    files: u64,
    dirs: u64,
    size: u64,
}

impl DirectoryUsage {
    fn scan(dir: PathBuf) -> Result<Self> {
        let mut files: u64 = 0;
        let mut dirs: u64 = 0;
        let mut size: u64 = 0;
        utils::walk_dir(dir, |_, meta| {
            if meta.is_dir() {
                dirs += 1;
            }
            if meta.is_file() {
                files += 1;
            }
            size += meta.len();

            Ok(true)
        })?;

        Ok(DirectoryUsage { files, dirs, size })
    }
}

impl Display for DirectoryUsage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let files_word = if self.files > 1 { "files" } else { "file" };
        let dirs_word = if self.dirs > 1 { "dirs" } else { "dir" };
        let size = utils::human_bytes(self.size);
        write!(
            f,
            "{} {files_word}, {} {dirs_word}, {size}",
            self.files, self.dirs
        )
    }
}

#[derive(Debug, Serialize)]
struct MetaDiskUsage {
    total: DirectoryUsage,
    database: u64,
}

impl MetaDiskUsage {
    fn build(cfg: &Config) -> Result<Self> {
        let meta_dir = cfg.get_meta_dir();
        let database_path = meta_dir.join("database");

        let total = DirectoryUsage::scan(meta_dir.clone())?;
        let database_size = match fs::metadata(database_path) {
            Ok(meta) => meta.len(),
            Err(err) if err.kind() == io::ErrorKind::NotFound => 0,
            Err(err) => return Err(err).context("get metadata for database"),
        };

        Ok(MetaDiskUsage {
            total,
            database: database_size,
        })
    }
}

#[derive(Debug, Serialize)]
struct RemoteDiskUsage {
    name: String,
    usage: DirectoryUsage,
    owners: Vec<OwnerDiskUsage>,
}

#[derive(Debug, Serialize)]
struct OwnerDiskUsage {
    name: String,
    usage: DirectoryUsage,
    repos: Vec<RepoDiskUsage>,
}

#[derive(Debug, Serialize)]
struct RepoDiskUsage {
    name: String,
    usage: DirectoryUsage,
}

impl RemoteDiskUsage {
    fn scan(cfg: &Config, db: &Database) -> Result<Vec<Self>> {
        let mut remotes: Vec<RemoteDiskUsage> = Vec::with_capacity(cfg.remotes.len());
        for remote_name in cfg.remotes.keys() {
            let owner_names = db.list_owners(remote_name);

            let mut remote = RemoteDiskUsage {
                name: remote_name.to_string(),
                usage: DirectoryUsage {
                    files: 0,
                    dirs: 0,
                    size: 0,
                },
                owners: Vec::with_capacity(owner_names.len()),
            };

            for owner_name in owner_names {
                let repos = db.list_by_owner(remote_name, &owner_name, &None);
                let mut owner = OwnerDiskUsage {
                    name: owner_name.clone(),
                    usage: DirectoryUsage {
                        files: 0,
                        dirs: 0,
                        size: 0,
                    },
                    repos: Vec::with_capacity(repos.len()),
                };

                for repo in repos {
                    let path = repo.get_path(cfg);
                    let usage = DirectoryUsage::scan(path).with_context(|| {
                        format!("scan disk usage for {}", repo.name_with_remote())
                    })?;

                    owner.usage.files += usage.files;
                    owner.usage.dirs += usage.dirs;
                    owner.usage.size += usage.size;
                    owner.repos.push(RepoDiskUsage {
                        name: repo.name.into_owned(),
                        usage,
                    });
                }

                remote.usage.files += owner.usage.files;
                remote.usage.dirs += owner.usage.dirs;
                remote.usage.size += owner.usage.size;
                remote.owners.push(owner);
            }

            remotes.push(remote);
        }

        remotes.sort_unstable_by(|a, b| a.name.cmp(&b.name));
        Ok(remotes)
    }
}

#[derive(Debug, Serialize)]
struct RemoteApiInfo {
    name: String,
    provider: Option<ProviderInfo>,
}

impl RemoteApiInfo {
    fn build(remote_cfg: &RemoteConfig) -> Result<Self> {
        let name = remote_cfg.get_name().to_string();
        let mut provider_info: Option<ProviderInfo> = None;
        if remote_cfg.provider.is_some() {
            let provider = api::build_raw_provider(remote_cfg);
            provider_info = Some(provider.info()?);
        }
        Ok(RemoteApiInfo {
            name,
            provider: provider_info,
        })
    }
}
