use std::env;

use anyhow::Result;
use async_trait::async_trait;
use clap::Args;
use console::style;

use crate::cmd::CacheArgs;
use crate::config::context::ConfigContext;
use crate::db::repo::{DisplayLevel, QueryOptions, Repository};
use crate::{cursor_up, debug, output, outputln};

use super::Command;

/// Check system environment and verify roxide functionality.
#[derive(Debug, Args)]
pub struct CheckCommand {
    #[clap(flatten)]
    pub cache: CacheArgs,
}

#[async_trait]
impl Command for CheckCommand {
    async fn run(self, ctx: ConfigContext) -> Result<()> {
        debug!("[cmd] Run check command: {:?}", self);

        show_section("Environment");
        check_os().show();
        check_shell().show();
        outputln!();

        show_section("Commands");
        check_git(&ctx).show();
        check_fzf(&ctx).show();
        check_bash(&ctx).show();
        check_edit(&ctx).show();
        outputln!();

        show_section("Remote API");
        let db = ctx.get_db()?;
        let remotes = db.with_transaction(|tx| tx.repo().query_remotes(None))?;
        if remotes.is_empty() {
            outputln!("No remote to check");
            return Ok(());
        }
        for remote in remotes {
            outputln!("Checking remote {} ...", remote.remote);
            let (item, ok) = check_remote(&ctx, &remote.remote, self.cache.force_no_cache).await;
            cursor_up!();
            item.show();
            if !ok {
                continue;
            }

            let repos = db.with_transaction(|tx| {
                tx.repo().query(QueryOptions {
                    remote: Some(&remote.remote),
                    ..Default::default()
                })
            })?;

            for repo in repos {
                outputln!("- Checking {} ...", repo.display_name(DisplayLevel::Owner));
                let item = check_repo(&ctx, &repo, self.cache.force_no_cache).await;
                cursor_up!();
                item.show();
            }
        }

        Ok(())
    }

    fn complete_command() -> clap::Command {
        clap::Command::new("check")
    }
}

struct CheckItem {
    status: CheckStatus,
    message: String,
    sub: bool,
}

enum CheckStatus {
    Ok,
    Warn,
    Failed,
}

impl CheckItem {
    fn show(&self) {
        if self.sub {
            output!("- ");
        }
        match self.status {
            CheckStatus::Ok => output!("{}", style("[ok]").green().bold()),
            CheckStatus::Warn => output!("{}", style("[warn]").yellow().bold()),
            CheckStatus::Failed => output!("{}", style("[failed]").red()),
        }
        outputln!(" {}", self.message);
    }
}

fn show_section(section: &str) {
    outputln!("{}", style(format!("[{section}]")).bold());
}

fn check_os() -> CheckItem {
    let os = env::consts::OS;
    if os != "linux" && os != "macos" {
        return CheckItem {
            status: CheckStatus::Failed,
            message: format!("does not support os {}", style(os).blue()),
            sub: false,
        };
    }
    CheckItem {
        status: CheckStatus::Ok,
        message: format!("os: {os}"),
        sub: false,
    }
}

fn check_shell() -> CheckItem {
    let Some(shell) = super::get_shell_type() else {
        return CheckItem {
            status: CheckStatus::Failed,
            message: "cannot determine your shell".to_string(),
            sub: false,
        };
    };

    if shell != "zsh" && shell != "bash" {
        return CheckItem {
            status: CheckStatus::Failed,
            message: format!("does not support shell {}", style(shell).blue()),
            sub: false,
        };
    }

    if env::var("ROXIDE_WRAP").is_err() {
        return CheckItem {
            status: CheckStatus::Warn,
            message: format!(
                "NOT IN wrap mode, please add {} to your profile and use {} command instead",
                style(format!("source <(ROXIDE_INIT='{shell}' roxide)")).blue(),
                style("rox").blue()
            ),
            sub: false,
        };
    }

    CheckItem {
        status: CheckStatus::Ok,
        message: format!("shell: {shell}"),
        sub: false,
    }
}

