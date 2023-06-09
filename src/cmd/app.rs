use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::cmd::run::attach::AttachArgs;
use crate::cmd::run::branch::BranchArgs;
use crate::cmd::run::complete::CompleteArgs;
use crate::cmd::run::home::HomeArgs;
use crate::cmd::run::init::InitArgs;
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
    Attach(AttachArgs),
    Branch(BranchArgs),
}

impl Run for App {
    fn run(&self) -> Result<()> {
        match &self.command {
            Commands::Init(args) => args.run(),
            Commands::Home(args) => args.run(),
            Commands::Complete(args) => args.run(),
            Commands::Attach(args) => args.run(),
            Commands::Branch(args) => args.run(),
        }
    }
}
