use anyhow::Context;
use anyhow::Result;
use async_trait::async_trait;
use clap::Arg;
use clap::Args;
use clap::ValueHint;

use crate::cmd::Command;
use crate::cmd::complete;
use crate::config::context::ConfigContext;
use crate::debug;
use crate::repo::backup::BackupData;
use crate::repo::select::SelectRepoArgs;
use crate::repo::select::{RepoSelector, SelectManyReposOptions};

/// Export repositories and mirrors to a JSON file or stdout.
#[derive(Debug, Args)]
pub struct ExportCommand {
    #[clap(flatten)]
    pub select_repo: SelectRepoArgs,

    /// The file to export. If not specified, export to stdout.
    #[arg(long, short)]
    pub file: Option<String>,
}

#[async_trait]
impl Command for ExportCommand {
    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Run export command: {:?}", self);

        let selector = RepoSelector::new(&ctx, &self.select_repo);
        let list = selector.select_many(SelectManyReposOptions::default())?;
        let repos = list.items;
        let data = BackupData::load(&ctx, repos)?;

        let data = serde_json::to_string(&data)?;
        if let Some(file) = self.file {
            std::fs::write(&file, data)
                .with_context(|| format!("failed to write file {file:?}"))?;
            return Ok(());
        }

        println!("{data}");
        Ok(())
    }

    fn complete_command() -> clap::Command {
        clap::Command::new("export")
            .args(complete::repo_args())
            .arg(
                Arg::new("file")
                    .long("file")
                    .short('f')
                    .value_hint(ValueHint::AnyPath),
            )
    }
}
