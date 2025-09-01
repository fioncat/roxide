use std::ffi::OsStr;
use std::sync::OnceLock;

use crate::config::CmdConfig;

use super::Cmd;

static GIT_COMMAND_CONFIG: OnceLock<CmdConfig> = OnceLock::new();

pub fn set_cmd(cfg: CmdConfig) {
    let _ = GIT_COMMAND_CONFIG.set(cfg);
}

pub fn new<I, S>(args: I) -> Cmd
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    GIT_COMMAND_CONFIG
        .get()
        .map(|cfg| Cmd::new(&cfg.name).args(&cfg.args))
        .unwrap_or(Cmd::new("git"))
        .args(args)
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;

    use super::*;

    #[test]
    fn test_git() {
        if !env::var("TEST_GIT").is_ok_and(|v| v == "true") {
            return;
        }

        let repo_path = "tests/roxide_git";
        let _ = fs::remove_dir_all(repo_path);
        let mut git = new(["clone", "https://github.com/fioncat/roxide.git", repo_path]).mute();
        git.execute().unwrap();

        let mut git = new(["branch", "--show-current"])
            .current_dir(repo_path)
            .mute();
        let branch = git.output().unwrap();
        assert_eq!(branch, "main");
    }
}
