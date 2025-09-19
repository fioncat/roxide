use std::collections::HashSet;
use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Result, bail};

use crate::config::context::ConfigContext;
use crate::db::repo::{QueryOptions, Repository};
use crate::debug;
use crate::scan::{ScanHandler, scan_files};

pub struct Orphans {
    pub workspace: Vec<PathBuf>,
    pub mirrors: Vec<PathBuf>,
}

pub async fn scan_orphans(ctx: &ConfigContext) -> Result<Orphans> {
    let db = ctx.get_db()?;

    let (remotes, owners, repos, mirrors) = db.with_transaction(|tx| {
        Ok((
            tx.repo().query_remotes(None)?,
            tx.repo().query_owners(None::<&str>, None)?,
            tx.repo().query(QueryOptions::default())?,
            tx.mirror().query_all()?,
        ))
    })?;
    let remotes = remotes
        .into_iter()
        .map(|r| Repository::escaped_path(&r.remote).into_owned())
        .collect::<HashSet<_>>();
    let owners = owners
        .into_iter()
        .map(|owner| {
            format!(
                "{}/{}",
                Repository::escaped_path(&owner.remote),
                Repository::escaped_path(&owner.owner)
            )
        })
        .collect::<HashSet<_>>();
    let repos = repos
        .into_iter()
        .map(|repo| {
            format!(
                "{}/{}/{}",
                Repository::escaped_path(&repo.remote),
                Repository::escaped_path(&repo.owner),
                repo.name
            )
        })
        .collect::<HashSet<_>>();
    let mirrors = mirrors
        .into_iter()
        .map(|mirror| {
            format!(
                "{}/{}/{}",
                Repository::escaped_path(&mirror.remote),
                Repository::escaped_path(&mirror.owner),
                mirror.name
            )
        })
        .collect::<HashSet<_>>();
    debug!(
        "[orphan] Beginning to scan orphans, remotes: {remotes:?}, owners: {owners:?}, repos: {repos:?}, mirrors: {mirrors:?}"
    );

    let workspace = PathBuf::from(&ctx.cfg.workspace);
    let mirrors_dir = PathBuf::from(&ctx.cfg.mirrors_dir);
    let workspace_handler = WorkspaceOrphanHandler {
        orphans: Mutex::new(Vec::new()),
        workspace,
        mirrors_dir: Some(mirrors_dir),
        remotes,
        owners,
        repos,
    };
    let workspace_handler = if workspace_handler.workspace.exists() {
        scan_files([&ctx.cfg.workspace], workspace_handler, true).await?
    } else {
        workspace_handler
    };
    let workspace_orphans = workspace_handler.orphans.into_inner().unwrap();
    debug!("[orphan] Found workspace orphans: {workspace_orphans:?}");

    let mirrors_handler = WorkspaceOrphanHandler {
        orphans: Mutex::new(Vec::new()),
        workspace: workspace_handler.mirrors_dir.unwrap(),
        mirrors_dir: None,
        remotes: workspace_handler.remotes,
        owners: workspace_handler.owners,
        repos: mirrors,
    };
    let mirrors_handler = if mirrors_handler.workspace.exists() {
        scan_files([&ctx.cfg.mirrors_dir], mirrors_handler, true).await?
    } else {
        mirrors_handler
    };
    let mirrors_orphans = mirrors_handler.orphans.into_inner().unwrap();
    debug!("[orphan] Found mirrors orphans: {mirrors_orphans:?}");

    Ok(Orphans {
        workspace: workspace_orphans,
        mirrors: mirrors_orphans,
    })
}

#[derive(Debug)]
struct WorkspaceOrphanHandler {
    orphans: Mutex<Vec<PathBuf>>,

    workspace: PathBuf,
    mirrors_dir: Option<PathBuf>,

    remotes: HashSet<String>,
    owners: HashSet<String>,
    repos: HashSet<String>,
}

impl ScanHandler<()> for WorkspaceOrphanHandler {
    fn handle_files(&self, files: Vec<(PathBuf, Metadata)>, _: ()) -> Result<()> {
        debug!("[orphan] Found orphan file: {files:?}");
        // All files will be treated as orphans
        let mut orphans = self.orphans.lock().unwrap();
        for (path, _) in files {
            orphans.push(path);
        }
        Ok(())
    }

