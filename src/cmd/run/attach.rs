use anyhow::{bail, Result};
use clap::Args;

use crate::cmd::Run;
use crate::config;
use crate::repo::database::Database;
use crate::repo::query::{Query, SelectOptions};
use crate::repo::tmp_mark::TmpMark;
use crate::term::Cmd;
use crate::{confirm, info};

/// Attach current directory to a repo.
#[derive(Args)]
pub struct AttachArgs {
    /// The remote name.
    pub remote: String,
    /// The repo query, format is `owner[/[name]]`
    pub query: String,

    /// If true, the cache will not be used when calling the API search.
    #[clap(long, short)]
    pub force: bool,

    /// Mark repo as tmp.
    #[clap(long, short)]
    pub tmp: bool,
}

impl Run for AttachArgs {
    fn run(&self) -> Result<()> {
        let mut db = Database::read()?;

        if let Some(found) = db.current() {
            bail!(
                "This path has already been bound to {}, please detach it first",
                found.long_name()
            );
        }

        let query = Query::new(&db, vec![self.remote.clone(), self.query.clone()]);
        let (remote, repo, exists) = query.select_remote(
            SelectOptions::new()
                .with_force(self.force)
                .with_search(true)
                .with_repo_path(format!("{}", config::current_dir().display())),
        )?;
        if exists {
            bail!(
                "The repo {} has already been bound to {}, please detach it first",
                repo.long_name(),
                repo.get_path().display()
            );
        }

        confirm!(
            "Do you want to attach current directory to {}",
            repo.long_name()
        );
        if self.tmp {
            let mut tmp_mark = TmpMark::read()?;
            tmp_mark.mark(&repo);
            tmp_mark.save()?;
        }
        if let Some(user) = &remote.user {
            Cmd::git(&["config", "user.name", user.as_str()])
                .with_desc(format!("Set user to {}", user))
                .execute()?
                .check()?;
        }
        if let Some(email) = &remote.email {
            Cmd::git(&["config", "user.email", email.as_str()])
                .with_desc(format!("Set email to {}", email))
                .execute()?
                .check()?;
        }
        info!("Attach current directory to {}", repo.long_name());
        db.update(repo);

        db.close()
    }
}
