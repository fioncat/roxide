use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::config::context::ConfigContext;
use crate::db::DatabaseHandle;
use crate::db::mirror::Mirror;
use crate::db::repo::Repository;
use crate::info;
use crate::repo::mirror::MirrorSelector;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RestoreData {
    pub repos: Vec<Repository>,
    pub mirrors: Vec<Mirror>,
}

impl RestoreData {
    pub fn load(ctx: &ConfigContext, mut repos: Vec<Repository>) -> Result<Self> {
        let mut mirrors = Vec::new();
        for repo in repos.iter_mut() {
            let selector = MirrorSelector::new(ctx, repo);
            let repo_mirrors = selector.select_many()?;
            mirrors.extend(repo_mirrors);

            if let Some(path_raw) = repo.path.take() {
                let path = Path::new(&path_raw);
                match path.strip_prefix(&ctx.cfg.home_dir).ok() {
                    Some(rel_path) => {
                        let path = Path::new("~").join(rel_path);
                        let path = format!("{}", path.display());
                        repo.path = Some(path);
                    }
                    None => repo.path = Some(path_raw),
                }
            }
        }
        Ok(Self { repos, mirrors })
    }

    pub fn restore(self, ctx: &ConfigContext) -> Result<()> {
        let db = ctx.get_db()?;
        db.with_transaction(|tx| self.restore_innr(ctx, tx))
    }

    fn restore_innr(self, ctx: &ConfigContext, tx: &DatabaseHandle) -> Result<()> {
        let mut mirrors_map: HashMap<u64, Vec<Mirror>> = HashMap::new();
        for mirror in self.mirrors {
            match mirrors_map.get_mut(&mirror.repo_id) {
                Some(v) => v.push(mirror),
                None => {
                    mirrors_map.insert(mirror.repo_id, vec![mirror]);
                }
            }
        }

        let mut repos_count = 0;
        let mut mirrors_count = 0;
        for mut repo in self.repos {
            Self::restore_repo_path(ctx, &mut repo);

            let mirrors = mirrors_map.remove(&repo.id);
            let id = match tx.repo().get(&repo.remote, &repo.owner, &repo.name)? {
                Some(repo) => repo.id,
                None => {
                    let id = tx.repo().insert(&repo)?;
                    info!("Restored repository: {}", repo.full_name());
                    repos_count += 1;
                    id
                }
            };
            let Some(mirrors) = mirrors else {
                continue;
            };
            for mut mirror in mirrors {
                if tx
                    .mirror()
                    .get(&mirror.remote, &mirror.owner, &mirror.name)?
                    .is_none()
                {
                    mirror.repo_id = id;
                    tx.mirror().insert(&mirror)?;
                    info!(
                        "Restored mirror: {:?} for {}",
                        mirror.name,
                        repo.full_name()
                    );
                    mirrors_count += 1;
                }
            }
        }

        info!("Restore completed, with {repos_count} repo(s) and {mirrors_count} mirror(s)");
        Ok(())
    }

    pub fn dry_run(self, ctx: &ConfigContext) -> Result<Self> {
        let db = ctx.get_db()?;
        db.with_transaction(|tx| self.dry_run_inner(ctx, tx))
    }

    fn dry_run_inner(self, ctx: &ConfigContext, tx: &DatabaseHandle) -> Result<Self> {
        let mut result = Self {
            repos: Vec::new(),
            mirrors: Vec::new(),
        };
        for mut repo in self.repos {
            if tx
                .repo()
                .get(&repo.remote, &repo.owner, &repo.name)?
                .is_none()
            {
                Self::restore_repo_path(ctx, &mut repo);
                result.repos.push(repo);
            }
        }
        for mirror in self.mirrors {
            if tx
                .mirror()
                .get(&mirror.remote, &mirror.owner, &mirror.name)?
                .is_none()
            {
                result.mirrors.push(mirror);
            }
        }
        Ok(result)
    }

