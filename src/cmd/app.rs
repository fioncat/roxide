use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use strum::EnumVariantNames;

use crate::cmd::complete::CompleteArgs;
use crate::cmd::run::attach::AttachArgs;
use crate::cmd::run::branch::BranchArgs;
use crate::cmd::run::config::ConfigArgs;
use crate::cmd::run::detach::DetachArgs;
use crate::cmd::run::gc::GcArgs;
use crate::cmd::run::get::GetArgs;
use crate::cmd::run::home::HomeArgs;
use crate::cmd::run::import::ImportArgs;
use crate::cmd::run::init::InitArgs;
use crate::cmd::run::merge::MergeArgs;
use crate::cmd::run::open::OpenArgs;
use crate::cmd::run::rebase::RebaseArgs;
use crate::cmd::run::recover::RecoverArgs;
use crate::cmd::run::release::ReleaseArgs;
use crate::cmd::run::remove::RemoveArgs;
use crate::cmd::run::reset::ResetArgs;
use crate::cmd::run::run::RunArgs;
use crate::cmd::run::snapshot::SnapshotArgs;
use crate::cmd::run::squash::SquashArgs;
use crate::cmd::run::sync::SyncArgs;
use crate::cmd::run::tag::TagArgs;
use crate::cmd::run::update::UpdateArgs;
use crate::cmd::run::version::VersionArgs;
use crate::cmd::Run;
use crate::self_update;

#[derive(Parser)]
#[command(author, version = env!("ROXIDE_VERSION"), about)]
pub struct App {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, EnumVariantNames)]
#[strum(serialize_all = "kebab-case")]
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
    Reset(ResetArgs),
    Update(UpdateArgs),
    Import(ImportArgs),
    Run(RunArgs),
    Sync(SyncArgs),
    Snapshot(SnapshotArgs),
    Version(VersionArgs),
    Gc(GcArgs),
    Recover(RecoverArgs),
}

impl Run for App {
    fn run(&self) -> Result<()> {
        match &self.command {
            Commands::Update(_) | Commands::Init(_) | Commands::Complete(_) => {}
            _ => self_update::auto().context("Check auto self-update")?,
        }
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
            Commands::Reset(args) => args.run(),
            Commands::Update(args) => args.run(),
            Commands::Import(args) => args.run(),
            Commands::Run(args) => args.run(),
            Commands::Sync(args) => args.run(),
            Commands::Snapshot(args) => args.run(),
            Commands::Version(args) => args.run(),
            Commands::Gc(args) => args.run(),
            Commands::Recover(args) => args.run(),
        }
    }
}
