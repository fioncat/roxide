use std::io::ErrorKind;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::{env, fs};

use anyhow::{bail, Context, Result};
use clap::{Args, ValueEnum};

use crate::cmd::Run;
use crate::config::types::Config;

/// Edit roxide config file in terminal.
#[derive(Args)]
pub struct ConfigArgs {
    /// If provided, edit remote config file.
    pub remote: Option<String>,

    /// The editor to use, default will use `EDITOR` env.
    #[clap(long, short)]
    pub editor: Option<Editor>,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum Editor {
    Nvim,
    Vim,
    Vi,
}

impl ToString for Editor {
    fn to_string(&self) -> String {
        match self {
            Editor::Nvim => String::from("nvim"),
            Editor::Vim => String::from("vim"),
            Editor::Vi => String::from("vi"),
        }
    }
}

impl Run for ConfigArgs {
    fn run(&self) -> Result<()> {
        let editor = self.get_editor()?;
        let path = self.get_path()?;
        let path = format!("{}", path.display());

        let mut cmd = Command::new(&editor);
        cmd.stdout(Stdio::inherit());
        cmd.stderr(Stdio::inherit());
        cmd.stdin(Stdio::inherit());
        cmd.arg(&path);
        cmd.output()
            .with_context(|| format!("Use editor {editor} to edit config {path}"))?;

        Ok(())
    }
}

impl ConfigArgs {
    fn get_editor(&self) -> Result<String> {
        if let Some(editor) = &self.editor {
            return Ok(editor.to_string());
        }
        let editor = env::var("EDITOR").context("Get EDITOR env")?;
        if editor.is_empty() {
            bail!("Could not get your default editor, please check env `EDITOR`");
        }
        Ok(editor)
    }

    fn get_path(&self) -> Result<PathBuf> {
        let dir = Config::dir()?;

        let name = match &self.remote {
            Some(remote) => remote.as_str(),
            None => "config",
        };

        let yml_name = format!("{name}.yml");
        let yml_path = dir.join(&yml_name);
        if self.path_exists(&yml_path)? {
            return Ok(yml_path);
        }

        let yaml_name = format!("{name}.yaml");
        let yaml_path = dir.join(&yaml_name);
        if self.path_exists(&yaml_path)? {
            return Ok(yaml_path);
        }

        Ok(dir.join(yml_name))
    }

    fn path_exists(&self, path: &PathBuf) -> Result<bool> {
        match fs::read(path) {
            Ok(_) => Ok(true),
            Err(err) if err.kind() == ErrorKind::NotFound => Ok(false),
            Err(err) => Err(err).with_context(|| format!("Read file {}", path.display())),
        }
    }
}
