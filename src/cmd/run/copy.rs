use std::fs;
use std::io::ErrorKind;

use anyhow::{bail, Context, Result};
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
    pub name: Option<String>,

    /// Mark repo as tmp.
    #[clap(long, short)]
    pub tmp: bool,
}

impl Run for CopyArgs {
    fn run(&self) -> Result<()> {
        let mut db = Database::read()?;

        let (mut owner, mut name) = match self.name.as_ref() {
            Some(name) => query::parse_owner(name),
            None => (String::new(), String::new()),
        };
        if owner.is_empty() {
            owner = String::from("copy");
        }
        if name.is_empty() {
            let dir = config::current_dir();
            let dir_name = match dir.file_name() {
                Some(dir_name) => match dir_name.to_str() {
                    Some(dir_name) => dir_name,
                    None => bail!("Invalid current directory name"),
                },
                None => bail!("You are not in a valid directory"),
            };
            name = format!("{dir_name}-copy");
        }
        let repo = Repo::new(&self.remote, &owner, &name, None);

        if let Some(_) = db.get(&self.remote, &owner, &name) {
            confirm!("Do you want to overwrite the repo {}", repo.full_name());
        } else {
            confirm!("Do you want to create {}", repo.full_name());
        }

        let git_remote = GitRemote::new();
        let clone_url = git_remote.get_url()?;
        let user = match Cmd::git(&["config", "user.name"])
            .with_desc("Get current user")
            .execute()?
            .checked_read()
        {
            Ok(user) => Some(user),
            Err(_) => None,
        };
        let email = match Cmd::git(&["config", "user.email"])
            .with_desc("Get current email")
            .execute()?
            .checked_read()
        {
            Ok(email) => Some(email),
            Err(_) => None,
        };

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
        if let Some(user) = user {
            Cmd::git(&["-C", path.as_str(), "config", "user.name", user.as_str()])
                .with_desc(format!("Set user to {}", user))
                .execute()?
                .check()?;
        }
        if let Some(email) = email {
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
