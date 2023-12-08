use anyhow::Result;
use clap::Args;

use crate::cmd::Run;
use crate::config::Config;
use crate::confirm;
use crate::repo::database::Database;

/// Detach current path in database, don't remove directory
#[derive(Args)]
pub struct DetachArgs {}

impl Run for DetachArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
        let mut db = Database::load(cfg)?;
        let repo = db.must_get_current()?;

        confirm!(
            "Do you want to detach current path from '{}'",
            repo.name_with_remote()
        );

        db.remove(repo);

        db.save()
    }
}
