use std::env;

use console::style;

use crate::cmd::get_shell_type;
use crate::config::context::ConfigContext;
use crate::db::repo::{DisplayLevel, Repository};
use crate::{output, outputln};

pub struct CheckItem {
    status: CheckStatus,
    message: String,
    sub: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    Ok,
    Warn,
    Failed,
}

impl CheckItem {
    pub fn show(&self) {
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

pub fn check_os() -> CheckItem {
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

pub fn check_shell() -> CheckItem {
    let Some(shell) = get_shell_type() else {
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

pub fn check_git(ctx: &ConfigContext) -> CheckItem {
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

pub fn check_fzf(ctx: &ConfigContext) -> CheckItem {
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

pub fn check_bash(ctx: &ConfigContext) -> CheckItem {
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

pub fn check_edit(ctx: &ConfigContext) -> CheckItem {
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

pub async fn check_remote(
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

pub async fn check_repo(ctx: &ConfigContext, repo: &Repository, force_no_cache: bool) -> CheckItem {
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

#[cfg(test)]
mod tests {
    use crate::config::context;

    use super::*;

    #[test]
    fn test_check_os() {
        assert_eq!(check_os().status, CheckStatus::Ok);
    }

    #[test]
    fn test_check_shell() {
        unsafe {
            env::set_var("SHELL", "/bin/zsh");
        }
        assert_eq!(check_shell().status, CheckStatus::Warn);

        unsafe {
            env::set_var("ROXIDE_WRAP", "1");
        }
        assert_eq!(check_shell().status, CheckStatus::Ok);

        unsafe {
            env::set_var("SHELL", "/bin/xxx");
        }
        assert_eq!(check_shell().status, CheckStatus::Failed);
    }

    #[test]
    fn test_check_git() {
        let ctx = context::tests::build_test_context("test_git");
        assert_eq!(check_git(&ctx).status, CheckStatus::Ok);
    }

    #[test]
    fn test_check_fzf() {
        let ctx = context::tests::build_test_context("test_fzf");
        assert_eq!(check_fzf(&ctx).status, CheckStatus::Ok);
    }

    #[test]
    fn test_check_bash() {
        let ctx = context::tests::build_test_context("test_bash");
        assert_eq!(check_bash(&ctx).status, CheckStatus::Ok);
    }

    #[test]
    fn test_check_edit() {
        unsafe {
            env::set_var("EDITOR", "xxx");
        }
        let ctx = context::tests::build_test_context("test_edit");
        assert_eq!(check_edit(&ctx).status, CheckStatus::Failed);
    }

    #[tokio::test]
    async fn test_check_remote() {
        let ctx = context::tests::build_test_context("test_check_remote");
        let (item, ok) = check_remote(&ctx, "github", false).await;
        assert!(ok);
        assert_eq!(item.status, CheckStatus::Ok);

        let (item, ok) = check_remote(&ctx, "test", false).await;
        assert!(!ok);
        assert_eq!(item.status, CheckStatus::Ok);
    }

    #[tokio::test]
    async fn test_check_repo() {
        let ctx = context::tests::build_test_context("test_check_repo");
        assert_eq!(
            check_repo(
                &ctx,
                &Repository {
                    remote: "github".to_string(),
                    owner: "fioncat".to_string(),
                    name: "roxide".to_string(),
                    ..Default::default()
                },
                false,
            )
            .await
            .status,
            CheckStatus::Ok
        );
    }
}
