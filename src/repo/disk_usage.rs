use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use serde::Serialize;

use crate::config::context::ConfigContext;
use crate::db::repo::{DisplayLevel, Repository};
use crate::format::format_bytes;
use crate::scan::{ScanFile, ScanHandler, Task, scan_files_with_data};
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
        let task = Task {
            path,
            data: name.clone(),
        };
        tasks.push(task);
        usages.insert(name, RepoDiskUsage { repo, usage: 0 });
    }

    let handler = RepoDiskUsageHandler {
        usages: Mutex::new(usages),
    };
    let handler = scan_files_with_data(tasks, handler).await?;
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
    fn handle(&self, files: Vec<ScanFile>, name: Arc<String>) -> Result<()> {
        let usage: u64 = files.iter().map(|f| f.metadata.len()).sum();
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
                    name: "exmaple0".to_string(),
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
                    name: "exmaple1".to_string(),
                    ..Default::default()
                },
                dirs: vec![],
                files: vec![("a.txt", 50 * 1024), ("b.txt", 60 * 1024)],
            },
            Case {
                repo: Repository {
                    remote: "test".to_string(),
                    owner: "disk-usage".to_string(),
                    name: "exmaple2".to_string(),
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
}
