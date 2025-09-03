pub mod branch;
pub mod commit;
pub mod remote;
pub mod tag;

use std::ffi::OsStr;
use std::path::Path;
use std::sync::OnceLock;

use crate::config::CmdConfig;

use super::Cmd;

static GIT_COMMAND_CONFIG: OnceLock<CmdConfig> = OnceLock::new();

pub fn set_cmd(cfg: CmdConfig) {
    let _ = GIT_COMMAND_CONFIG.set(cfg);
}

#[cfg(test)]
pub fn get_cmd() -> &'static CmdConfig {
    GIT_COMMAND_CONFIG.get().unwrap()
}

pub fn new<I, S, P>(args: I, path: Option<P>, message: impl ToString, mute: bool) -> Cmd
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
    P: AsRef<Path>,
{
    let mut git = GIT_COMMAND_CONFIG
        .get()
        .map(|cfg| Cmd::new(&cfg.name).args(&cfg.args))
        .unwrap_or(Cmd::new("git"))
        .args(args);
    if let Some(p) = path {
        git = git.current_dir(p)
    }
    if mute {
        git.mute()
    } else {
        git.message(message)
    }
}

#[cfg(test)]
pub mod tests {
    use std::{env, fs};

    use super::*;

    pub fn enable() -> bool {
        env::var("TEST_GIT").is_ok_and(|v| v == "true")
    }

    pub fn setup() -> Option<&'static str> {
        if !enable() {
            return None;
        }

        let repo_path = "tests/roxide_git";
        if fs::metadata(repo_path).is_ok() {
            return Some(repo_path);
        }

        None
    }

    #[test]
    fn test_git() {
        let Some(repo_path) = setup() else {
            return;
        };

        let branch = new(["branch", "--show-current"], Some(repo_path), "", true)
            .output()
            .unwrap();
        assert_eq!(branch, "main");
    }
}
