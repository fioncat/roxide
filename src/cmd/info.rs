use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt::Display;
use std::fs;
use std::path::PathBuf;

use anyhow::{bail, Result};
use clap::Args;
use serde::Serialize;

use crate::api::ProviderInfo;
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
        todo!()
    }
}

impl InfoArgs {}

struct Info {
    config: ConfigInfo,
    commands: Vec<CommandInfo>,
    standalone: Vec<StandaloneInfo>,

    meta_disk_usage: MetaDiskUsage,
    repo_disk_usage: Vec<RemoteDiskUsage>,

    remote_api: Vec<RemoteApiInfo>,
}

impl Display for Info {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Config:")?;
        writeln!(f, "\tpath: {}", self.config.path)?;
        writeln!(f, "\tmeta: {}", self.config.meta_path)?;
        writeln!(f)?;

        if !self.commands.is_empty() {
            writeln!(f, "Commands:")?;
            for cmd in self.commands.iter() {
                writeln!(f, "\t{cmd}")?;
            }
            writeln!(f)?;
        }

        if !self.standalone.is_empty() {
            writeln!(f, "Standalone:")?;
            for standalone in self.standalone.iter() {
                writeln!(f, "\t{standalone}")?;
            }
            writeln!(f)?;
        }

        writeln!(f, "Meta Disk Usage:")?;
        let meta_database = utils::human_bytes(self.meta_disk_usage.database);
        writeln!(f, "\tTotal:    {}", self.meta_disk_usage.total)?;
        writeln!(f, "\tDatabase: {meta_database}")?;
        writeln!(f)?;

        if !self.repo_disk_usage.is_empty() {
            writeln!(f, "Repository Disk Usage:")?;
            for remote_usage in self.repo_disk_usage.iter() {
                writeln!(f, "\t{}: {}", remote_usage.name, remote_usage.usage)?;
                for owner_usage in remote_usage.owners.iter() {
                    writeln!(f, "\t\t{}: {}", owner_usage.name, owner_usage.usage)?;
                    for repo_usage in owner_usage.repos.iter() {
                        writeln!(f, "\t\t\t{}: {}", repo_usage.name, repo_usage.usage)?;
                    }
                }
            }
            writeln!(f)?;
        }

        if !self.remote_api.is_empty() {
            writeln!(f, "Remote API:")?;
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

struct ConfigInfo {
    path: String,
    meta_path: String,
}

struct CommandInfo {
    name: String,
    path: String,
    version: String,
}

impl Display for CommandInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}, {}", self.name, self.path, self.version)
    }
}

struct StandaloneInfo {
    name: String,
    path: String,
}

impl Display for StandaloneInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} -> {}", self.name, self.path)
    }
}

struct DirectoryUsage {
    files: u64,
    dirs: u64,
    size: u64,
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

struct MetaDiskUsage {
    total: DirectoryUsage,
    database: u64,
}

struct RemoteDiskUsage {
    name: String,
    usage: DirectoryUsage,
    owners: Vec<OwnerDiskUsage>,
}

struct OwnerDiskUsage {
    name: String,
    usage: DirectoryUsage,
    repos: Vec<RepoDiskUsage>,
}

struct RepoDiskUsage {
    name: String,
    usage: DirectoryUsage,
}

struct RemoteApiInfo {
    name: String,
    provider: Option<ProviderInfo>,
}
