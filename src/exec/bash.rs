use std::{path::PathBuf, sync::OnceLock};

use anyhow::Result;

use crate::{config::CmdConfig, debug};

use super::Cmd;

static BASH_COMMAND_CONFIG: OnceLock<CmdConfig> = OnceLock::new();

pub fn set_command(cfg: CmdConfig) {
    let _ = BASH_COMMAND_CONFIG.set(cfg);
}

pub fn run<S>(path: &str, files: &[S], envs: &[(&str, String)], mute: bool) -> Result<()>
where
    S: AsRef<str>,
{
    for file in files {
        let file_path = PathBuf::from(file.as_ref());
        let file_name = file_path
            .file_name()
            .map(|s| s.to_str().unwrap_or_default())
            .unwrap_or_default();

        debug!(
            "[bash] Begin to run script {file_name} for {path}, script full path: {}",
            file.as_ref()
        );
        let mut cmd = BASH_COMMAND_CONFIG
            .get()
            .map(|cfg| Cmd::new(&cfg.name).args(&cfg.args))
            .unwrap_or(Cmd::new("bash"))
            .args(["-c", file.as_ref()])
            .current_dir(path);
        if mute {
            cmd = cmd.mute();
        } else {
            let desc = format!("Run script: `{file_name}`");
            cmd = cmd.message(desc);
        }
        for (k, v) in envs {
            cmd = cmd.env(k, v);
        }

        cmd.execute()?;
        debug!("[bash] Script {file_name} for {path} finished");
    }
    Ok(())
}
