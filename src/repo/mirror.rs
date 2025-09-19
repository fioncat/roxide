use anyhow::Result;

use crate::config::context::ConfigContext;
use crate::db::mirror::Mirror;
use crate::db::repo::Repository;
use crate::repo::ops::RepoOperator;
use crate::{debug, info};

pub struct MirrorSelector<'ctx, 'repo> {
    ctx: &'ctx ConfigContext,

    repo: &'repo Repository,

    fzf_filter: Option<&'static str>,
}

impl<'ctx, 'repo> MirrorSelector<'ctx, 'repo> {
    pub fn new(ctx: &'ctx ConfigContext, repo: &'repo Repository) -> Self {
        Self {
            ctx,
            repo,
            fzf_filter: None,
        }
    }

    pub fn select_one(&self, name: Option<String>, fuzzy: bool) -> Result<Mirror> {
        let mut mirrors = self.select_many()?;
        if let Some(mut name) = name {
            if name == "-" {
                name = format!("{}-mirror", self.repo.name);
            }
            let mirror = mirrors
                .into_iter()
                .find(|m| {
                    if fuzzy {
                        m.name.contains(&name)
                    } else {
                        m.name == name
                    }
                })
                .unwrap_or(Mirror::new_from_repo(self.repo, Some(name)));
            return Ok(mirror);
        }

        if mirrors.is_empty() {
            return Ok(Mirror::new_from_repo(self.repo, None));
        }

        if mirrors.len() == 1 {
            return Ok(mirrors.remove(0));
        }

        let items = mirrors.iter().map(|m| m.name.as_str()).collect::<Vec<_>>();
        let idx = self
            .ctx
            .fzf_search("Selecting a mirror", &items, self.fzf_filter)?;
        Ok(mirrors.remove(idx))
    }

    pub fn select_many(&self) -> Result<Vec<Mirror>> {
        let db = self.ctx.get_db()?;
        db.with_transaction(|tx| tx.mirror().query_by_repo_id(self.repo.id))
    }
}

pub fn get_current_mirror(ctx: &ConfigContext) -> Result<Option<(Repository, Mirror)>> {
    debug!(
        "[mirror] Trying to get current mirror in {:?}",
        ctx.current_dir
    );
    let Some((remote, owner, mirror_name)) =
        super::parse_workspace_path(&ctx.cfg.mirrors_dir, &ctx.current_dir)
    else {
        debug!("[mirror] Current directory is not in mirrors workspace");
        return Ok(None);
    };
    debug!("[mirror] Current is in the mirror workspace: {remote}/{owner}/{mirror_name}");

    let db = ctx.get_db()?;
    db.with_transaction(|tx| {
        let Some(mirror) = tx.mirror().get(&remote, &owner, &mirror_name)? else {
            debug!("[mirror] Mirror not found in database");
            return Ok(None);
        };
        let Some(repo) = tx.repo().get_by_id(mirror.repo_id)? else {
            debug!(
                "[mirror] Repo id {} not found in database, we need to clean the mirror",
                mirror.repo_id
            );
            tx.mirror().delete_by_repo_id(mirror.repo_id)?;
            return Ok(None);
        };
        Ok(Some((repo, mirror)))
    })
}

pub fn remove_mirror(ctx: &ConfigContext, repo: &Repository, mirror: Mirror) -> Result<()> {
    let path = mirror.get_path(&ctx.cfg.mirrors_dir);
    debug!(
        "[mirror] Removing mirror {mirror:?} for repo: {repo:?}, path: {:?}",
        path.display()
    );

    let repo_op = RepoOperator::load(ctx, repo)?;
    let id = mirror.id;

    let mirror_repo = mirror.into_repo();
    let mirror_op = RepoOperator::new(ctx, repo_op.remote(), repo_op.owner(), &mirror_repo, path);
    mirror_op.remove()?;

    let db = ctx.get_db()?;
    db.with_transaction(|tx| tx.mirror().delete(id))?;

    Ok(())
}

