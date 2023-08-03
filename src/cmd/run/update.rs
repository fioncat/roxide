use anyhow::Result;
use clap::Args;

use crate::cmd::Run;
use crate::self_update;

/// If there is a new version, update roxide
#[derive(Args)]
pub struct UpdateArgs {}

impl Run for UpdateArgs {
    fn run(&self) -> Result<()> {
        self_update::trigger()
    }
}
