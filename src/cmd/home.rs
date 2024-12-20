use std::fs;
use std::io;
use std::path::Path;

use anyhow::{Context, Result};
use clap::Args;

use crate::batch::Task;
use crate::cmd::{Completion, CompletionResult, Run};
use crate::config::Config;
use crate::error;
use crate::exec::Cmd;
use crate::info;
use crate::repo::database::{Database, SelectOptions, Selector};
use crate::repo::detect::labels::DetectLabels;
use crate::repo::Repo;
use crate::workflow::Workflow;
use crate::{api, confirm, utils};

/// Enter a repository.
#[derive(Args)]
pub struct HomeArgs {
    /// Repository selection head.
    pub head: Option<String>,

    /// Repository selection query.
    pub query: Option<String>,

    /// Use search instead of fuzzy matching.
    #[clap(short, long)]
    pub search: bool,

    /// When calling the remote API, ignore caches that are not expired.
    #[clap(short, long)]
    pub force: bool,

    /// Open repo in default browser rather than clone it.
    #[clap(short, long)]
    pub open: bool,

    /// If the repo does not exist and needs to be cloned, use `depth=1`.
    #[clap(short, long)]
    pub thin: bool,

    /// Use a scaffolding to create the repo.
    #[clap(short, long)]
    pub bootstrap: Option<String>,

    /// Append these labels to the database.
    #[clap(short, long)]
    pub labels: Option<String>,
}

impl Run for HomeArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
        let mut db = Database::load(cfg)?;

        let append_labels = utils::parse_labels(&self.labels);

        let opts = SelectOptions::default()
            .with_force_search(self.search)
            .with_force_no_cache(self.force);
        let selector = Selector::from_args(&self.head, &self.query, opts);

        let (mut repo, exists) = selector.one(&db)?;

        if self.open {
            let provider = api::build_provider(cfg, &repo.remote_cfg, self.force)?;
            let api_repo = provider.get_repo(&repo.owner, &repo.name)?;
            return utils::open_url(api_repo.web_url);
        }

        if !exists {
            confirm!("Do you want to create {}", repo.name_with_owner());
        }

        let path = repo.get_path(cfg);
        match fs::read_dir(&path) {
            Ok(_) => {}
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                let result = self.create_dir(cfg, &repo, &path);
                if result.is_err() {
                    if let Err(err) = utils::remove_dir_recursively(path.clone(), false) {
                        error!("Remove garbage path '{}' failed: {}", path.display(), err);
                    }
                    return result;
                }
            }
            Err(err) => {
                return Err(err).with_context(|| format!("read repo directory {}", path.display()));
            }
        }

        if cfg.detect.auto {
            let detect_labels = DetectLabels::new(cfg);
            detect_labels
                .update(&mut repo)
                .context("auto detect labels for repo")?;
        }

        println!("{}", path.display());

        repo.append_labels(append_labels);
        repo.accessed += 1;
        repo.last_accessed = cfg.now();
        db.upsert(repo.update());
        db.save()
    }
}

impl HomeArgs {
    fn create_dir(&self, cfg: &Config, repo: &Repo, path: &Path) -> Result<()> {
        if let Some(ref name) = self.bootstrap {
            self.clone_from_scaffolding(name, repo, path, cfg)
        } else if repo.remote_cfg.clone.is_some() {
            self.clone(repo, path)
        } else {
            self.create_local(path)
        }?;

        if let Some(owner) = repo.remote_cfg.owners.get(repo.owner.as_ref()) {
            if let Some(on_create) = &owner.on_create {
                for wf_name in on_create.iter() {
                    let wf = Workflow::load(wf_name, cfg, repo)?;
                    wf.run()?;
                }
            }
        }

        Ok(())
    }

    fn create_local(&self, path: &Path) -> Result<()> {
        fs::create_dir_all(path)
            .with_context(|| format!("create repo directory {}", path.display()))?;
        let path = format!("{}", path.display());
        Cmd::git(&["-C", path.as_str(), "init"])
            .with_display("Git init")
            .execute()?;
        Ok(())
    }

    fn clone(&self, repo: &Repo, path: &Path) -> Result<()> {
        let url = repo.clone_url();
        let path = format!("{}", path.display());
        let mut args = vec!["clone"];
        if self.thin {
            args.extend(&["--depth", "1"]);
        }
        args.extend(&[url.as_str(), path.as_str()]);
        Cmd::git(&args)
            .with_display(format!("Clone {}", repo.name_with_remote()))
            .execute()?;

        self.init_repo_user(repo, path.as_ref())?;
        Ok(())
    }

    fn clone_from_scaffolding(
        &self,
        name: &str,
        repo: &Repo,
        path: &Path,
        cfg: &Config,
    ) -> Result<()> {
        let scaf_conf = cfg.get_scaffolding(name)?;
        let mut wfs = Vec::new();
        if !scaf_conf.exec.is_empty() {
            for wf_name in scaf_conf.exec.iter() {
                let wf = Workflow::load(wf_name, cfg, repo)?;
                wfs.push(wf);
            }
        }

        Cmd::git(&[
            "clone",
            // The scaffolding repo's git info will be soon deleted, so its clone will always
            // be shallow since we don't need the git history at all.
            "--depth",
            "1",
            scaf_conf.clone.as_ref(),
            path.to_str().unwrap(),
        ])
        .with_display(format!("Clone scaffolding '{name}'"))
        .execute()?;

        for wf in wfs.iter() {
            wf.run()?;
        }

        info!("Remove scaffolding git info");
        let git_info_path = path.join(".git");
        fs::remove_dir_all(git_info_path)?;

        let path = format!("{}", path.display());
        Cmd::git(&["-C", path.as_str(), "init"])
            .with_display("Git init")
            .execute()?;

        if repo.remote_cfg.clone.is_some() {
            self.init_repo_user(repo, path.as_ref())?;
            let url = repo.clone_url();
            Cmd::git(&["-C", path.as_str(), "remote", "add", "origin", url.as_str()])
                .with_display(format!("Set remote origin url to '{}'", url))
                .execute()?;
        }

        Ok(())
    }

    fn init_repo_user(&self, repo: &Repo, path: &Path) -> Result<()> {
        let path = format!("{}", path.display());
        if let Some(user) = &repo.remote_cfg.user {
            Cmd::git(&["-C", path.as_str(), "config", "user.name", user.as_str()])
                .with_display(format!("Set user to {}", user))
                .execute()?;
        }
        if let Some(email) = &repo.remote_cfg.email {
            Cmd::git(&["-C", path.as_str(), "config", "user.email", email.as_str()])
                .with_display(format!("Set email to {}", email))
                .execute()?;
        }
        Ok(())
    }

    pub fn completion() -> Completion {
        Completion {
            args: Completion::repo_args,
            flags: Some(|cfg, flag, to_complete| match flag {
                'l' => Completion::labels_flag(cfg, to_complete),
                'b' => Self::complete_bootstrap(cfg, to_complete),
                _ => Ok(None),
            }),
        }
    }

    fn complete_bootstrap(cfg: &Config, to_complete: &str) -> Result<Option<CompletionResult>> {
        let mut items = Vec::with_capacity(cfg.scaffoldings.len());
        for name in cfg.scaffoldings.keys() {
            if name.starts_with(to_complete) {
                items.push(name.clone());
            }
        }
        Ok(Some(CompletionResult::from(items)))
    }
}
