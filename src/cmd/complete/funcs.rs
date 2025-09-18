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

    fn run_cases<I>(name: &str, cases: I, f: fn(CompleteContext) -> Result<Vec<String>>)
    where
        I: IntoIterator<Item = CompleteCase>,
    {
        for case in cases {
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

            let args: Vec<_> = case.args.iter().map(|s| s.to_string()).collect();
            let cmp_ctx = CompleteContext {
                ctx,
                current: case.current.to_string(),
                args,
            };
            let results = f(cmp_ctx).unwrap();
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
                expect: vec!["cargo-init", "gomod-init", "print-envs"],
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
