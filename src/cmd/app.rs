use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::cmd::run::attach::AttachArgs;
use crate::cmd::run::branch::BranchArgs;
use crate::cmd::run::complete::CompleteArgs;
use crate::cmd::run::config::ConfigArgs;
use crate::cmd::run::detach::DetachArgs;
use crate::cmd::run::get::GetArgs;
use crate::cmd::run::home::HomeArgs;
use crate::cmd::run::init::InitArgs;
use crate::cmd::run::merge::MergeArgs;
use crate::cmd::run::open::OpenArgs;
use crate::cmd::run::rebase::RebaseArgs;
use crate::cmd::run::release::ReleaseArgs;
use crate::cmd::run::remove::RemoveArgs;
use crate::cmd::run::squash::SquashArgs;
use crate::cmd::run::tag::TagArgs;
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
    Merge(MergeArgs),
    Remove(RemoveArgs),
    Detach(DetachArgs),
    Config(ConfigArgs),
    Get(GetArgs),
    Rebase(RebaseArgs),
    Squash(SquashArgs),
    Tag(TagArgs),
    Release(ReleaseArgs),
    Open(OpenArgs),
}

impl Run for App {
    fn run(&self) -> Result<()> {
        match &self.command {
            Commands::Init(args) => args.run(),
            Commands::Home(args) => args.run(),
            Commands::Complete(args) => args.run(),
            Commands::Attach(args) => args.run(),
            Commands::Branch(args) => args.run(),
            Commands::Merge(args) => args.run(),
            Commands::Remove(args) => args.run(),
            Commands::Detach(args) => args.run(),
            Commands::Config(args) => args.run(),
            Commands::Get(args) => args.run(),
            Commands::Rebase(args) => args.run(),
            Commands::Squash(args) => args.run(),
            Commands::Tag(args) => args.run(),
            Commands::Release(args) => args.run(),
            Commands::Open(args) => args.run(),
        }
    }
}
