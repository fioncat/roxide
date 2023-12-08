use std::collections::HashSet;

use anyhow::{bail, Result};
use clap::Args;

use crate::cmd::{Completion, Run};
use crate::config::Config;
use crate::repo::database::{Database, SelectOptions, Selector};
use crate::term::Cmd;
use crate::{confirm, info, utils};

/// Attach current directory to a repo.
#[derive(Args)]
pub struct AttachArgs {
    pub head: String,

    pub query: Option<String>,

    /// If true, the cache will not be used when calling the API search.
    #[clap(short)]
    pub force: bool,

    /// Update repo labels with this value.
    #[clap(short)]
    pub labels: Option<String>,

    /// Clean labels for the repo.
    #[clap(short = 'L')]
    pub clean_labels: bool,
}

impl Run for AttachArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
        let update_labels = if self.clean_labels {
            Some(HashSet::new())
        } else {
            utils::parse_labels(&self.labels)
        };

        let mut db = Database::load(cfg)?;

        if let Some(found) = db.get_current() {
            bail!(
                "this path has already been bound to '{}', please detach it first",
                found.name_with_remote()
            );
        }

        let path = format!("{}", cfg.get_current_dir().display());
        let opts = SelectOptions::default()
            .with_force_no_cache(self.force)
            .with_force_remote(true)
            .with_repo_path(path);

        let head = Some(self.head.clone());
        let selector = Selector::from_args(&db, &head, &self.query, opts);
        let (repo, exists) = selector.one()?;

        if exists {
            bail!(
                "the repo '{}' has already been bound to '{}', please detach it first",
                repo.name_with_remote(),
                repo.get_path(cfg).display()
            );
        }

        confirm!(
            "Do you want to attach current directory to {}",
            repo.name_with_remote()
        );

        if let Some(user) = &repo.remote.cfg.user {
            Cmd::git(&["config", "user.name", user.as_str()])
                .with_display(format!("Set user to '{}'", user))
                .execute_check()?;
        }
        if let Some(email) = &repo.remote.cfg.email {
            Cmd::git(&["config", "user.email", email.as_str()])
                .with_display(format!("Set email to '{}'", email))
                .execute_check()?;
        }

        info!("Attach current directory to {}", repo.name_with_remote());
        db.update(repo, update_labels);

        db.save()
    }
}

impl AttachArgs {
    pub fn completion() -> Completion {
        Completion {
            args: Completion::owner_args,
            flags: Some(Completion::labels),
        }
    }
}
