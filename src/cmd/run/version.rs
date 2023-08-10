use anyhow::Result;
use clap::Args;

use crate::build_info::BuildInfo;
use crate::cmd::Run;

/// Show version.
#[derive(Args)]
pub struct VersionArgs {}

impl Run for VersionArgs {
    fn run(&self) -> Result<()> {
        let info = BuildInfo::new();
        info.show();
        Ok(())
    }
}
