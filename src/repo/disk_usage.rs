use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use serde::Serialize;

use crate::config::context::ConfigContext;
use crate::db::repo::Repository;
use crate::format::format_bytes;
use crate::scan::{ScanFile, ScanHandler, Task, scan_files_with_data};
use crate::term::list::ListItem;

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

#[derive(Debug, Serialize)]
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
