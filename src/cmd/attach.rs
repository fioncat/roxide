use anyhow::{bail, Result};
use clap::Args;

use crate::cmd::{Completion, Run};
use crate::config::Config;
use crate::repo::database::{Database, SelectOptions, Selector};
use crate::term::Cmd;
use crate::{confirm, info, utils};

/// Attach the current directory to a repository.
#[derive(Args)]
pub struct AttachArgs {
    /// Repository selection head.
    pub head: String,

    /// Repository selection query.
    pub query: Option<String>,

    /// When calling the remote API, ignore caches that are not expired.
    #[clap(short, long)]
    pub force: bool,

    /// Append these labels to the database.
    #[clap(short, long)]
    pub labels: Option<String>,
}

impl Run for AttachArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
        let append_labels = utils::parse_labels(&self.labels);

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
        let selector = Selector::from_args(&head, &self.query, opts);
        let (mut repo, exists) = selector.one(&db)?;

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

        if let Some(user) = &repo.remote_cfg.user {
            Cmd::git(&["config", "user.name", user.as_str()])
                .with_display(format!("Set user to '{}'", user))
                .execute()?;
        }
        if let Some(email) = &repo.remote_cfg.email {
            Cmd::git(&["config", "user.email", email.as_str()])
                .with_display(format!("Set email to '{}'", email))
                .execute()?;
        }

        info!("Attach current directory to {}", repo.name_with_remote());
        repo.append_labels(append_labels);
        db.upsert(repo.update());

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
