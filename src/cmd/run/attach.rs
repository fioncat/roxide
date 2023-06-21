use std::collections::HashSet;
use std::rc::Rc;

use anyhow::{bail, Result};
use clap::Args;

use crate::cmd::Run;
use crate::config::types::Remote;
use crate::repo::database::Database;
use crate::repo::types::Repo;
use crate::shell::Shell;
use crate::{api, confirm, info, shell};
use crate::{config, utils};

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

        let remote = config::must_get_remote(&self.remote)?;
        let repo = self.get_repo(&remote, &db)?;

        if let Some(found) = db.get(
            remote.name.as_str(),
            repo.owner.as_str(),
            repo.name.as_str(),
        ) {
            bail!(
                "The repo {} has already been bound to {}, please detach it first",
                found.long_name(),
                found.get_path().display()
            );
        }

        confirm!(
            "Do you want to attach current directory to {}",
            repo.long_name()
        );
        if let Some(user) = &remote.user {
            Shell::git(&["config", "user.name", user.as_str()])
                .with_desc(format!("Set user to {}", user))
                .execute()?
                .check()?;
        }
        if let Some(email) = &remote.email {
            Shell::git(&["config", "user.email", email.as_str()])
                .with_desc(format!("Set email to {}", email))
                .execute()?
                .check()?;
        }
        info!("Attach current directory to {}", repo.long_name());
        db.update(repo);

        db.close()
    }
}

impl AttachArgs {
    fn get_repo(&self, remote: &Remote, db: &Database) -> Result<Rc<Repo>> {
        let path = config::current_dir();
        let path = format!("{}", path.display());
        let (owner, name) = utils::parse_query(&self.query);
        if self.query.ends_with("/") || owner.is_empty() {
            let owner = self.query.trim_end_matches("/").to_string();
            let provider = api::init_provider(remote, self.force)?;
            let items = provider.list_repos(owner.as_str())?;

            let attached = db.list_by_owner(&remote.name, &owner);
            let attached_set: HashSet<&str> =
                attached.iter().map(|repo| repo.name.as_str()).collect();

            let mut items: Vec<_> = items
                .into_iter()
                .filter(|name| !attached_set.contains(name.as_str()))
                .collect();

            let idx = shell::search(&items)?;
            let name = items.remove(idx);
            return Ok(Repo::new(&remote.name, &owner, &name, Some(path)));
        }

        Ok(Repo::new(&remote.name, &owner, &name, Some(path)))
    }
}
