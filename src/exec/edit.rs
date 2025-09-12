use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

use crate::config::CmdConfig;
use crate::debug;

pub fn edit<P>(cfg: &CmdConfig, file: P, allow_fail: bool) -> Result<()>
where
    P: AsRef<Path>,
{
    let mut cmd = Command::new(&cfg.name);
    cmd.stdin(Stdio::inherit());
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());

    cmd.args(&cfg.args).arg(file.as_ref());
    match cmd.status() {
        Ok(status) => {
            if !status.success() {
                if allow_fail {
                    debug!("[edit] Editor exited with bad status: {status}, but ignoring");
                    return Ok(());
                }
                bail!("editor exited with bad status: {status}");
            }
            Ok(())
        }
        Err(e) => Err(e).with_context(|| format!("failed to run editor command {:?}", cfg.name)),
    }
}