    fn restore_repo_path(ctx: &ConfigContext, repo: &mut Repository) {
        if let Some(path_raw) = repo.path.take() {
            let path = Path::new(&path_raw);
            match path.strip_prefix("~").ok() {
                Some(rel_path) => {
                    let path = Path::new(&ctx.cfg.home_dir).join(rel_path);
                    let path = format!("{}", path.display());
                    repo.path = Some(path);
                }
                None => repo.path = Some(path_raw),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::config::context;

    use super::*;

    #[test]
    fn test_restore() {
        let ctx = context::tests::build_test_context("restore");
        let restore_repos = vec![
            Repository {
                id: 1,
                remote: "github".to_string(),
                owner: "fioncat".to_string(),
                name: "ai-examples".to_string(),
                sync: true,
                pin: false,
                last_visited_at: 100000,
                visited_count: 222,
                ..Default::default()
            },
            Repository {
                id: 2,
                remote: "github".to_string(),
                owner: "fioncat".to_string(),
                name: "ai-dev".to_string(),
                sync: true,
                pin: true,
                path: Some("~/ai/dev".to_string()),
                last_visited_at: 99999,
                visited_count: 111,
                ..Default::default()
            },
            Repository {
                id: 3,
                remote: "github".to_string(),
                owner: "fioncat".to_string(),
                name: "roxide".to_string(),
                sync: false,
                pin: false,
                ..Default::default()
            },
        ];
        let restore_mirrors = vec![
            Mirror {
                id: 1,
                repo_id: 2,
                remote: "github".to_string(),
                owner: "fioncat".to_string(),
                name: "ai-dev-mirror".to_string(),
                last_visited_at: 3333,
                visited_count: 12,
                ..Default::default()
            },
            Mirror {
                id: 2,
                repo_id: 2,
                remote: "github".to_string(),
                owner: "fioncat".to_string(),
                name: "ai-dev-test".to_string(),
                last_visited_at: 5555,
                visited_count: 13,
                ..Default::default()
            },
            Mirror {
                id: 3,
                repo_id: 3,
                remote: "github".to_string(),
                owner: "fioncat".to_string(),
                name: "roxide-mirror-test".to_string(),
                last_visited_at: 7777,
                visited_count: 20,
                ..Default::default()
            },
            Mirror {
                id: 4,
                repo_id: 3,
                remote: "github".to_string(),
                owner: "fioncat".to_string(),
                name: "roxide-mirror".to_string(),
                last_visited_at: 8888,
                visited_count: 25,
                ..Default::default()
            },
        ];

        let data = RestoreData {
            repos: restore_repos.clone(),
            mirrors: restore_mirrors.clone(),
        };
        let result = data.clone().dry_run(&ctx).unwrap();
        let expect_result = RestoreData {
            repos: vec![restore_repos[0].clone(), {
                let mut repo = restore_repos[1].clone();
                let path = ctx.cfg.home_dir.join("ai").join("dev");
                repo.path = Some(format!("{}", path.display()));
                repo
            }],
            mirrors: vec![
                restore_mirrors[0].clone(),
                restore_mirrors[1].clone(),
                restore_mirrors[2].clone(),
            ],
        };
        assert_eq!(result, expect_result);

        data.restore(&ctx).unwrap();

        let db = ctx.get_db().unwrap();
        let mut expect = restore_repos[0].clone();
        expect.id = 7;
        let result = db
            .with_transaction(|tx| tx.repo().get("github", "fioncat", "ai-examples"))
            .unwrap()
            .unwrap();
        assert_eq!(result, expect);

        let mut expect = restore_repos[1].clone();
        expect.id = 8;
        let path = ctx.cfg.home_dir.join("ai").join("dev");
        expect.path = Some(format!("{}", path.display()));
        let result = db
            .with_transaction(|tx| tx.repo().get("github", "fioncat", "ai-dev"))
            .unwrap()
            .unwrap();
        assert_eq!(result, expect);

        // This repo is already exists, won't be inserted or updated
        let mut no_expect = restore_repos[2].clone();
        no_expect.id = 9;
        let result = db
            .with_transaction(|tx| tx.repo().get("github", "fioncat", "roxide"))
            .unwrap()
            .unwrap();
        assert_ne!(result, no_expect);

        let mut expect = restore_mirrors[0].clone();
        expect.id = 4;
        expect.repo_id = 8;
        let result = db
            .with_transaction(|tx| tx.mirror().get("github", "fioncat", "ai-dev-mirror"))
            .unwrap()
            .unwrap();
        assert_eq!(result, expect);

        let mut expect = restore_mirrors[1].clone();
        expect.id = 5;
        expect.repo_id = 8;
        let result = db
            .with_transaction(|tx| tx.mirror().get("github", "fioncat", "ai-dev-test"))
            .unwrap()
            .unwrap();
        assert_eq!(result, expect);

        let mut expect = restore_mirrors[2].clone();
        expect.id = 6;
        expect.repo_id = 1;
        let result = db
            .with_transaction(|tx| tx.mirror().get("github", "fioncat", "roxide-mirror-test"))
            .unwrap()
            .unwrap();
        assert_eq!(result, expect);

        let mut no_expect = restore_mirrors[3].clone();
        no_expect.id = 7;
        no_expect.repo_id = 1;
        let result = db
            .with_transaction(|tx| tx.mirror().get("github", "fioncat", "roxide-mirror"))
            .unwrap()
            .unwrap();
        assert_ne!(result, no_expect);
    }
}
