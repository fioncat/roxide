use std::{env, fs};

use anyhow::{bail, Context, Result};
use clap::Args;

use crate::api::github::Github;
use crate::cmd::Run;
use crate::config::Config;
use crate::term::Cmd;
use crate::{confirm, info, term};

/// If there is a new version, update roxide
#[derive(Args)]
pub struct UpdateArgs {}

impl Run for UpdateArgs {
    fn run(&self, _cfg: &Config) -> Result<()> {
        let exec_path = env::current_exe().context("get current exec path")?;
        let exec_dir = match exec_path.parent() {
            Some(dir) => dir,
            None => bail!("invalid exec path {}", exec_path.display()),
        };

        info!("Checking new version for roxide");
        let api = Github::new_empty();
        let latest = api.get_latest_tag("fioncat", "roxide")?;
        let current = format!("v{}", env!("ROXIDE_VERSION"));
        if latest.as_str() == current {
            info!("Your roxide is up-to-date");
            return Ok(());
        }

        confirm!(
            "Found new version {} for roxide, do you want to update",
            latest
        );

        let file_name = Self::target_filename()?;
        let url = format!("https://github.com/fioncat/roxide/releases/latest/download/{file_name}");
        let target_path = "/tmp/roxide-update/roxide.tar.gz";
        term::download("roxide", url, target_path)?;

        Cmd::with_args("tar", &["-xzf", target_path, "-C", "/tmp/roxide-update"])
            .with_display("Unpack roxide")
            .execute()?;

        let replace_path = format!("{}", exec_dir.join("roxide").display());
        Cmd::with_args("mv", &["/tmp/roxide-update/roxide", &replace_path])
            .with_display("Replace roxide binary")
            .execute()?;

        fs::remove_dir_all("/tmp/roxide-update").context("remove update tmp dir")?;

        info!("Update roxide to {} done", latest);
        Ok(())
    }
}

impl UpdateArgs {
    fn target_filename() -> Result<String> {
        let os = match env::consts::OS {
        "linux" => "unknown-linux-musl",
        "macos" => "apple-darwin",
        _ => bail!("Downloading roxide for os {} from the release page is not supported. Please build roxide manually", env::consts::OS),
    };

        let arch = match env::consts::ARCH {
        "x86_64" => "x86_64",
        "aarch64" => "aarch64",
        _ => bail!("Downloading roxide for arch {} from the release page is not supported. Please build roxide manually", env::consts::ARCH),
    };

        Ok(format!("roxide_{arch}-{os}.tar.gz"))
    }
}
