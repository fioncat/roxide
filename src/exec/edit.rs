use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

use crate::config::CmdConfig;
use crate::exec::SilentExit;

pub fn edit<P>(cfg: &CmdConfig, file: P) -> Result<()>
where
    P: AsRef<Path>,
{
    let mut cmd = Command::new(&cfg.name);
    cmd.stdin(Stdio::inherit());
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());

    cmd.args(&cfg.args).arg(file.as_ref());
    match cmd.status() {
        Ok(status) if status.success() => Ok(()),
        Ok(_) => bail!(SilentExit { code: 130 }),
        Err(e) => Err(e).with_context(|| format!("failed to run editor command {:?}", cfg.name)),
    }
}