    fn should_skip(&self, dir: &Path, _: ()) -> Result<bool> {
        if let Some(ref mirrors_dir) = self.mirrors_dir
            && dir == mirrors_dir
        {
            debug!("[orphan] Skipping mirrors dir: {}", dir.display());
            // Workspace handler won't handle mirrors dir
            return Ok(true);
        }

        let Some(relative) = dir.strip_prefix(&self.workspace).ok() else {
            debug!("[orphan] Skipping non-workspace dir: {}", dir.display());
            // Non-workspace dir, skip
            return Ok(true);
        };

        let mut components = vec![];
        for comp in relative.components() {
            if let Some(s) = comp.as_os_str().to_str() {
                components.push(s);
            } else {
                bail!("invalid path: {:?}", dir.display());
            }
        }

        let mut orphans = self.orphans.lock().unwrap();

        match components.len() {
            0 => {
                debug!("[orphan] Skipping workspace dir: {}", dir.display());
                Ok(true)
            }
            1 => {
                let remote = components[0];
                if self.remotes.contains(remote) {
                    debug!("[orphan] Found remote dir: {}", dir.display());
                    Ok(false) // This is a remote, continue scanning
                } else {
                    debug!("[orphan] Found orphan remote dir: {}", dir.display());
                    orphans.push(dir.to_path_buf());
                    Ok(true)
                }
            }
            2 => {
                let remote = components[0];
                let owner = components[1];
                let full = format!("{remote}/{owner}");
                if self.owners.contains(&full) {
                    debug!("[orphan] Found owner dir: {}", dir.display());
                    Ok(false) // This is an owner, continue scanning
                } else {
                    debug!("[orphan] Found orphan owner dir: {}", dir.display());
                    orphans.push(dir.to_path_buf());
                    Ok(true)
                }
            }
            3 => {
                let remote = components[0];
                let owner = components[1];
                let name = components[2];
                let full = format!("{remote}/{owner}/{name}");
                if self.repos.contains(&full) {
                    debug!("[orphan] Found repo dir: {}", dir.display());
                } else {
                    debug!("[orphan] Found orphan repo dir: {}", dir.display());
                    orphans.push(dir.to_path_buf());
                }
                Ok(true) // Whether it's a repo or not, don't scan deeper
            }
            _ => Ok(true),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::config::context;
    use crate::repo::ensure_dir;

    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_scan_orphans() {
        let ctx = context::tests::build_test_context("scan_orphans");

        let dirs = [
            "github/fioncat/roxide",
            "github/fioncat/nvimdots",
            "github/fioncat/unknown",
            "github/unknown",
            "github/kubernetes",
            "github/unknown/unknown",
            "unknown",
            "gitlab",
            "mirrors/github/fioncat/roxide-mirror",
            "mirrors/github/fioncat/roxide-nonexistent",
            "mirrors/github/unknown",
            "mirrors/github/unknown/unknown",
            "mirrors/test",
            "mirrors/unknown",
        ];
        for dir in dirs {
            let path = Path::new(&ctx.cfg.workspace).join(dir);
            ensure_dir(&path).unwrap();
        }

        let files = [
            "github/test.txt",
            "github/fioncat/roxide/test.txt",
            "github/fioncat/unknown/test.txt",
            "test.txt",
            "mirrors/test.txt",
        ];
        for file in files {
            let path = Path::new(&ctx.cfg.workspace).join(file);
            fs::write(&path, b"test").unwrap();
        }

        let orphans = scan_orphans(&ctx).await.unwrap();

        let mut result = orphans
            .workspace
            .iter()
            .map(|p| {
                p.strip_prefix(&ctx.cfg.workspace)
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_string()
            })
            .collect::<Vec<_>>();
        result.sort_unstable();
        let expect = vec![
            "github/fioncat/unknown",
            "github/test.txt",
            "github/unknown",
            "test.txt",
            "unknown",
        ];
        assert_eq!(result, expect);

        let mut result = orphans
            .mirrors
            .iter()
            .map(|p| {
                p.strip_prefix(&ctx.cfg.mirrors_dir)
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_string()
            })
            .collect::<Vec<_>>();
        result.sort_unstable();
        let expect = vec![
            "github/fioncat/roxide-nonexistent",
            "github/unknown",
            "test",
            "test.txt",
            "unknown",
        ];
        assert_eq!(result, expect);
    }
}
