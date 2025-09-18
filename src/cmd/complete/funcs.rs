use crate::db::repo::{QueryOptions, RemoteState};
use crate::exec::git::branch::Branch;
use crate::exec::git::tag::Tag;
use crate::repo::current::get_current_repo;
use crate::repo::mirror::MirrorSelector;

use super::*;

pub fn no_complete(_cmp: CompleteContext) -> Result<Vec<String>> {
    Ok(vec![])
}

pub fn complete_head(cmp: CompleteContext) -> Result<Vec<String>> {
    debug!("[complete] Begin to complete head: {cmp}");
    if cmp.current.is_empty() {
        debug!("[complete] Current is empty, complete remote");
        return complete_remote(cmp);
    }

    let db = cmp.ctx.get_db()?;
    let remotes = db.with_transaction(|tx| tx.repo().query_remotes(None))?;
    debug!("[complete] Remotes: {remotes:?}");

    let remotes = remotes
        .into_iter()
        .filter(|r| r.remote.starts_with(&cmp.current))
        .collect::<Vec<_>>();
    if !remotes.is_empty() {
        debug!("[complete] Found matching remotes by current, return them");
        return Ok(remote_items(remotes, &cmp.current));
    }

    debug!("[complete] No matching remotes, complete by repo names");
    let repos = db.with_transaction(|tx| tx.repo().query(QueryOptions::default()))?;
    let items = repos
        .into_iter()
        .filter(|r| r.name.starts_with(&cmp.current))
        .map(|r| r.name)
        .collect::<Vec<_>>();
    debug!("[complete] Results: {items:?}");
    Ok(items)
}

pub fn complete_remote(cmp: CompleteContext) -> Result<Vec<String>> {
    debug!("[complete] Begin to complete remote: {cmp}");
    let db = cmp.ctx.get_db()?;
    let remotes = db.with_transaction(|tx| tx.repo().query_remotes(None))?;
    debug!("[complete] Remotes: {remotes:?}");
    Ok(remote_items(remotes, &cmp.current))
}

pub fn remote_items(remotes: Vec<RemoteState>, current: &str) -> Vec<String> {
    let items = remotes
        .into_iter()
        .filter(|r| r.remote.starts_with(current))
        .map(|r| r.remote)
        .collect();
    debug!("[complete] Results: {items:?}");
    items
}

pub fn complete_owner(mut cmp: CompleteContext) -> Result<Vec<String>> {
    debug!("[complete] Begin to complete owner: {cmp}");
    let Some(remote) = cmp.args.pop() else {
        debug!("[complete] Remote is required to complete owner, skip");
        return Ok(vec![]);
    };

    let db = cmp.ctx.get_db()?;
    let owners = db.with_transaction(|tx| tx.repo().query_owners(Some(remote), None))?;
    debug!("[complete] Owners: {owners:?}");
    let items = owners
        .into_iter()
        .filter(|o| o.owner.starts_with(&cmp.current))
        .map(|o| o.owner)
        .collect::<Vec<_>>();
    debug!("[complete] Results: {items:?}");
    Ok(items)
}

pub fn complete_name(mut cmp: CompleteContext) -> Result<Vec<String>> {
    debug!("[complete] Begin to complete name: {cmp}");
    let Some(owner) = cmp.args.pop() else {
        debug!("[complete] Owner is required to complete name, skip");
        return Ok(vec![]);
    };
    let Some(remote) = cmp.args.pop() else {
        debug!("[complete] Remote is required to complete name, skip");
        return Ok(vec![]);
    };

    let db = cmp.ctx.get_db()?;
    let repos = db.with_transaction(|tx| {
        tx.repo().query(QueryOptions {
            remote: Some(&remote),
            owner: Some(&owner),
            name: Some(&cmp.current),
            fuzzy: true,
            ..Default::default()
        })
    })?;
    debug!("[complete] Repos: {repos:?}");
    let items = repos
        .into_iter()
        .filter(|n| n.name.starts_with(&cmp.current))
        .map(|n| n.name)
        .collect::<Vec<_>>();
    debug!("[complete] Results: {items:?}");
    Ok(items)
}

pub fn complete_branch(cmp: CompleteContext) -> Result<Vec<String>> {
    debug!("[complete] Begin to complete branch: {cmp}");

    let branches = Branch::list(cmp.ctx.git().mute())?;
    debug!("[complete] Branches: {branches:?}");

    let items = branches
        .into_iter()
        .filter(|b| b.name.starts_with(&cmp.current))
        .filter(|b| !b.current)
        .map(|b| b.name)
        .collect::<Vec<_>>();
    debug!("[complete] Results: {items:?}");
    Ok(items)
}

pub fn complete_tag(cmp: CompleteContext) -> Result<Vec<String>> {
    debug!("[complete] Begin to complete tag: {cmp}");

    let tags = Tag::list(cmp.ctx.git().mute())?;
    debug!("[complete] Tags: {tags:?}");

    let items = tags
        .into_iter()
        .filter(|t| t.name.starts_with(&cmp.current))
        .map(|t| t.name)
        .collect::<Vec<_>>();

    debug!("[complete] Results: {items:?}");
    Ok(items)
}

pub fn complete_tag_method(cmp: CompleteContext) -> Result<Vec<String>> {
    debug!("[complete] Begin to complete tag method: {cmp}");

    let items = vec!["patch", "minor", "major", "date", "date-dash", "date-dot"]
        .into_iter()
        .filter(|m| m.starts_with(&cmp.current))
        .map(|m| m.to_string())
        .collect::<Vec<_>>();

    debug!("[complete] Results: {items:?}");
    Ok(items)
}

pub fn complete_config_type(cmp: CompleteContext) -> Result<Vec<String>> {
    debug!("[complete] Begin to complete config type: {cmp}");
    let items = vec!["remote", "hook"]
        .into_iter()
        .filter(|m| m.starts_with(&cmp.current))
        .map(|m| m.to_string())
        .collect::<Vec<_>>();
    debug!("[complete] Results: {items:?}");
    Ok(items)
}

pub fn complete_config_name(mut cmp: CompleteContext) -> Result<Vec<String>> {
    debug!("[complete] Begin to complete config name: {cmp}");
    let Some(config_type) = cmp.args.pop() else {
        debug!("[complete] Config type is required to complete config name, skip");
        return Ok(vec![]);
    };
    let mut items = match config_type.as_str() {
        "remote" => cmp
            .ctx
            .cfg
            .remotes
            .iter()
            .filter(|r| r.name.starts_with(&cmp.current))
            .map(|r| r.name.clone())
            .collect::<Vec<_>>(),
        "hook" => cmp
            .ctx
            .cfg
            .hook_runs
            .hooks
            .keys()
            .filter(|h| h.starts_with(&cmp.current))
            .cloned()
            .collect::<Vec<_>>(),
        _ => {
            debug!("[complete] Unknown config type: {config_type}, skip");
            vec![]
        }
    };
    items.sort_unstable();
    debug!("[complete] Results: {items:?}");
    Ok(items)
}

pub fn complete_mirror_name(cmp: CompleteContext) -> Result<Vec<String>> {
    debug!("[complete] Begin to complete mirror name: {cmp}");

    let repo = get_current_repo(&cmp.ctx)?;
    debug!("[complete] Current repo: {repo:?}");

    let selector = MirrorSelector::new(&cmp.ctx, &repo);
    let mirrors = selector.select_many()?;

    let items = mirrors
        .into_iter()
        .filter(|m| m.name.starts_with(&cmp.current))
        .map(|m| m.name)
        .collect::<Vec<_>>();
    debug!("[complete] Results: {items:?}");
    Ok(items)
}
