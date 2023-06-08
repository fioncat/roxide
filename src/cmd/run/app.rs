use anyhow::Result;

use crate::cmd::app::{App, Commands};
use crate::cmd::Run;

impl Run for App {
    fn run(&self) -> Result<()> {
        match &self.command {
            Commands::Home(args) => args.run(),
            Commands::Complete(args) => args.run(),
        }
    }
}
