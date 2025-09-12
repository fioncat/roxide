use std::io;
use std::io::Read;

use anyhow::Context;
use anyhow::Result;
use async_trait::async_trait;
use clap::Arg;
use clap::Args;
use clap::ValueHint;
use console::style;

use crate::cmd::Command;
use crate::cmd::complete;
use crate::config::context::ConfigContext;
use crate::repo::restore::RestoreData;
use crate::repo::select::SelectRepoArgs;
use crate::term::list::TableArgs;
use crate::{debug, outputln};

/// Import repositories and mirrors from a JSON file or stdin.
#[derive(Debug, Args)]
pub struct ImportCommand {
    #[clap(flatten)]
    pub select_repo: SelectRepoArgs,

    /// The file to import. If not specified, import from stdin.
    #[arg(long, short)]
    pub file: Option<String>,

    /// Perform a trial run with no changes made.
    #[arg(long, short)]
    pub dry_run: bool,

    #[clap(flatten)]
    pub table: TableArgs,
}

#[async_trait]
impl Command for ImportCommand {
    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Run import command: {:?}", self);
        ctx.lock()?;

        let data = if let Some(file) = self.file {
            std::fs::read_to_string(&file)
                .with_context(|| format!("failed to read file {file:?}"))?
        } else {
            let mut buffer = String::new();
            io::stdin()
                .read_to_string(&mut buffer)
                .context("failed to read from stdin")?;
            buffer
        };

        let data: RestoreData = serde_json::from_str(&data).context("failed to parse JSON data")?;

        if !self.dry_run {
            return data.restore(&ctx);
        }

        let result = data.dry_run(&ctx)?;

        if result.repos.is_empty() {
            outputln!("Nothing to insert");
            return Ok(());
        }

        outputln!("{}", style("[New Repositories]").magenta().bold());
        let text = self.table.render(
            vec![
                "Remote",
                "Owner",
                "Name",
                "Flags",
                "LastVisited",
                "Visited",
                "Path",
            ],
            &result.repos,
        )?;
        outputln!("{text}");

        if !result.mirrors.is_empty() {
            outputln!("{}", style("[New Mirrors]").magenta().bold());
            let text = self.table.render(
                vec!["Remote", "Owner", "Name", "LastVisited", "Visited"],
                &result.mirrors,
            )?;
            outputln!("{text}");
        }

        Ok(())
    }

    fn complete_command() -> clap::Command {
        clap::Command::new("import")
            .args(complete::repo_args())
            .arg(
                Arg::new("file")
                    .long("file")
                    .short('f')
                    .value_hint(ValueHint::FilePath),
            )
    }
}
