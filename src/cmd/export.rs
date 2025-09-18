use anyhow::{Context, Result};
use async_trait::async_trait;
use clap::Args;

use crate::cmd::Command;
use crate::cmd::complete::{CompleteArg, CompleteCommand};
use crate::config::context::ConfigContext;
use crate::debug;
use crate::repo::restore::RestoreData;
use crate::repo::select::{RepoSelector, SelectManyReposOptions, SelectRepoArgs};

/// Export repositories and mirrors to a JSON file or stdout.
#[derive(Debug, Args)]
pub struct ExportCommand {
    #[clap(flatten)]
    pub select_repo: SelectRepoArgs,

    /// The file to export. If not specified, export to stdout.
    #[arg(short)]
    pub file: Option<String>,
}

#[async_trait]
impl Command for ExportCommand {
    fn name() -> &'static str {
        "export"
    }

    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Run export command: {:?}", self);

        let selector = RepoSelector::new(&ctx, &self.select_repo);
        let list = selector.select_many(SelectManyReposOptions::default())?;
        let repos = list.items;
        let data = RestoreData::load(&ctx, repos)?;

        let data = serde_json::to_string(&data)?;
        if let Some(file) = self.file {
            std::fs::write(&file, data)
                .with_context(|| format!("failed to write file {file:?}"))?;
            return Ok(());
        }

        println!("{data}");
        Ok(())
    }

    fn complete() -> CompleteCommand {
        Self::default_complete()
            .args(SelectRepoArgs::complete())
            .arg(CompleteArg::new().short('f').files())
    }
}
