use std::sync::Arc;

use anyhow::{Result, bail};

use crate::config::context::ConfigContext;
use crate::db::LimitOptions;
use crate::db::repo::{Repository, SearchLevel};
use crate::exec::fzf;

pub struct RepoSelector<'a> {
    ctx: Arc<ConfigContext>,

    head: &'a str,
    owner: &'a str,
    name: &'a str,

    limit: LimitOptions,
}

impl<'a> RepoSelector<'a> {
    pub fn new(ctx: Arc<ConfigContext>, args: &'a [String]) -> Self {
        Self {
            ctx,
            head: args.first().map(|s| s.as_str()).unwrap_or_default(),
            owner: args.get(1).map(|s| s.as_str()).unwrap_or_default(),
            name: args.get(2).map(|s| s.as_str()).unwrap_or_default(),
            limit: LimitOptions::default(),
        }
    }

    pub fn select_one(&self, force_no_cache: bool, no_remote: bool) -> Result<Repository> {
        if self.head.is_empty() {
            return self.select_one_from_db("", "", false);
        }
        todo!()
    }

    fn select_one_from_db(&self, remote: &str, owner: &str, latest: bool) -> Result<Repository> {
        let db = self.ctx.get_db()?;
        let mut level = SearchLevel::Name;
        let mut repos = db.with_transaction(|tx| {
            if remote.is_empty() {
                level = SearchLevel::Remote;
                tx.repo().query_all(self.limit)
            } else if owner.is_empty() {
                level = SearchLevel::Owner;
                tx.repo().query_by_remote(remote, self.limit)
            } else {
                tx.repo().query_by_owner(remote, owner, self.limit)
            }
        })?;

        if repos.is_empty() {
            bail!("no repo to select");
        }

        if latest {
            return Ok(repos.remove(0));
        }

        let items = repos
            .iter()
            .map(|r| r.search_item(level))
            .collect::<Vec<_>>();
        let idx = fzf::search("Select one repo from database", &items, None)?;
        Ok(repos.remove(idx))
    }
}
