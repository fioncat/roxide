use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

use crate::config::context::ConfigContext;

pub fn get_password(ctx: &ConfigContext, update: bool) -> Result<String> {
    let path = Path::new(&ctx.cfg.data_dir).join("password");
    if !update && path.exists() {
        return fs::read_to_string(path).context("failed to read password file");
    }
    let password = input_password()?;
    let sha256 = format!("{:x}", Sha256::digest(password.as_bytes()));
    fs::write(&path, &sha256).context("failed to write password file")?;
    Ok(sha256)
}

fn input_password() -> Result<String> {
    if cfg!(test) {
        return Ok("test_password".to_string());
    }

    let password = rpassword::prompt_password("Input password: ")?;
    if password.is_empty() {
        bail!("password cannot be empty");
    }

    let confirm = rpassword::prompt_password("Confirm password: ")?;
    if password != confirm {
        bail!("passwords do not match");
    }

    Ok(password)
}
