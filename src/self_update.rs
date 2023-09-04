use std::io::ErrorKind;
use std::path::PathBuf;
use std::{env, fs};

use anyhow::{bail, Context, Result};

use crate::api::github::Github;
use crate::shell::{self, Shell};
use crate::{config, confirm, info, utils};

pub fn trigger() -> Result<()> {
    let exec_path = env::current_exe().context("Get current exec path")?;
    let exec_dir = match exec_path.parent() {
        Some(dir) => dir,
        None => bail!("Invalid exec path {}", exec_path.display()),
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

    let file_name = target_filename()?;
    let url = format!("https://github.com/fioncat/roxide/releases/latest/download/{file_name}");
    let target_path = "/tmp/roxide-update/roxide.tar.gz";
    shell::download("roxide", url, target_path)?;

    Shell::with_args("tar", &["-xzf", target_path, "-C", "/tmp/roxide-update"])
        .with_desc("Unpack roxide")
        .execute()?
        .check()?;
    let replace_path = format!("{}", exec_dir.join("roxide").display());
    Shell::with_args("mv", &["/tmp/roxide-update/roxide", &replace_path])
        .with_desc("Replace roxide binary")
        .execute()?
        .check()?;

    fs::remove_dir_all("/tmp/roxide-update").context("Remove update tmp dir")?;
    info!("Update roxide to {} done", latest);
    Ok(())
}

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

pub fn auto() -> Result<()> {
    match env::var_os("ROXIDE_AUTO_UPDATE") {
        Some(auto_update_opt) => match auto_update_opt.to_str() {
            Some(opt) => match opt.to_lowercase().as_str() {
                "yes" | "true" => {}
                _ => return Ok(()),
            },
            _ => return Ok(()),
        },
        None => return Ok(()),
    }
    let now = config::now_secs();
    let last_update = get_last_update_time()?;
    let duration = now.saturating_sub(last_update);
    if duration < utils::DAY {
        return Ok(());
    }
    write_last_update_time(now)?;

    trigger()
}

fn get_last_update_time() -> Result<u64> {
    let path = PathBuf::from(&config::base().metadir).join("last_update");
    utils::ensure_dir(&path)?;

    match fs::read(&path) {
        Ok(data) => {
            let time = String::from_utf8(data).context("Decode last_update utf-8")?;
            let time = time
                .parse::<u64>()
                .context("Parse last_update as integer")?;
            Ok(time)
        }
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(0),
        Err(err) => Err(err).with_context(|| format!("Read last_update file {}", path.display())),
    }
}

fn write_last_update_time(now: u64) -> Result<()> {
    let path = PathBuf::from(&config::base().metadir).join("last_update");
    let content = format!("{now}");
    utils::write_file(&path, content.as_bytes()).context("Write last_update file")
}