fn check_git(ctx: &ConfigContext) -> CheckItem {
    match ctx.get_git_version() {
        Ok(version) => CheckItem {
            status: CheckStatus::Ok,
            message: format!("git version: {version}"),
            sub: false,
        },
        Err(e) => CheckItem {
            status: CheckStatus::Failed,
            message: format!("cannot get git version: {e:#}"),
            sub: false,
        },
    }
}

fn check_fzf(ctx: &ConfigContext) -> CheckItem {
    match ctx.get_fzf_version() {
        Ok(version) => CheckItem {
            status: CheckStatus::Ok,
            message: format!("fzf version: {version}"),
            sub: false,
        },
        Err(e) => CheckItem {
            status: CheckStatus::Failed,
            message: format!("cannot get fzf version: {e:#}"),
            sub: false,
        },
    }
}

fn check_bash(ctx: &ConfigContext) -> CheckItem {
    match which::which(&ctx.cfg.bash.name) {
        Ok(path) => CheckItem {
            status: CheckStatus::Ok,
            message: format!("bash: {}", path.display()),
            sub: false,
        },
        Err(e) => CheckItem {
            status: CheckStatus::Failed,
            message: format!("cannot find bash command {}: {e:#}", ctx.cfg.bash.name),
            sub: false,
        },
    }
}

fn check_edit(ctx: &ConfigContext) -> CheckItem {
    match which::which(&ctx.cfg.edit.name) {
        Ok(path) => CheckItem {
            status: CheckStatus::Ok,
            message: format!("editor: {}", path.display()),
            sub: false,
        },
        Err(e) => CheckItem {
            status: CheckStatus::Failed,
            message: format!("cannot find edit command {}: {e:#}", ctx.cfg.edit.name),
            sub: false,
        },
    }
}

async fn check_remote(
    ctx: &ConfigContext,
    remote: &str,
    force_no_cache: bool,
) -> (CheckItem, bool) {
    let Ok(api) = ctx.get_api(remote, force_no_cache) else {
        return (
            CheckItem {
                status: CheckStatus::Ok,
                message: format!("{remote} skip"),
                sub: false,
            },
            false,
        );
    };

    let info = match api.info().await {
        Ok(info) => info,
        Err(e) => {
            return (
                CheckItem {
                    status: CheckStatus::Failed,
                    message: format!("cannot get remote info: {e:#}"),
                    sub: false,
                },
                false,
            );
        }
    };

    if !info.ping {
        return (
            CheckItem {
                status: CheckStatus::Failed,
                message: format!("cannot connect to {remote}"),
                sub: false,
            },
            false,
        );
    }

    let cache = if info.cache {
        style("enabled").green()
    } else {
        style("disabled").yellow()
    };
    let auth_user = info
        .auth_user
        .as_deref()
        .map(|s| style(s).green())
        .unwrap_or(style("<unknown>").yellow());

    (
        CheckItem {
            status: CheckStatus::Ok,
            message: format!("{remote}: cache {cache}, auth_user {auth_user}",),
            sub: false,
        },
        true,
    )
}

async fn check_repo(ctx: &ConfigContext, repo: &Repository, force_no_cache: bool) -> CheckItem {
    let api = match ctx.get_api(&repo.remote, force_no_cache) {
        Ok(api) => api,
        Err(e) => {
            return CheckItem {
                status: CheckStatus::Failed,
                message: format!("cannot get API for remote {}: {e:#}", repo.remote),
                sub: true,
            };
        }
    };

    let name = repo.display_name(DisplayLevel::Owner);
    match api.get_repo(&repo.remote, &repo.owner, &repo.name).await {
        Ok(_) => CheckItem {
            status: CheckStatus::Ok,
            message: name,
            sub: true,
        },
        Err(e) => CheckItem {
            status: CheckStatus::Failed,
            message: format!("{name}: {e:#}"),
            sub: true,
        },
    }
}
