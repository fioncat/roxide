use anyhow::Result;
use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::cmd::Run;

#[derive(Parser)]
#[command(author, version, about)]
pub struct App {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Init(InitArgs),
    Home(HomeArgs),
    Complete(CompleteArgs),
}

#[derive(Args)]
pub struct HomeArgs {
    pub query: Vec<String>,

    #[clap(long, short)]
    pub search: bool,
}

#[derive(Args)]
pub struct CompleteArgs {
    pub args: Vec<String>,
}

#[derive(Args)]
pub struct InitArgs {
    pub shell: Shell,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum Shell {
    Zsh,
}

impl Run for App {
    fn run(&self) -> Result<()> {
        match &self.command {
            Commands::Init(args) => args.run(),
            Commands::Home(args) => args.run(),
            Commands::Complete(args) => args.run(),
        }
    }
}
