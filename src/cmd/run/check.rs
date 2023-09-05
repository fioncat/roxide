use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::Args;
use console::style;
use semver::VersionReq;

use crate::cmd::Run;
use crate::config;
use crate::config::types::Config;
use crate::{term, utils};

/// Check your machine env
#[derive(Args)]
pub struct CheckArgs {}

impl Run for CheckArgs {
    fn run(&self) -> Result<()> {
        let checks: Vec<(&str, fn() -> Result<()>)> = vec![
            ("git", check_git),
            ("fzf", check_fzf),
            ("shell", check_shell),
            ("config", check_config),
        ];

        let mut fail_message = Vec::new();
        for (name, check) in checks {
            print!("Check {name} ... ");
            match check() {
                Ok(_) => println!("{}", style("pass").green()),
                Err(err) => {
                    fail_message.push(format!("{name}: {err}"));
                    println!("{}", style("fail").red());
                }
            }
        }

        println!();
        if !fail_message.is_empty() {
            println!("Check failed:");
            for msg in fail_message {
                println!("  {msg}");
            }
        } else {
            println!("All checks passed");
        }

        Ok(())
    }
}

const REQ_GIT_VERSION: &str = ">=1.20.0";

fn check_git() -> Result<()> {
    let ver = term::git_version()?;
    let req = VersionReq::parse(REQ_GIT_VERSION).unwrap();

    if !req.matches(&ver) {
        bail!("git version too low, require >= 1.20.0");
    }
    Ok(())
}

const REQ_FZF_VERSION: &str = ">=0.40.0";

fn check_fzf() -> Result<()> {
    let ver = term::fzf_version()?;
    let req = VersionReq::parse(REQ_FZF_VERSION).unwrap();

    if !req.matches(&ver) {
        bail!("fzf version too low, require >= 0.40.0");
    }
    Ok(())
}

fn check_shell() -> Result<()> {
    let shell = term::shell_type()?;
    match shell.as_str() {
        "bash" | "zsh" => Ok(()),
        _ => bail!("Unsupport shell {shell}"),
    }
}

fn check_config() -> Result<()> {
    let config_dir = Config::dir()?;
    check_config_dir(config_dir).context("Check config dir")?;

    let workspace = PathBuf::from(&config::base().workspace);
    check_config_dir(workspace).context("Check workspace dir")?;

    let meta_dir = PathBuf::from(&config::base().metadir);
    check_config_dir(meta_dir).context("Check meta dir")?;

    Ok(())
}

fn check_config_dir(path: PathBuf) -> Result<()> {
    let test_file = path.join(".test_roxide_file");
    utils::ensure_dir(&test_file).context("Ensure dir")?;

    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&test_file)
        .context("Open test file")?;

    let test_data = &[1, 2, 3, 4, 5];
    file.write_all(test_data).context("Write test file")?;
    drop(file);

    fs::remove_file(&test_file).context("Remove test file")?;

    Ok(())
}
