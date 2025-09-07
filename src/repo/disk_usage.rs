use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use serde::Serialize;

use crate::config::context::ConfigContext;
use crate::db::repo::{DisplayLevel, OwnerState, RemoteState, Repository};
use crate::format::format_bytes;
use crate::scan::{ScanHandler, ScanTask, scan_files_with_data};
use crate::term::list::{List, ListItem};

pub async fn repo_disk_usage(
    ctx: Arc<ConfigContext>,
    repos: Vec<Repository>,
) -> Result<Vec<RepoDiskUsage>> {
    let mut tasks = Vec::with_capacity(repos.len());
    let mut usages = HashMap::with_capacity(repos.len());

    for repo in repos {
        let name = Arc::new(repo.full_name());
        let path = repo.get_path(&ctx.cfg.workspace);
        let task = ScanTask::new(path, name.clone())?;
        tasks.push(task);
        usages.insert(name, RepoDiskUsage { repo, usage: 0 });
    }

    let handler = RepoDiskUsageHandler {
        usages: Mutex::new(usages),
    };
    let handler = scan_files_with_data(tasks, handler, true).await?;
    let usages = handler.usages.into_inner().unwrap();
    let mut repos: Vec<_> = usages.into_values().collect();
    repos.sort_unstable_by(|a, b| b.usage.cmp(&a.usage));
    Ok(repos)
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct RepoDiskUsage {
    pub repo: Repository,
    pub usage: u64,
}

impl ListItem for RepoDiskUsage {
    fn row<'a>(&'a self, title: &str) -> Cow<'a, str> {
        if title == "DiskUsage" {
            return format_bytes(self.usage).into();
        }
        self.repo.row(title)
    }
}

pub struct RepoDiskUsageList {
    pub usages: Vec<RepoDiskUsage>,
    pub total: u32,
    pub level: DisplayLevel,
}

impl List<RepoDiskUsage> for RepoDiskUsageList {
    fn titles(&self) -> Vec<&'static str> {
        let mut titles = self.level.titles();
        titles.push("DiskUsage");
        titles
    }

    fn total(&self) -> u32 {
        self.total
    }

    fn items(&self) -> &[RepoDiskUsage] {
        &self.usages
    }
}

#[derive(Debug)]
struct RepoDiskUsageHandler {
    usages: Mutex<HashMap<Arc<String>, RepoDiskUsage>>,
}

impl ScanHandler<Arc<String>> for RepoDiskUsageHandler {
    fn handle_files(&self, files: Vec<(PathBuf, Metadata)>, name: Arc<String>) -> Result<()> {
        let usage: u64 = files.iter().map(|(_, metadata)| metadata.len()).sum();
        let mut usages = self.usages.lock().unwrap();
        let Some(repo_usage) = usages.get_mut(name.as_ref()) else {
            return Ok(());
        };
        repo_usage.usage += usage;
        Ok(())
    }

    fn should_skip(&self, _dir: &Path, _name: Arc<String>) -> Result<bool> {
        Ok(false)
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct RemoteDiskUsage {
    pub remote: RemoteState,
    pub usage: u64,
}

impl ListItem for RemoteDiskUsage {
    fn row<'a>(&'a self, title: &str) -> Cow<'a, str> {
        if title == "DiskUsage" {
            return format_bytes(self.usage).into();
        }
        self.remote.row(title)
    }
}

impl RemoteDiskUsage {
    pub fn group_by_repo_usages(repos: Vec<RepoDiskUsage>) -> Vec<Self> {
        let mut remote_usages: HashMap<&String, u64> = HashMap::new();
        let mut remote_repo_counts: HashMap<&String, u32> = HashMap::new();
        let mut remote_owner_counts: HashMap<&String, HashSet<&String>> = HashMap::new();

        for repo_usage in &repos {
            let remote = &repo_usage.repo.remote;
            let owner = &repo_usage.repo.owner;
            *remote_usages.entry(remote).or_insert(0) += repo_usage.usage;
            *remote_repo_counts.entry(remote).or_insert(0) += 1;
            remote_owner_counts.entry(remote).or_default().insert(owner);
        }

        let mut result: Vec<Self> = remote_usages
            .into_iter()
            .map(|(remote, usage)| RemoteDiskUsage {
                remote: RemoteState {
                    remote: remote.to_string(),
                    owner_count: remote_owner_counts[remote].len() as u32,
                    repo_count: remote_repo_counts[remote],
                },
                usage,
            })
            .collect();

        result.sort_unstable_by(|a, b| b.usage.cmp(&a.usage));
        result
    }
}

