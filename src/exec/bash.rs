use std::borrow::Cow;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::config::CmdConfig;
use crate::debug;

pub fn run<P, F>(
    cfg: &CmdConfig,
    path: P,
    file: F,
    envs: &[(&str, Cow<str>)],
    message: impl ToString,
    mute: bool,
) -> Result<()>
where
    P: AsRef<Path>,
    F: AsRef<Path>,
{
    let file_path = PathBuf::from(file.as_ref());
    let file_name = file_path
        .file_name()
        .map(|s| s.to_str().unwrap_or_default())
        .unwrap_or_default();

    debug!(
        "[bash] Begin to run bash {file_name} for {}, full path: {}",
        path.as_ref().display(),
        file.as_ref().display()
    );
    let mut cmd = cfg
        .new_cmd()
        .args([file.as_ref()])
        .current_dir(path.as_ref());
    if mute {
        cmd = cmd.mute();
    } else {
        cmd = cmd.message(message);
    }
    for (k, v) in envs {
        cmd = cmd.env(k, v);
    }

    cmd.execute()?;
    debug!(
        "[bash] Bash {file_name} for {} finished",
        path.as_ref().display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::config::Config;

    use super::*;

    #[test]
    fn test_bash() {
        let base_dir = "tests/bash_playground";
        let _ = fs::remove_dir_all(base_dir);
        fs::create_dir(base_dir).unwrap();

        const BASH0: &str = r#"
        echo $TEST_ENV > test_env
        touch empty_file
        echo "Some content" > some_file_0
        "#;
        let bash0_path = "tests/test_bash0.sh";
        fs::write(bash0_path, BASH0).unwrap();

        const BASH1: &str = r#"
        echo "Another content" > some_file_1
        # The PATH env should be passed
        echo "$PATH" > path_env
        "#;
        let bash1_path = "tests/test_bash1.sh";
        fs::write(bash1_path, BASH1).unwrap();

        let bash0_path = fs::canonicalize(bash0_path).unwrap();
        let bash1_path = fs::canonicalize(bash1_path).unwrap();

        let envs = [("TEST_ENV", Cow::Borrowed("Hello, World!"))];

        let cfg = Config::default_bash();
        run(&cfg, base_dir, bash0_path, &envs, "Test", true).unwrap();
        run(&cfg, base_dir, bash1_path, &envs, "Test", true).unwrap();

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
