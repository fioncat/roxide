use anyhow::Result;
use async_trait::async_trait;
use clap::Args;

use crate::cmd::complete::CompleteCommand;
use crate::config::context::ConfigContext;
use crate::repo::current::get_current_repo;
use crate::repo::mirror::MirrorSelector;
use crate::term::list::TableArgs;
use crate::{debug, outputln};

use super::Command;

/// List mirrors for current repository.
#[derive(Debug, Args)]
pub struct ListMirrorCommand {
    #[clap(flatten)]
    pub table: TableArgs,
}

#[async_trait]
impl Command for ListMirrorCommand {
    fn name() -> &'static str {
        "mirror"
    }

    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Run list mirror command: {:?}", self);

        let repo = get_current_repo(&ctx)?;
        let selector = MirrorSelector::new(&ctx, &repo);
        let mirrors = selector.select_many()?;

        let repos = mirrors
            .into_iter()
            .map(|m| m.into_repo())
            .collect::<Vec<_>>();

        let text = self
            .table
            .render(vec!["ID", "Name", "LastVisited", "Visited"], &repos)?;
        outputln!("{text}");
        Ok(())
    }

    fn complete() -> CompleteCommand {
        Self::default_complete().args(TableArgs::complete())
    }
}
