use std::sync::Arc;
use std::{fs, io};

use anyhow::{Context, Result, bail};

use crate::config::context::ConfigContext;
use crate::db::repo::Repository;
use crate::exec::{bash, git};
use crate::{debug, info};

#[derive(Debug, Clone, Default)]
pub struct CreateRepoOptions {
    pub thin: bool,
    pub clone_url: Option<String>,
    pub mute: bool,
}

pub fn ensure_repo_create(
    ctx: Arc<ConfigContext>,
    repo: &Repository,
    mut opts: CreateRepoOptions,
) -> Result<()> {
    let path = repo.get_path(&ctx.cfg.workspace);

    debug!(
        "[create] Ensure repo create: {:?}, path: {}",
        repo.full_name(),
        path.display()
    );

    match fs::metadata(&path) {
        Ok(_) => {
            debug!("[create] Repo already exists, return");
            return Ok(());
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e).context("check repo path metadata"),
    };

    let remote = ctx.cfg.get_remote(&repo.remote)?;
    let clone_url = match opts.clone_url.take() {
        Some(url) => Some(url),
        None => super::get_repo_clone_url(remote, repo),
    };
    debug!("[create] Clone URL: {clone_url:?}");

    let owner = remote.get_owner(&repo.owner);
    match clone_url {
        Some(url) => {
            debug!("[create] Clone repo from {url:?}");
            let message = format!("Cloning from {url}");
            let path = format!("{}", path.display());
            let args = if opts.thin {
                vec!["clone", "--depth", "1", &url, &path]
            } else {
                vec!["clone", &url, &path]
            };
            git::new(args, None::<&str>, message, opts.mute).execute()?;

            if let Some(ref user) = owner.user {
                debug!("[create] Set user.name to {user:?}");
                let message = format!("Set user to {user:?}");
                git::new(
                    ["config", "user.name", user],
                    Some(&path),
                    message,
                    opts.mute,
                )
                .execute()?;
            }
            if let Some(ref email) = owner.email {
                debug!("[create] Set user.email to {email:?}");
                let message = format!("Set email to {email:?}");
                git::new(
                    ["config", "user.email", email],
                    Some(&path),
                    message,
                    opts.mute,
                )
                .execute()?;
            }
        }
        None => {
            debug!(
                "[create] Create empty repo, default branch: {}",
                ctx.cfg.default_branch
            );

            if !opts.mute {
                info!("Create empty repository: {}", path.display());
            }
            super::ensure_dir(&path)?;
            git::new(
                ["init", "-b", ctx.cfg.default_branch.as_str()],
                Some(&path),
                "Initializing empty git repository",
                opts.mute,
            )
            .execute()?;
        }
    };

    if !owner.on_create.is_empty() {
        let envs = [
            ("REPO_REMOTE", repo.remote.as_str()),
            ("REPO_NAME", repo.name.as_str()),
            ("REPO_OWNER", repo.owner.as_str()),
        ];

        for hook_name in owner.on_create.iter() {
            debug!("[create] Run create hook: {hook_name:?}");
            let Some(hook_path) = ctx.cfg.hooks.get(hook_name) else {
                bail!("hook {hook_name:?} not found");
            };

            bash::run(
                &path,
                hook_path,
                &envs,
                format!("Running create hook: {hook_name}"),
                opts.mute,
            )
            .with_context(|| format!("failed to run create hook {hook_name:?}"))?;
        }
    }

    debug!("[create] Ensure repo create done");
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::config::context;
    use crate::repo::get_repo_clone_url;
    use crate::repo::select::RepoSelector;

    use super::*;

    #[tokio::test]
    async fn test_create_clone() {
        if !git::tests::enable() {
            return;
        }

        let ctx = context::tests::build_test_context("create_clone", None);
        let args = ["roxide".to_string()];
        let selector = RepoSelector::from_args(ctx.clone(), &args);
        let repo = selector.select_one(false, true).await.unwrap();
        let path = repo.get_path(&ctx.cfg.workspace);
        let _ = fs::remove_dir_all(&ctx.cfg.workspace);
        ensure_repo_create(
            ctx.clone(),
            &repo,
            CreateRepoOptions {
                mute: true,
                ..Default::default()
            },
        )
        .unwrap();

        let git_remote = git::remote::Remote::origin(Some(&path), true)
            .unwrap()
            .unwrap();
        assert_eq!(git_remote.name, "origin");

        let url = git_remote.get_url().unwrap();
        let remote = ctx.cfg.get_remote(&repo.remote).unwrap();
        let expect = get_repo_clone_url(remote, &repo).unwrap();
        assert_eq!(url, expect);

        // The create can be called multiple times
        ensure_repo_create(ctx, &repo, CreateRepoOptions::default()).unwrap();
    }

    #[test]
    fn test_create_empty() {
        let repo = Repository {
            remote: "test".to_string(),
            owner: "rust".to_string(),
            name: "hello".to_string(),
            ..Default::default()
        };
        let ctx = context::tests::build_test_context("create_empty", None);
        let path = repo.get_path(&ctx.cfg.workspace);
        let _ = fs::remove_dir_all(&ctx.cfg.workspace);
        ensure_repo_create(
            ctx.clone(),
            &repo,
            CreateRepoOptions {
                mute: true,
                ..Default::default()
            },
        )
        .unwrap();

        git::new(["status"], Some(&path), "", true)
            .execute()
            .unwrap();

        // The cargo-init hook should have created a Cargo.toml
        let cargo_path = path.join("Cargo.toml");
        fs::metadata(&cargo_path).unwrap();
    }
}
