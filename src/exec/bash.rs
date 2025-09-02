use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::Result;

use crate::config::CmdConfig;
use crate::debug;

use super::Cmd;

static BASH_COMMAND_CONFIG: OnceLock<CmdConfig> = OnceLock::new();

pub fn set_cmd(cfg: CmdConfig) {
    let _ = BASH_COMMAND_CONFIG.set(cfg);
}

#[cfg(test)]
pub fn get_cmd() -> &'static CmdConfig {
    BASH_COMMAND_CONFIG.get().unwrap()
}

pub fn run<S>(path: &str, files: &[S], envs: &[(&str, String)], mute: bool) -> Result<()>
where
    S: AsRef<Path>,
{
    for file in files {
        let file_path = PathBuf::from(file.as_ref());
        let file_name = file_path
            .file_name()
            .map(|s| s.to_str().unwrap_or_default())
            .unwrap_or_default();

        debug!(
            "[bash] Begin to run script {file_name} for {path}, script full path: {}",
            file.as_ref().display()
        );
        let mut cmd = BASH_COMMAND_CONFIG
            .get()
            .map(|cfg| Cmd::new(&cfg.name).args(&cfg.args))
            .unwrap_or(Cmd::new("bash"))
            .args([file.as_ref()])
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

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn test_bash() {
        let base_dir = "tests/bash_playground";
        let _ = fs::remove_dir_all(base_dir);
        fs::create_dir(base_dir).unwrap();

        const SCRIPT0: &str = r#"
        echo $TEST_ENV > test_env
        touch empty_file
        echo "Some content" > some_file_0
        "#;
        let script0_path = "tests/test_script0.sh";
        fs::write(script0_path, SCRIPT0).unwrap();

        const SCRIPT1: &str = r#"
        echo "Another content" > some_file_1
        # The PATH env should be passed to the script
        echo "$PATH" > path_env
        "#;
        let script1_path = "tests/test_script1.sh";
        fs::write(script1_path, SCRIPT1).unwrap();

        let script0_path = fs::canonicalize(script0_path).unwrap();
        let script1_path = fs::canonicalize(script1_path).unwrap();

        let envs = [("TEST_ENV", "Hello, World!".to_string())];

        run(base_dir, &[script0_path, script1_path], &envs, true).unwrap();

        assert_eq!(
            fs::read(format!("{base_dir}/test_env",)).unwrap(),
            b"Hello, World!\n"
        );
        assert_eq!(
            fs::read(format!("{base_dir}/some_file_0")).unwrap(),
            b"Some content\n"
        );
        assert_eq!(
            fs::read(format!("{base_dir}/some_file_1")).unwrap(),
            b"Another content\n"
        );
        assert!(
            fs::read(format!("{base_dir}/empty_file"))
                .unwrap()
                .is_empty()
        );

        let path_env = std::env::var("PATH").unwrap() + "\n";
        assert_eq!(
            fs::read(format!("{base_dir}/path_env")).unwrap(),
            path_env.as_bytes()
        );
    }
}