pub struct RemoteDiskUsageList {
    pub usages: Vec<RemoteDiskUsage>,
    pub total: u32,
}

impl List<RemoteDiskUsage> for RemoteDiskUsageList {
    fn titles(&self) -> Vec<&'static str> {
        vec!["Remote", "OwnerCount", "RepoCount", "DiskUsage"]
    }

    fn total(&self) -> u32 {
        self.total
    }

    fn items(&self) -> &[RemoteDiskUsage] {
        &self.usages
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct OwnerDiskUsage {
    pub owner: OwnerState,
    pub usage: u64,
}

impl ListItem for OwnerDiskUsage {
    fn row<'a>(&'a self, title: &str) -> Cow<'a, str> {
        if title == "DiskUsage" {
            return format_bytes(self.usage).into();
        }
        self.owner.row(title)
    }
}

impl OwnerDiskUsage {
    pub fn group_by_repo_usages(repos: Vec<RepoDiskUsage>) -> Vec<Self> {
        let mut owner_usages: HashMap<(&String, &String), u64> = HashMap::new();
        let mut owner_repo_counts: HashMap<(&String, &String), u32> = HashMap::new();

        for repo_usage in &repos {
            let key = (&repo_usage.repo.remote, &repo_usage.repo.owner);
            *owner_usages.entry(key).or_insert(0) += repo_usage.usage;
            *owner_repo_counts.entry(key).or_insert(0) += 1;
        }

        let mut result: Vec<Self> = owner_usages
            .into_iter()
            .map(|((remote, owner), usage)| OwnerDiskUsage {
                owner: OwnerState {
                    remote: remote.to_string(),
                    owner: owner.to_string(),
                    repo_count: owner_repo_counts[&(remote, owner)],
                },
                usage,
            })
            .collect();

        result.sort_unstable_by(|a, b| b.usage.cmp(&a.usage));
        result
    }
}

pub struct OwnerDiskUsageList {
    pub show_remote: bool,
    pub usages: Vec<OwnerDiskUsage>,
    pub total: u32,
}

impl List<OwnerDiskUsage> for OwnerDiskUsageList {
    fn titles(&self) -> Vec<&'static str> {
        if self.show_remote {
            vec!["Remote", "Owner", "RepoCount", "DiskUsage"]
        } else {
            vec!["Owner", "RepoCount", "DiskUsage"]
        }
    }

    fn total(&self) -> u32 {
        self.total
    }

    fn items(&self) -> &[OwnerDiskUsage] {
        &self.usages
    }
}

