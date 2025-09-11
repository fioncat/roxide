use std::env;
use std::ffi::OsStr;

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
use crate::repo::current::get_current_repo;
use crate::repo::mirror::MirrorSelector;

use super::App;

const INIT_ENV: &str = "ROXIDE_INIT";
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
pub fn list_pull_requests_args() -> [Arg; 3] {
    [
        branch_arg().short('b').long("base"),
        Arg::new("all").short('a').long("all"),
        Arg::new("upstream").short('u').long("upstream"),
    ]
}

fn setup_complete() -> Result<(ConfigContext, Vec<String>)> {
    let ctx = ConfigContext::setup()?;
    let mut args = env::args().collect::<Vec<_>>();
    debug!("[complete] Raw args: {args:?}");
    args.remove(0); // remove binary name
    args.remove(0); // remnove "--"
    args.pop(); // remove current
    debug!("[complete] Setup done, args: {args:?}");
    Ok((ctx, args))
}

macro_rules! register_complete {
    ($($param:ident),+ $(,)?) => {
        $(
            paste! {
                pub fn $param(current: &OsStr) -> Vec<CompletionCandidate> {
                    let (ctx, args) = match setup_complete() {
                        Ok((ctx, args)) => (ctx, args),
                        Err(e) => {
                            debug!("Complete setup error: {e:#}");
                            return vec![];
                        }
                    };
                    match [<complete_ $param>](&ctx, args, current.to_str().unwrap_or_default()) {
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

register_complete!(
    head,
    remote,
    owner,
    name,
    branch,
    tag,
    tag_method,
    config_type,
    config_name,
    mirror_name,
);

fn complete_head(
    ctx: &ConfigContext,
    args: Vec<String>,
    current: &str,
) -> Result<Vec<CompletionCandidate>> {
    debug!("[complete] Begin to complete head, current: {current:?}");
    if current.is_empty() {
        debug!("[complete] Current is empty, complete remote");
        return complete_remote(ctx, args, "");
    }

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

fn complete_remote(
    ctx: &ConfigContext,
    _args: Vec<String>,
    current: &str,
) -> Result<Vec<CompletionCandidate>> {
    debug!("[complete] Begin to complete remote, current: {current:?}");
    let db = ctx.get_db()?;
    let remotes = db.with_transaction(|tx| tx.repo().query_remotes(None))?;
    debug!("[complete] Remotes: {remotes:?}");
    Ok(remotes_to_candidates(remotes, current))
}

fn complete_owner(
    ctx: &ConfigContext,
    mut args: Vec<String>,
    current: &str,
) -> Result<Vec<CompletionCandidate>> {
    debug!("[complete] Begin to complete owner, current: {current:?}");
    let Some(remote) = args.pop() else {
        debug!("[complete] Remote is required to complete owner, skip");
        return Ok(vec![]);
    };

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

fn complete_name(
    ctx: &ConfigContext,
    mut args: Vec<String>,
    current: &str,
) -> Result<Vec<CompletionCandidate>> {
    debug!("[complete] Begin to complete name, current: {current:?}");
    let Some(owner) = args.pop() else {
        debug!("[complete] Owner is required to complete name, skip");
        return Ok(vec![]);
    };
    let Some(remote) = args.pop() else {
        debug!("[complete] Remote is required to complete name, skip");
        return Ok(vec![]);
    };

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

fn complete_branch(
    ctx: &ConfigContext,
    _args: Vec<String>,
    current: &str,
) -> Result<Vec<CompletionCandidate>> {
    debug!("[complete] Begin to complete branch, current: {current:?}");

    let branches = Branch::list(ctx.git())?;
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

fn complete_tag(
    ctx: &ConfigContext,
    _args: Vec<String>,
    current: &str,
) -> Result<Vec<CompletionCandidate>> {
    debug!("[complete] Begin to complete tag, current: {current:?}");

    let tags = Tag::list(ctx.git())?;
    debug!("[complete] Tags: {tags:?}");

    let candidates = tags
        .into_iter()
        .filter(|t| t.name.starts_with(current))
        .map(|t| CompletionCandidate::new(t.name))
        .collect::<Vec<_>>();

    debug!("[complete] Results: {candidates:?}");
    Ok(candidates)
}

fn complete_tag_method(
    _ctx: &ConfigContext,
    _args: Vec<String>,
    current: &str,
) -> Result<Vec<CompletionCandidate>> {
    debug!("[complete] Begin to complete tag method, current: {current:?}");

    let candidates = vec!["patch", "minor", "major", "date", "date-dash", "date-dot"]
        .into_iter()
        .filter(|m| m.starts_with(current))
        .map(CompletionCandidate::new)
        .collect::<Vec<_>>();

    debug!("[complete] Results: {candidates:?}");
    Ok(candidates)
}

fn complete_config_type(
    _ctx: &ConfigContext,
    _args: Vec<String>,
    current: &str,
) -> Result<Vec<CompletionCandidate>> {
    debug!("[complete] Begin to complete config type, current: {current:?}");
    let candidates = vec!["remote", "hook"]
        .into_iter()
        .filter(|m| m.starts_with(current))
        .map(CompletionCandidate::new)
        .collect::<Vec<_>>();
    debug!("[complete] Results: {candidates:?}");
    Ok(candidates)
}

fn complete_config_name(
    ctx: &ConfigContext,
    mut args: Vec<String>,
    current: &str,
) -> Result<Vec<CompletionCandidate>> {
    debug!("[complete] Begin to complete config name, current: {current:?}");
    let Some(config_type) = args.pop() else {
        debug!("[complete] Config type is required to complete config name, skip");
        return Ok(vec![]);
    };
    let mut candidates = match config_type.as_str() {
        "remote" => ctx
            .cfg
            .remotes
            .iter()
            .filter(|r| r.name.starts_with(current))
            .map(|r| CompletionCandidate::new(r.name.clone()))
            .collect::<Vec<_>>(),
        "hook" => ctx
            .cfg
            .hooks
            .hooks
            .keys()
            .filter(|h| h.starts_with(current))
            .map(|h| CompletionCandidate::new(h.clone()))
            .collect::<Vec<_>>(),
        _ => {
            debug!("[complete] Unknown config type: {config_type}, skip");
            vec![]
        }
    };
    candidates.sort_by(|a, b| a.get_value().cmp(b.get_value()));
    debug!("[complete] Results: {candidates:?}");
    Ok(candidates)
}

fn complete_mirror_name(
    ctx: &ConfigContext,
    _: Vec<String>,
    current: &str,
) -> Result<Vec<CompletionCandidate>> {
    debug!("[complete] Begin to complete mirror name, current: {current:?}");

    let repo = get_current_repo(ctx)?;
    debug!("[complete] Current repo: {repo:?}");

    let selector = MirrorSelector::new(ctx, &repo);
    let mirrors = selector.select_many()?;

    let items = mirrors
        .into_iter()
        .filter(|m| m.name.starts_with(current))
        .map(|m| CompletionCandidate::new(m.name))
        .collect::<Vec<_>>();
    debug!("[complete] Results: {items:?}");
    Ok(items)
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
mod tests {
    use crate::config::context;

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
        F: Fn(&ConfigContext, Vec<String>, &str) -> Result<Vec<CompletionCandidate>>,
    {
        let name = format!("complete_{name}");
        let mut ctx = context::tests::build_test_context(&name);

        if name == "complete_mirror_name" {
            let repo = ctx
                .get_db()
                .unwrap()
                .with_transaction(|tx| tx.repo().get("github", "fioncat", "roxide"))
                .unwrap()
                .unwrap();
            let path = repo.get_path(&ctx.cfg.workspace);
            ctx.current_dir = path;
        }

        for case in cases {
            let mut args: Vec<_> = case.args.iter().map(|s| s.to_string()).collect();
            args.insert(0, name.to_string());
            let candidates = f(&ctx, args, case.current).unwrap();
            let results = candidates
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

    #[test]
    fn test_complete_config_name() {
        let cases = [
            CompleteCase {
                args: vec![],
                current: "",
                expect: vec![],
            },
            CompleteCase {
                args: vec![],
                current: "git",
                expect: vec![],
            },
            CompleteCase {
                args: vec!["remote"],
                current: "",
                expect: vec!["github", "gitlab", "test"],
            },
            CompleteCase {
                args: vec!["remote"],
                current: "git",
                expect: vec!["github", "gitlab"],
            },
            CompleteCase {
                args: vec!["hook"],
                current: "",
                expect: vec!["cargo-init", "gomod-init"],
            },
            CompleteCase {
                args: vec!["hook"],
                current: "go",
                expect: vec!["gomod-init"],
            },
        ];

        run_cases("config_name", cases, complete_config_name);
    }

    #[test]
    fn test_complete_mirror_name() {
        let cases = [
            CompleteCase {
                args: vec![],
                current: "",
                expect: vec!["roxide-golang", "roxide-mirror", "roxide-rs"],
            },
            CompleteCase {
                args: vec![],
                current: "roxide",
                expect: vec!["roxide-golang", "roxide-mirror", "roxide-rs"],
            },
            CompleteCase {
                args: vec![],
                current: "roxide-go",
                expect: vec!["roxide-golang"],
            },
            CompleteCase {
                args: vec![],
                current: "roxide-rs",
                expect: vec!["roxide-rs"],
            },
            CompleteCase {
                args: vec![],
                current: "xxx",
                expect: vec![],
            },
        ];

        run_cases("mirror_name", cases, complete_mirror_name);
    }
}
