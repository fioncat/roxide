use std::env;
use std::ffi::OsStr;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Arg;
use clap_complete::env::{Bash, Shells, Zsh};
use clap_complete::{ArgValueCompleter, CompleteEnv, CompletionCandidate};
use paste::paste;

use crate::cmd::Command;
use crate::config::Config;
use crate::config::context::ConfigContext;
use crate::db::repo::{LimitOptions, QueryOptions, RemoteState};
use crate::debug;
use crate::term::output;

use super::App;

const INIT_ENV: &str = "ROXIDE_INIT";
const COMPLETE_DEBUG_ENV: &str = "ROXIDE_COMPLETE_DEBUG";
const BIN_NAME: &str = "rox";

pub fn register_complete() -> Result<bool> {
    let args = env::args_os();
    let is_init = args.len() == 1;
    let current_dir = env::current_dir().ok();
    let completed = CompleteEnv::with_factory(App::complete_command)
        .var(INIT_ENV)
        .bin(BIN_NAME)
        .shells(Shells(&[&Zsh, &Bash]))
        .try_complete(args, current_dir.as_deref())?;
    if completed && is_init {
        let binary = env::current_exe().context("failed to get current exe path")?;
        let binary = format!("{}", binary.display());
        let init_script = include_str!("../../hack/rox.sh").replace("{{binary}}", &binary);
        println!();
        println!("{init_script}");
    }
    Ok(completed)
}

#[inline]
pub fn repo_args() -> [Arg; 3] {
    [head_arg(), owner_arg(), name_arg()]
}

#[inline]
pub fn head_arg() -> Arg {
    Arg::new("head").add(ArgValueCompleter::new(head))
}

#[inline]
pub fn owner_arg() -> Arg {
    Arg::new("owner").add(ArgValueCompleter::new(owner))
}

#[inline]
pub fn name_arg() -> Arg {
    Arg::new("name").add(ArgValueCompleter::new(name))
}

fn setup_complete() -> Vec<String> {
    if let Ok(debug) = env::var(COMPLETE_DEBUG_ENV) {
        output::set_debug(debug);
    }
    let mut args = env::args().collect::<Vec<_>>();
    args.remove(0); // remove binary name
    args.remove(0); // remnove "--"
    args.pop(); // remove current
    debug!("[complete] Setup done, args: {args:?}");
    args
}

macro_rules! register_complete {
    ($($param:ident),+ $(,)?) => {
        $(
            paste! {
                pub fn $param(current: &OsStr) -> Vec<CompletionCandidate> {
                    let args = setup_complete();
                    match [<complete_ $param>](args, current.to_str().unwrap_or_default()) {
                        Ok(items) => items,
                        Err(e) => {
                            debug!("Complete error: {e:#}");
                            vec![]
                        }
                    }
                }
            }
        )+
    };
}

register_complete!(head, owner, name);

fn complete_head(args: Vec<String>, current: &str) -> Result<Vec<CompletionCandidate>> {
    debug!("[complete] Begin to complete head, current: {current:?}");
    if current.is_empty() {
        debug!("[complete] Current is empty, complete remote");
        return complete_remote(args, "");
    }

    let ctx = build_context()?;
    let db = ctx.get_db()?;
    let remotes = db.with_transaction(|tx| tx.repo().query_remotes(LimitOptions::default()))?;
    debug!("[complete] Remotes: {remotes:?}");

    let remotes = remotes
        .into_iter()
        .filter(|r| r.remote.starts_with(current))
        .collect::<Vec<_>>();
    if !remotes.is_empty() {
        debug!("[complete] Found matching remotes by current, return them");
        return Ok(remotes_to_candidates(remotes, current));
    }

    debug!("[complete] No matching remotes, complete by repo names");
    let repos = db.with_transaction(|tx| tx.repo().query(QueryOptions::default()))?;
    let candidates = repos
        .into_iter()
        .filter(|r| r.name.starts_with(current))
        .map(|r| CompletionCandidate::new(r.name))
        .collect::<Vec<_>>();
    debug!("[complete] Results: {candidates:?}");
    Ok(candidates)
}

fn complete_remote(_args: Vec<String>, current: &str) -> Result<Vec<CompletionCandidate>> {
    debug!("[complete] Begin to complete remote, current: {current:?}");
    let ctx = build_context()?;
    let db = ctx.get_db()?;
    let remotes = db.with_transaction(|tx| tx.repo().query_remotes(LimitOptions::default()))?;
    debug!("[complete] Remotes: {remotes:?}");
    Ok(remotes_to_candidates(remotes, current))
}

fn complete_owner(mut args: Vec<String>, current: &str) -> Result<Vec<CompletionCandidate>> {
    debug!("[complete] Begin to complete owner, current: {current:?}");
    let Some(remote) = args.pop() else {
        debug!("[complete] Remote is required to complete owner, skip");
        return Ok(vec![]);
    };

    let ctx = build_context()?;
    let db = ctx.get_db()?;
    let owners =
        db.with_transaction(|tx| tx.repo().query_owners(&remote, LimitOptions::default()))?;
    debug!("[complete] Owners: {owners:?}");
    let candidates = owners
        .into_iter()
        .filter(|o| o.owner.starts_with(current))
        .map(|o| CompletionCandidate::new(o.owner))
        .collect::<Vec<_>>();
    debug!("[complete] Results: {candidates:?}");
    Ok(candidates)
}

fn complete_name(mut args: Vec<String>, current: &str) -> Result<Vec<CompletionCandidate>> {
    debug!("[complete] Begin to complete name, current: {current:?}");
    let Some(owner) = args.pop() else {
        debug!("[complete] Owner is required to complete name, skip");
        return Ok(vec![]);
    };
    let Some(remote) = args.pop() else {
        debug!("[complete] Remote is required to complete name, skip");
        return Ok(vec![]);
    };

    let ctx = build_context()?;
    let db = ctx.get_db()?;
    let repos = db.with_transaction(|tx| {
        tx.repo().query(QueryOptions {
            remote: Some(&remote),
            owner: Some(&owner),
            name: Some(current),
            fuzzy: true,
            ..Default::default()
        })
    })?;
    debug!("[complete] Repos: {repos:?}");
    let candidates = repos
        .into_iter()
        .filter(|n| n.name.starts_with(current))
        .map(|n| CompletionCandidate::new(n.name))
        .collect::<Vec<_>>();
    debug!("[complete] Results: {candidates:?}");
    Ok(candidates)
}

#[inline]
fn remotes_to_candidates(remotes: Vec<RemoteState>, current: &str) -> Vec<CompletionCandidate> {
    let candidates = remotes
        .into_iter()
        .filter(|r| r.remote.starts_with(current))
        .map(|r| CompletionCandidate::new(r.remote))
        .collect();
    debug!("[complete] Results: {candidates:?}");
    candidates
}

#[inline]
fn build_context() -> Result<Arc<ConfigContext>> {
    let cfg = Config::read(None::<&str>)?;
    ConfigContext::new(cfg)
}