#[cfg(test)]
mod tests {
    use crate::config::context;
    use crate::repo::ensure_dir;
    use crate::repo::ops::RepoOperator;
    use crate::scan::disk_usage::disk_usage;
    use crate::scan::ignore::Ignore;

    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_repo_disk_usage() {
        let ctx = context::tests::build_test_context("repo_disk_usage", None);

        struct Case {
            repo: Repository,
            dirs: Vec<&'static str>,
            files: Vec<(&'static str, u64)>,
        }

        let cases = [
            Case {
                repo: Repository {
                    remote: "test".to_string(),
                    owner: "disk-usage".to_string(),
                    name: "example0".to_string(),
                    ..Default::default()
                },
                dirs: vec!["dir0", "dir1"],
                files: vec![
                    ("test.txt", 10 * 1024),
                    ("dir0/hello.txt", 20 * 1024),
                    ("dir0/subdir0/test0.txt", 30 * 1024),
                    ("dir0/subdir1/test1.txt", 40 * 1024),
                    ("dir0/test2.txt", 50 * 1024),
                    ("sample.txt", 40 * 1024),
                ],
            },
            Case {
                repo: Repository {
                    remote: "test".to_string(),
                    owner: "disk-usage".to_string(),
                    name: "example1".to_string(),
                    ..Default::default()
                },
                dirs: vec![],
                files: vec![("a.txt", 50 * 1024), ("b.txt", 60 * 1024)],
            },
            Case {
                repo: Repository {
                    remote: "test".to_string(),
                    owner: "disk-usage".to_string(),
                    name: "example".to_string(),
                    ..Default::default()
                },
                dirs: vec![],
                files: vec![],
            },
        ];

        let mut repos = vec![];
        let mut expect = vec![];
        for case in cases {
            let op = RepoOperator::new(ctx.as_ref(), &case.repo, true).unwrap();
            op.ensure_create(false, None).unwrap();
            let path = op.path();
            for dir in case.dirs {
                let path = path.join(dir);
                ensure_dir(&path).unwrap();
            }
            let usage: u64 = case.files.iter().map(|(_, s)| *s).sum();
            for (file, size) in case.files {
                let path = path.join(file);
                ensure_dir(path.parent().unwrap()).unwrap();
                std::fs::write(&path, vec![0u8; size as usize]).unwrap();
            }

            let git_usage = disk_usage(path.join(".git"), 1, Ignore::default())
                .await
                .unwrap()
                .files_usage;

            expect.push(RepoDiskUsage {
                repo: case.repo.clone(),
                usage: usage + git_usage,
            });
            repos.push(case.repo);
        }
        expect.sort_unstable_by(|a, b| b.usage.cmp(&a.usage));

        let usages = repo_disk_usage(ctx, repos).await.unwrap();
        assert_eq!(usages, expect);
    }

    #[test]
    fn test_group_remotes_by_repo_usages() {
        let repos = vec![
            RepoDiskUsage {
                repo: Repository {
                    remote: "github".to_string(),
                    owner: "user1".to_string(),
                    name: "repo1".to_string(),
                    ..Default::default()
                },
                usage: 1000,
            },
            RepoDiskUsage {
                repo: Repository {
                    remote: "github".to_string(),
                    owner: "user1".to_string(),
                    name: "repo2".to_string(),
                    ..Default::default()
                },
                usage: 2000,
            },
            RepoDiskUsage {
                repo: Repository {
                    remote: "github".to_string(),
                    owner: "user2".to_string(),
                    name: "repo3".to_string(),
                    ..Default::default()
                },
                usage: 5000,
            },
            RepoDiskUsage {
                repo: Repository {
                    remote: "gitlab".to_string(),
                    owner: "user1".to_string(),
                    name: "repo4".to_string(),
                    ..Default::default()
                },
                usage: 1500,
            },
        ];

        let result = RemoteDiskUsage::group_by_repo_usages(repos);
        let expected = vec![
            RemoteDiskUsage {
                remote: RemoteState {
                    remote: "github".to_string(),
                    owner_count: 2,
                    repo_count: 3,
                },
                usage: 8000,
            },
            RemoteDiskUsage {
                remote: RemoteState {
                    remote: "gitlab".to_string(),
                    owner_count: 1,
                    repo_count: 1,
                },
                usage: 1500,
            },
        ];
        assert_eq!(result, expected);
    }

    #[test]
    fn test_group_owners_by_repo_usages() {
        let repos = vec![
            RepoDiskUsage {
                repo: Repository {
                    remote: "github".to_string(),
                    owner: "user1".to_string(),
                    name: "repo1".to_string(),
                    ..Default::default()
                },
                usage: 1000,
            },
            RepoDiskUsage {
                repo: Repository {
                    remote: "github".to_string(),
                    owner: "user1".to_string(),
                    name: "repo2".to_string(),
                    ..Default::default()
                },
                usage: 2000,
            },
            RepoDiskUsage {
                repo: Repository {
                    remote: "github".to_string(),
                    owner: "user2".to_string(),
                    name: "repo3".to_string(),
                    ..Default::default()
                },
                usage: 5000,
            },
            RepoDiskUsage {
                repo: Repository {
                    remote: "gitlab".to_string(),
                    owner: "user1".to_string(),
                    name: "repo4".to_string(),
                    ..Default::default()
                },
                usage: 1500,
            },
        ];

        let result = OwnerDiskUsage::group_by_repo_usages(repos);

        let expected = vec![
            OwnerDiskUsage {
                owner: OwnerState {
                    remote: "github".to_string(),
                    owner: "user2".to_string(),
                    repo_count: 1,
                },
                usage: 5000,
            },
            OwnerDiskUsage {
                owner: OwnerState {
                    remote: "github".to_string(),
                    owner: "user1".to_string(),
                    repo_count: 2,
                },
                usage: 3000,
            },
            OwnerDiskUsage {
                owner: OwnerState {
                    remote: "gitlab".to_string(),
                    owner: "user1".to_string(),
                    repo_count: 1,
                },
                usage: 1500,
            },
        ];

        assert_eq!(result, expected);
    }
}
