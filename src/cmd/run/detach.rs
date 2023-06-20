use anyhow::Result;
use clap::Args;

use crate::cmd::Run;
use crate::confirm;
use crate::repo::database::Database;

/// Detach current path in database, donot remove directory
#[derive(Args)]
pub struct DetachArgs {}

impl Run for DetachArgs {
    fn run(&self) -> Result<()> {
        let mut db = Database::read()?;
        let repo = db.must_current()?;

        confirm!(
            "Do you want to detach current path from {}",
            repo.long_name()
        );

        db.remove(repo);

        db.close()
    }
}
