use std::path::PathBuf;

use anyhow::Result;
use clap::Args;

use crate::cmd::Run;
use crate::config::Config;
use crate::{term, utils};

/// Edit config file in terminal.
#[derive(Args)]
pub struct ConfigArgs {}

impl Run for ConfigArgs {
    fn run(&self, _cfg: &Config) -> Result<()> {
        let path = self.get_path()?;

        let editor = term::get_editor()?;
        term::edit_file(editor, &path)
    }
}

impl ConfigArgs {
    fn get_path(&self) -> Result<PathBuf> {
        if let Some(path) = Config::get_path()? {
            return Ok(path);
        }

        let path = Config::get_raw_path()?;

        let data = include_bytes!("../../config/config.toml");
        utils::write_file(&path, data)?;

        Ok(path)
    }
}
