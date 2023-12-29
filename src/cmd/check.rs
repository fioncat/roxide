use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::Args;
use console::style;
use semver::VersionReq;

use crate::cmd::Run;
use crate::config::Config;
use crate::{stderr, stderrln, term, utils};

/// Check system environment.
#[derive(Args)]
pub struct CheckArgs {}

impl Run for CheckArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
        let checks: Vec<(&str, fn(&Config) -> Result<()>)> = vec![
            ("git", Self::check_git),
            ("fzf", Self::check_fzf),
            ("shell", Self::check_shell),
            ("config", Self::check_config),
        ];

        let mut fail_message = Vec::new();
        for (name, check) in checks {
            stderr!("Check {} ... ", name);
            match check(cfg) {
                Ok(_) => stderrln!("{}", style("pass").green()),
                Err(err) => {
                    fail_message.push(format!("{name}: {err}"));
                    stderrln!("{}", style("fail").red());
                }
            }
        }

        stderrln!();
        if !fail_message.is_empty() {
            stderrln!("Check failed:");
            for msg in fail_message {
                stderrln!("  {}", msg);
            }
        } else {
            stderrln!("All checks passed");
        }

        Ok(())
    }
}

impl CheckArgs {
    const REQ_GIT_VERSION: &'static str = ">=1.20.0";
    const REQ_FZF_VERSION: &'static str = ">=0.40.0";

    fn check_git(_cfg: &Config) -> Result<()> {
        let ver = term::git_version()?;
        let req = VersionReq::parse(Self::REQ_GIT_VERSION).unwrap();

        if !req.matches(&ver) {
            bail!("git version too low, require >= 1.20.0");
        }
        Ok(())
    }

    fn check_fzf(_cfg: &Config) -> Result<()> {
        let ver = term::fzf_version()?;
        let req = VersionReq::parse(Self::REQ_FZF_VERSION).unwrap();

        if !req.matches(&ver) {
            bail!("fzf version too low, require >= 0.40.0");
        }
        Ok(())
    }

    fn check_shell(_cfg: &Config) -> Result<()> {
        let shell = term::shell_type()?;
        match shell.as_str() {
            "bash" | "zsh" => Ok(()),
            _ => bail!("unsupported shell {shell}"),
        }
    }

    fn check_config(cfg: &Config) -> Result<()> {
        let workspace = PathBuf::from(&cfg.get_workspace_dir());
        Self::check_dir(workspace).context("check workspace dir")?;

        let meta_dir = PathBuf::from(&cfg.get_meta_dir());
        Self::check_dir(meta_dir).context("check meta dir")?;

        Ok(())
    }

    fn check_dir(path: PathBuf) -> Result<()> {
        let test_file = path.join(".test_roxide_file");
        utils::ensure_dir(&test_file).context("ensure dir")?;

        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&test_file)
            .context("open test file")?;

        let test_data = &[1, 2, 3, 4, 5];
        file.write_all(test_data).context("write test file")?;
        drop(file);

        fs::remove_file(&test_file).context("remove test file")?;

        Ok(())
    }
}