pub fn clean_mirrors(ctx: &ConfigContext, repo: &Repository) -> Result<()> {
    debug!("[mirror] Cleaning mirrors for repo: {repo:?}");
    let db = ctx.get_db()?;
    let mirrors = db.with_transaction(|tx| tx.mirror().query_by_repo_id(repo.id))?;
    if mirrors.is_empty() {
        debug!("[mirror] No mirrors to clean");
        return Ok(());
    }

    let repo_op = RepoOperator::load(ctx, repo)?;

    let ids = mirrors.iter().map(|m| m.id).collect::<Vec<_>>();
    for mirror in mirrors {
        let path = mirror.get_path(&ctx.cfg.mirrors_dir);
        info!("Remove mirror {} for {}", mirror.name, repo.full_name());
        let mirror_repo = mirror.into_repo();
        let mirror_op =
            RepoOperator::new(ctx, repo_op.remote(), repo_op.owner(), &mirror_repo, path);
        mirror_op.remove()?;
    }

    debug!("[mirror] Removing mirrors {ids:?} from database");
    db.with_transaction(|tx| {
        for id in ids {
            tx.mirror().delete(id)?;
        }
        Ok(())
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::config::context;
    use crate::db;
    use crate::repo::ensure_dir;

    use super::*;

    #[test]
    fn test_select_one() {
        #[derive(Debug)]
        struct Case {
            name: Option<String>,
            fuzzy: bool,
            filter: Option<&'static str>,
            expect: Mirror,
        }

        let cases = [
            Case {
                name: None,
                fuzzy: true,
                filter: Some("roxide-mirror"),
                expect: Mirror {
                    id: 1,
                    repo_id: 1,
                    name: "roxide-mirror".to_string(),
                    owner: "fioncat".to_string(),
                    remote: "github".to_string(),
                    last_visited_at: 1234,
                    visited_count: 12,
                    new_created: false,
                },
            },
            Case {
                name: Some("golang".to_string()),
                fuzzy: true,
                filter: None,
                expect: Mirror {
                    id: 2,
                    remote: "github".to_string(),
                    owner: "fioncat".to_string(),
                    name: "roxide-golang".to_string(),
                    repo_id: 1,
                    last_visited_at: 3456,
                    visited_count: 1,
                    new_created: false,
                },
            },
            Case {
                name: Some("roxide-rs".to_string()),
                fuzzy: false,
                filter: None,
                expect: Mirror {
                    id: 3,
                    remote: "github".to_string(),
                    owner: "fioncat".to_string(),
                    name: "roxide-rs".to_string(),
                    repo_id: 1,
                    last_visited_at: 89,
                    visited_count: 3,
                    new_created: false,
                },
            },
        ];

        let ctx = context::tests::build_test_context("select_one_mirror");
        let repo = ctx
            .get_db()
            .unwrap()
            .with_transaction(|tx| tx.repo().get("github", "fioncat", "roxide"))
            .unwrap()
            .unwrap();
        for case in cases {
            let mut selector = MirrorSelector::new(&ctx, &repo);
            selector.fzf_filter = case.filter;

            let mirror = selector.select_one(case.name.clone(), case.fuzzy).unwrap();
            assert_eq!(mirror, case.expect, "{case:?}");
        }
    }

    #[test]
    fn test_create() {
        let ctx = context::tests::build_test_context("create_mirror");
        let repo = ctx
            .get_db()
            .unwrap()
            .with_transaction(|tx| tx.repo().get("github", "kubernetes", "kubernetes"))
            .unwrap()
            .unwrap();

        let selector = MirrorSelector::new(&ctx, &repo);
        let mirror = selector.select_one(None, false).unwrap();
        assert_eq!(
            mirror,
            Mirror {
                repo_id: repo.id,
                remote: repo.remote.clone(),
                owner: repo.owner.clone(),
                name: format!("{}-mirror", repo.name),
                new_created: true,
                ..Mirror::default()
            }
        );

        let mirror = selector
            .select_one(Some("test".to_string()), false)
            .unwrap();
        assert_eq!(
            mirror,
            Mirror {
                repo_id: repo.id,
                remote: repo.remote.clone(),
                owner: repo.owner.clone(),
                name: "test".to_string(),
                new_created: true,
                ..Mirror::default()
            }
        );
    }

    #[test]
    fn test_select_many() {
        let ctx = context::tests::build_test_context("select_many_mirrors");
        let repo = ctx
            .get_db()
            .unwrap()
            .with_transaction(|tx| tx.repo().get("github", "fioncat", "roxide"))
            .unwrap()
            .unwrap();
        let mirrors = MirrorSelector::new(&ctx, &repo).select_many().unwrap();
        let mut expect = db::tests::test_mirrors();
        expect.sort_by(|a, b| b.last_visited_at.cmp(&a.last_visited_at));
        assert_eq!(mirrors, expect);

        let repo = ctx
            .get_db()
            .unwrap()
            .with_transaction(|tx| tx.repo().get("github", "kubernetes", "kubernetes"))
            .unwrap()
            .unwrap();
        let mirrors = MirrorSelector::new(&ctx, &repo).select_many().unwrap();
        assert!(mirrors.is_empty());
    }

    #[test]
    fn test_current_mirror() {
        let mut ctx = context::tests::build_test_context("current_mirror");
        let expect_repo = ctx
            .get_db()
            .unwrap()
            .with_transaction(|tx| tx.repo().get("github", "fioncat", "roxide"))
            .unwrap()
            .unwrap();

        let expect_mirror = ctx
            .get_db()
            .unwrap()
            .with_transaction(|tx| tx.mirror().get("github", "fioncat", "roxide-mirror"))
            .unwrap()
            .unwrap();

        let path = expect_mirror.get_path(&ctx.cfg.mirrors_dir);
        ctx.current_dir = path;

        let (repo, mirror) = get_current_mirror(&ctx).unwrap().unwrap();
        assert_eq!(repo, expect_repo);
        assert_eq!(mirror, expect_mirror);
    }

    #[test]
    fn test_remove_mirror() {
        let ctx = context::tests::build_test_context("remove_mirror");
        let repo = ctx
            .get_db()
            .unwrap()
            .with_transaction(|tx| tx.repo().get("github", "fioncat", "roxide"))
            .unwrap()
            .unwrap();
        let mirror = ctx
            .get_db()
            .unwrap()
            .with_transaction(|tx| tx.mirror().get("github", "fioncat", "roxide-mirror"))
            .unwrap()
            .unwrap();
        assert!(!mirror.new_created);

        let path = mirror.get_path(&ctx.cfg.mirrors_dir);
        ensure_dir(&path).unwrap();
        ensure_dir(path.join(".git")).unwrap();
        ensure_dir(path.join("src")).unwrap();

        let mirrors_dir = PathBuf::from(&ctx.cfg.mirrors_dir);
        assert!(mirrors_dir.exists());

        remove_mirror(&ctx, &repo, mirror).unwrap();

        let mirror = ctx
            .get_db()
            .unwrap()
            .with_transaction(|tx| tx.mirror().get("github", "fioncat", "roxide-mirror"))
            .unwrap();
        assert_eq!(mirror, None);
        assert!(!mirrors_dir.exists());
    }

    #[test]
    fn test_clean_mirrors() {
        let ctx = context::tests::build_test_context("clean_mirrors");
        let repo = ctx
            .get_db()
            .unwrap()
            .with_transaction(|tx| tx.repo().get("github", "fioncat", "roxide"))
            .unwrap()
            .unwrap();

        let mirrors = ctx
            .get_db()
            .unwrap()
            .with_transaction(|tx| tx.mirror().query_by_repo_id(repo.id))
            .unwrap();
        for mirror in mirrors.iter() {
            let path = mirror.get_path(&ctx.cfg.mirrors_dir);
            ensure_dir(&path).unwrap();
            ensure_dir(path.join(".git")).unwrap();
            ensure_dir(path.join("src")).unwrap();
        }

        let mirrors_dir = PathBuf::from(&ctx.cfg.mirrors_dir);
        assert!(mirrors_dir.exists());

        clean_mirrors(&ctx, &repo).unwrap();
        let mirrors = ctx
            .get_db()
            .unwrap()
            .with_transaction(|tx| tx.mirror().query_by_repo_id(repo.id))
            .unwrap();
        assert!(mirrors.is_empty());
        assert!(!mirrors_dir.exists());
    }
}
