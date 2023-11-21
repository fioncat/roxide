use std::fs;
use std::io::ErrorKind;

use anyhow::{Context, Result};
use clap::Args;

use crate::cmd::Run;
use crate::repo::database::Database;
use crate::repo::query;
use crate::repo::tmp_mark::TmpMark;
use crate::repo::types::Repo;
use crate::term::{Cmd, GitRemote};
use crate::{config, confirm, info};

/// Copy the current repo to workspace (clone).
#[derive(Args)]
pub struct CopyArgs {
    /// The remote name.
    pub remote: String,

    /// The target repo name.
    pub name: String,

    /// Mark repo as tmp.
    #[clap(long, short)]
    pub tmp: bool,
}

impl Run for CopyArgs {
    fn run(&self) -> Result<()> {
        let mut db = Database::read()?;

        let (owner, name) = query::parse_owner(&self.name);

        if let Some(_) = db.get(&self.remote, &owner, &name) {
            confirm!("Do you want to overwrite the repo");
        }

        let remote = config::must_get_remote(&self.remote)?;

        let git_remote = GitRemote::new();
        let clone_url = git_remote.get_url()?;

        let repo = Repo::new(&self.remote, &owner, &name, None);

        if self.tmp {
            let mut tmp_mark = TmpMark::read()?;
            tmp_mark.mark(&repo);
            tmp_mark.save()?;
        }

        let path = repo.get_path();
        match fs::read_dir(&path) {
            Ok(_) => {
                info!("Remove old directory");
                fs::remove_dir_all(&path).context("Remove exists directory")?;
            }
            Err(err) if err.kind() == ErrorKind::NotFound => {}
            Err(err) => {
                return Err(err).with_context(|| format!("Read repo directory {}", path.display()))
            }
        }

        let path = format!("{}", path.display());
        Cmd::git(&["clone", &clone_url, &path])
            .with_desc(format!("Copy to {}", repo.full_name()))
            .execute()?
            .check()?;
        if let Some(user) = &remote.user {
            Cmd::git(&["-C", path.as_str(), "config", "user.name", user.as_str()])
                .with_desc(format!("Set user to {}", user))
                .execute()?
                .check()?;
        }
        if let Some(email) = &remote.email {
            Cmd::git(&["-C", path.as_str(), "config", "user.email", email.as_str()])
                .with_desc(format!("Set email to {}", email))
                .execute()?
                .check()?;
        }

        println!("{}", path);
        db.update(repo);

        db.close()
    }
}
