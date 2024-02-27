use std::borrow::Cow;
use std::collections::HashSet;
use std::fs;
use std::io;

use anyhow::{bail, Context, Result};
use clap::Args;

use crate::cmd::{Completion, Run};
use crate::config::Config;
use crate::repo::database::{self, Database};
use crate::repo::Repo;
use crate::term::{Cmd, GitRemote};
use crate::{confirm, info, utils};

/// Copy the current repository to another new repository.
#[derive(Args)]
pub struct CopyArgs {
    /// The remote name.
    pub remote: String,

    /// The target repo name.
    pub name: Option<String>,

    /// Append these labels to the database for the new repository.
    #[clap(short, long)]
    pub labels: Option<String>,
}

impl Run for CopyArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
        let mut db = Database::load(cfg)?;

        let (mut owner, mut name) = match self.name.as_ref() {
            Some(name) => database::parse_owner(name),
            None => (String::new(), String::new()),
        };
        if owner.is_empty() {
            owner = String::from("copy");
        }
        if name.is_empty() {
            let dir = cfg.get_current_dir();
            let dir_name = match dir.file_name() {
                Some(dir_name) => match dir_name.to_str() {
                    Some(dir_name) => dir_name,
                    None => bail!("invalid current directory name"),
                },
                None => bail!("you are not in a valid directory"),
            };
            name = format!("{dir_name}-copy");
        }

        let mut labels = HashSet::with_capacity(1);
        labels.insert(String::from("copy"));
        let extra_labels = utils::parse_labels(&self.labels);
        if let Some(extra_labels) = extra_labels {
            labels.extend(extra_labels);
        }

        let mut repo = Repo::new(
            cfg,
            Cow::Borrowed(&self.remote),
            Cow::Owned(owner),
            Cow::Owned(name),
            None,
        )?;
        repo.append_labels(Some(labels));

        if db
            .get(&self.remote, repo.owner.as_ref(), repo.name.as_ref())
            .is_some()
        {
            confirm!(
                "Do you want to overwrite the repo {}",
                repo.name_with_remote()
            );
        } else {
            confirm!("Do you want to create {}", repo.name_with_remote());
        }

        let git_remote = GitRemote::new();
        let clone_url = git_remote.get_url()?;

        let user = match Cmd::git(&["config", "user.name"])
            .with_display("Get current user")
            .read()
        {
            Ok(user) => Some(user),
            Err(_) => None,
        };

        let email = match Cmd::git(&["config", "user.email"])
            .with_display("Get current email")
            .read()
        {
            Ok(email) => Some(email),
            Err(_) => None,
        };

        let path = repo.get_path(cfg);
        match fs::read_dir(&path) {
            Ok(_) => {
                info!("Remove old directory");
                fs::remove_dir_all(&path).context("remove exists directory")?;
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => {
                return Err(err).with_context(|| format!("read repo directory {}", path.display()))
            }
        }

        let path = format!("{}", path.display());
        Cmd::git(&["clone", &clone_url, &path])
            .with_display(format!("Copy to {}", repo.name_with_remote()))
            .execute()?;

        if let Some(user) = user {
            Cmd::git(&["-C", path.as_str(), "config", "user.name", user.as_str()])
                .with_display(format!("Set user to {}", user))
                .execute()?;
        }
        if let Some(email) = email {
            Cmd::git(&["-C", path.as_str(), "config", "user.email", email.as_str()])
                .with_display(format!("Set email to {}", email))
                .execute()?;
        }

        println!("{path}");

        db.upsert(repo);
        db.save()
    }
}

impl CopyArgs {
    pub fn completion() -> Completion {
        Completion {
            args: Completion::owner_args,
            flags: Some(Completion::labels),
        }
    }
}
