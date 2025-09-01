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
