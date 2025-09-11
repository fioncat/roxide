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
            .fzf_search("Select a mirror", &items, self.fzf_filter)?;
        Ok(mirrors.remove(idx))
    }

    pub fn select_many(&self) -> Result<Vec<Mirror>> {
        let db = self.ctx.get_db()?;
        db.with_transaction(|tx| tx.mirror().query_by_repo_id(self.repo.id))
    }
}

pub fn get_current_mirror(ctx: &ConfigContext) -> Result<Option<(Repository, Mirror)>> {
    debug!(
        "[mirror] Try to get current mirror in {:?}",
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
        "[mirror] Remove mirror {mirror:?} for repo: {repo:?}, path: {:?}",
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
    debug!("[mirror] Clean mirrors for repo: {repo:?}");
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

    debug!("[mirror] Remove mirrors {ids:?} from database");
    db.with_transaction(|tx| {
        for id in ids {
            tx.mirror().delete(id)?;
        }
        Ok(())
    })?;
    Ok(())
}
