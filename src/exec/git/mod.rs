pub mod branch;
pub mod commit;
pub mod remote;
pub mod tag;

use std::borrow::Cow;
use std::ffi::OsStr;
use std::path::Path;

use anyhow::Result;

use crate::config::CmdConfig;
use crate::exec::Cmd;

#[derive(Debug, Clone, Copy)]
pub struct GitCmd<'a> {
    cmd_cfg: &'a CmdConfig,
    work_dir: &'a Path,
    mute: bool,
}

impl<'a> GitCmd<'a> {
    pub fn new(cfg: &'a CmdConfig, work_dir: &'a Path, mute: bool) -> Self {
        Self {
            cmd_cfg: cfg,
            work_dir,
            mute,
        }
    }

    pub fn execute<I, S>(&self, args: I, message: impl ToString) -> Result<()>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.cmd(args, message).execute()
    }

    pub fn lines<I, S>(&self, args: I, message: impl ToString) -> Result<Vec<String>>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.cmd(args, message).lines()
    }

    pub fn output<I, S>(&self, args: I, message: impl ToString) -> Result<String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.cmd(args, message).output()
    }

    pub fn cmd<I, S>(&self, args: I, message: impl ToString) -> Cmd
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let cmd = self
            .cmd_cfg
            .new_cmd()
            .args(args)
            .message(message)
            .current_dir(&self.work_dir);
        if self.mute {
            return cmd.mute();
        }
        cmd
    }
}

const MAX_COMMIT_MESSAGE_LEN: usize = 40;

fn short_message<'a>(message: &'a str) -> Cow<'a, str> {
    if message.len() > MAX_COMMIT_MESSAGE_LEN {
        format!("{}...", &message[..MAX_COMMIT_MESSAGE_LEN]).into()
    } else {
        Cow::Borrowed(message)
    }
}

#[cfg(test)]
pub mod tests {
    use std::path::PathBuf;
    use std::{env, fs};

    use crate::config::context::ConfigContext;

    pub fn enable() -> bool {
        env::var("TEST_GIT").is_ok_and(|v| v == "true")
    }

    pub fn setup() -> Option<PathBuf> {
        if !enable() {
            return None;
        }

        let repo_path = "tests/roxide_git";
        let repo_path = fs::canonicalize(repo_path).ok()?;
        Some(repo_path)
    }

    #[test]
    fn test_git() {
        let Some(repo_path) = setup() else {
            return;
        };
        let ctx = ConfigContext::new_mock();
        let cmd = ctx.git_work_dir(&repo_path);

        let branch = cmd.output(["branch", "--show-current"], "").unwrap();
        assert_eq!(branch, "main");
    }
}
