use std::env;
use std::ffi::OsStr;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Arg;
use clap_complete::env::{Bash, Shells, Zsh};
use clap_complete::{ArgValueCompleter, CompleteEnv, CompletionCandidate};
use paste::paste;

use crate::cmd::Command;
use crate::config::context::ConfigContext;
use crate::db::repo::{QueryOptions, RemoteState};
use crate::debug;
use crate::exec::git::branch::Branch;
use crate::exec::git::tag::Tag;
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

fn setup_complete() -> Vec<String> {
    if let Ok(debug) = env::var(COMPLETE_DEBUG_ENV) {
        output::set_debug(debug);
    }
    let mut args = env::args().collect::<Vec<_>>();
    debug!("[complete] Raw args: {args:?}");
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

                #[inline]
                pub fn [<$param _arg>]() -> Arg {
                    Arg::new(stringify!($param)).add(ArgValueCompleter::new($param))
                }
            }
        )+
    };
}

register_complete!(head, remote, owner, name, branch, tag);

fn complete_head(args: Vec<String>, current: &str) -> Result<Vec<CompletionCandidate>> {
    debug!("[complete] Begin to complete head, current: {current:?}");
    if current.is_empty() {
        debug!("[complete] Current is empty, complete remote");
        return complete_remote(args, "");
    }

    let ctx = build_context(&args)?;
    let db = ctx.get_db()?;
    let remotes = db.with_transaction(|tx| tx.repo().query_remotes(None))?;
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

fn complete_remote(args: Vec<String>, current: &str) -> Result<Vec<CompletionCandidate>> {
    debug!("[complete] Begin to complete remote, current: {current:?}");
    let ctx = build_context(&args)?;
    let db = ctx.get_db()?;
    let remotes = db.with_transaction(|tx| tx.repo().query_remotes(None))?;
    debug!("[complete] Remotes: {remotes:?}");
    Ok(remotes_to_candidates(remotes, current))
}

fn complete_owner(mut args: Vec<String>, current: &str) -> Result<Vec<CompletionCandidate>> {
    debug!("[complete] Begin to complete owner, current: {current:?}");
    let Some(remote) = args.pop() else {
        debug!("[complete] Remote is required to complete owner, skip");
        return Ok(vec![]);
    };

    let ctx = build_context(&args)?;
    let db = ctx.get_db()?;
    let owners = db.with_transaction(|tx| tx.repo().query_owners(Some(remote), None))?;
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

    let ctx = build_context(&args)?;

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

fn complete_branch(args: Vec<String>, current: &str) -> Result<Vec<CompletionCandidate>> {
    debug!("[complete] Begin to complete branch, current: {current:?}");
    build_context(&args)?;

    let branches = Branch::list(None::<&str>, true)?;
    debug!("[complete] Branches: {branches:?}");

    let candidates = branches
        .into_iter()
        .filter(|b| b.name.starts_with(current))
        .filter(|b| !b.current)
        .map(|b| CompletionCandidate::new(b.name))
        .collect::<Vec<_>>();

    debug!("[complete] Results: {candidates:?}");
    Ok(candidates)
}

fn complete_tag(args: Vec<String>, current: &str) -> Result<Vec<CompletionCandidate>> {
    debug!("[complete] Begin to complete tag, current: {current:?}");
    build_context(&args)?;

    let tags = Tag::list(None::<&str>, true)?;
    debug!("[complete] Tags: {tags:?}");

    let candidates = tags
        .into_iter()
        .filter(|t| t.name.starts_with(current))
        .map(|t| CompletionCandidate::new(t.name))
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

#[cfg(test)]
#[inline]
fn build_context(args: &[String]) -> Result<Arc<ConfigContext>> {
    use crate::config::context;
    let test_name = format!("complete_{}", args[0]);
    Ok(context::tests::build_test_context(&test_name, None))
}

#[cfg(not(test))]
#[inline]
fn build_context(_: &[String]) -> Result<Arc<ConfigContext>> {
    use crate::config::Config;
    let cfg = Config::read(None::<&str>)?;
    ConfigContext::new(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct CompleteCase {
        args: Vec<&'static str>,
        current: &'static str,
        expect: Vec<&'static str>,
    }

    fn run_cases<I, F>(name: &str, cases: I, f: F)
    where
        I: IntoIterator<Item = CompleteCase>,
        F: Fn(Vec<String>, &str) -> Result<Vec<CompletionCandidate>>,
    {
        for case in cases {
            let mut args: Vec<_> = case.args.iter().map(|s| s.to_string()).collect();
            args.insert(0, name.to_string());
            let canidates = f(args, case.current).unwrap();
            let results = canidates
                .iter()
                .map(|c| c.get_value().to_str().unwrap().to_string())
                .collect::<Vec<_>>();
            assert_eq!(results, case.expect, "{name} case {case:?} failed");
        }
    }

    #[test]
    fn test_complete_remote() {
        let cases = [
            CompleteCase {
                args: vec![],
                current: "",
                expect: vec!["github", "gitlab"],
            },
            CompleteCase {
                args: vec![],
                current: "gi",
                expect: vec!["github", "gitlab"],
            },
            CompleteCase {
                args: vec![],
                current: "gith",
                expect: vec!["github"],
            },
            CompleteCase {
                args: vec![],
                current: "rox",
                expect: vec![],
            },
        ];

        run_cases("remote", cases, complete_remote);
    }

    #[test]
    fn test_complete_head() {
        let cases = [
            CompleteCase {
                args: vec![],
                current: "",
                expect: vec!["github", "gitlab"],
            },
            CompleteCase {
                args: vec![],
                current: "gi",
                expect: vec!["github", "gitlab"],
            },
            CompleteCase {
                args: vec![],
                current: "gith",
                expect: vec!["github"],
            },
            CompleteCase {
                args: vec![],
                current: "rox",
                expect: vec!["roxide"],
            },
            CompleteCase {
                args: vec![],
                current: "temp",
                expect: vec!["template"],
            },
            CompleteCase {
                args: vec![],
                current: "zzzz",
                expect: vec![],
            },
        ];

        run_cases("head", cases, complete_head);
    }

    #[test]
    fn test_complete_owner() {
        let cases = [
            CompleteCase {
                args: vec!["github"],
                current: "",
                expect: vec!["kubernetes", "fioncat"],
            },
            CompleteCase {
                args: vec!["github"],
                current: "f",
                expect: vec!["fioncat"],
            },
            CompleteCase {
                args: vec!["github"],
                current: "k",
                expect: vec!["kubernetes"],
            },
            CompleteCase {
                args: vec!["github"],
                current: "x",
                expect: vec![],
            },
            CompleteCase {
                args: vec!["gitlab"],
                current: "",
                expect: vec!["fioncat"],
            },
            CompleteCase {
                args: vec!["gitlab"],
                current: "fio",
                expect: vec!["fioncat"],
            },
            CompleteCase {
                args: vec!["test"],
                current: "",
                expect: vec![],
            },
            CompleteCase {
                args: vec!["test"],
                current: "test",
                expect: vec![],
            },
        ];

        run_cases("owner", cases, complete_owner);
    }

    #[test]
    fn test_complete_name() {
        let cases = [
            CompleteCase {
                args: vec!["github", "fioncat"],
                current: "",
                expect: vec!["nvimdots", "roxide", "otree"],
            },
            CompleteCase {
                args: vec!["github", "fioncat"],
                current: "r",
                expect: vec!["roxide"],
            },
            CompleteCase {
                args: vec!["github", "fioncat"],
                current: "otr",
                expect: vec!["otree"],
            },
            CompleteCase {
                args: vec!["github", "fioncat"],
                current: "x",
                expect: vec![],
            },
            CompleteCase {
                args: vec!["github", "kubernetes"],
                current: "",
                expect: vec!["kubernetes"],
            },
            CompleteCase {
                args: vec!["github", "kubernetes"],
                current: "kube",
                expect: vec!["kubernetes"],
            },
            CompleteCase {
                args: vec!["gitlab", "fioncat"],
                current: "",
                expect: vec!["template", "someproject"],
            },
            CompleteCase {
                args: vec!["gitlab", "fioncat"],
                current: "some",
                expect: vec!["someproject"],
            },
            CompleteCase {
                args: vec!["test", "test"],
                current: "",
                expect: vec![],
            },
            CompleteCase {
                args: vec!["test", "test"],
                current: "test",
                expect: vec![],
            },
        ];

        run_cases("name", cases, complete_name);
    }
}
