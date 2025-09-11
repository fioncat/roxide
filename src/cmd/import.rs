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
use crate::repo::backup::BackupData;
use crate::repo::select::SelectRepoArgs;
use crate::term::list::TableArgs;
use crate::{debug, outputln};

/// Import repositories and mirrors from a JSON file or stdin.
#[derive(Debug, Args)]
pub struct ExportCommand {
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
impl Command for ExportCommand {
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

        let data: BackupData = serde_json::from_str(&data).context("failed to parse JSON data")?;

        let db = ctx.get_db()?;
        if !self.dry_run {
            return db.with_transaction(|tx| data.restore(tx));
        }

        let repo_titles = vec![
            "Remote",
            "Owner",
            "Name",
            "Flags",
            "LastVisited",
            "Visited",
            "Path",
        ];
        let mirror_titles = vec![
            "RepoID",
            "Remote",
            "Owner",
            "Name",
            "LastVisited",
            "Visited",
        ];
        let result = db.with_transaction(|tx| data.dry_run(tx))?;

        outputln!("{}", style("[New Repositories]").magenta().bold());
        let text = self.table.render(repo_titles.clone(), &result.new_repos)?;
        outputln!("{text}");

        outputln!("{}", style("[Update Repositories]").magenta().bold());
        let text = self
            .table
            .render(repo_titles.clone(), &result.update_repos)?;
        outputln!("{text}");

        outputln!("{}", style("[New Mirrors]").magenta().bold());
        let text = self
            .table
            .render(mirror_titles.clone(), &result.new_mirrors)?;
        outputln!("{text}");

        outputln!("{}", style("[Update Mirrors]").magenta().bold());
        let text = self
            .table
            .render(mirror_titles.clone(), &result.update_mirrors)?;
        outputln!("{text}");

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
