use std::env;

use anyhow::Result;
use async_trait::async_trait;
use clap::Args;
use console::style;

use crate::config::context::ConfigContext;
use crate::db::repo::{DisplayLevel, QueryOptions};
use crate::{cursor_up, debug, output, outputln};

use super::Command;

#[derive(Debug, Args)]
pub struct CheckCommand {
    #[arg(long, short)]
    pub force_no_cache: bool,
}

#[async_trait]
impl Command for CheckCommand {
    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Run check command: {:?}", self);

        let shell = super::get_shell_type();
        match shell {
            Some(shell) => {
                if shell == "zsh" || shell == "bash" {
                    output!("shell: {}, ", style(&shell).green());
                    if env::var("ROXIDE_WRAP").is_ok() {
                        outputln!("wrap: {}", style("enabled").green());
                    } else {
                        let message = format!(
                            "please add `source <(ROXIDE_INIT='{shell}' roxide)` to your profile and use `rox` command instead"
                        );
                        outputln!(
                            "wrap: {}, {}",
                            style("disabled").red(),
                            style(message).yellow()
                        );
                    }
                } else {
                    outputln!(
                        "shell: {}, {}",
                        style(shell).yellow(),
                        style("now we does not support your shell, please issue us").red()
                    );
                }
            }
            None => {
                outputln!(
                    "shell: {}, {}",
                    style("unknown").red(),
                    style("cannot determine your shell").yellow()
                );
            }
        }

        outputln!();
        outputln!("git version: {}", ctx.get_git_version()?);
        outputln!("fzf version: {}", ctx.get_fzf_version()?);
        outputln!();

        let db = ctx.get_db()?;
        let remotes = db.with_transaction(|tx| tx.repo().query_remotes(None))?;
        if remotes.is_empty() {
            outputln!("No remote to check");
            return Ok(());
        }
        for (i, remote) in remotes.iter().enumerate() {
            let remote_cfg = ctx.cfg.get_remote(&remote.remote)?;
            if remote_cfg.api.is_none() {
                outputln!("{} skip", style(&remote.remote).blue());
                continue;
            }
            outputln!("Checking remote {:?} ...", remote.remote);
            let api = ctx.get_api(&remote.remote, self.force_no_cache)?;
            let info = api.info().await?;
            let ping = if info.ping {
                style("ok").green()
            } else {
                style("failed").red()
            };
            let cache = if info.cache {
                style("enabled").green()
            } else {
                style("disabled").yellow()
            };
            let auth_user = info
                .auth_user
                .as_deref()
                .map(|s| style(s).green())
                .unwrap_or(style("<none>").yellow());
            cursor_up!();
            outputln!(
                "{} ping {ping}, cache {cache}, auth_user {auth_user}",
                style(&remote.remote).blue(),
            );

            let repos = db.with_transaction(|tx| {
                tx.repo().query(QueryOptions {
                    remote: Some(&remote.remote),
                    ..Default::default()
                })
            })?;
            for repo in repos {
                outputln!("- Checking {} ...", repo.full_name());
                let result = api.get_repo(&repo.remote, &repo.owner, &repo.name).await;
                let result = match result {
                    Ok(_) => format!("{}", style("ok").green()),
                    Err(e) => format!("{}: {e:#}", style("failed").red()),
                };
                let name = repo.display_name(DisplayLevel::Owner);
                cursor_up!();
                outputln!("- {name} {result}");
            }
            if i < remotes.len() - 1 {
                outputln!();
            }
        }

        Ok(())
    }

    fn complete_command() -> clap::Command {
        Self::augment_args(clap::Command::new("check"))
    }
}
