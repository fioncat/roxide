mod attach;
mod check;
mod complete;
mod config;
mod create;
mod decrypt;
mod detach;
mod disk_usage;
mod encrypt;
mod export;
mod home;
mod import;
mod list;
mod mirror;
mod open;
mod rebase;
mod remove;
mod run_hook;
mod squash;
mod stats;
mod switch;
mod sync;
mod which;

use std::path::Path;
use std::{env, io};

use anyhow::Result;
use async_trait::async_trait;
use clap::Args;
use clap::error::ErrorKind as ArgsErrorKind;
use clap::{Parser, Subcommand};

use crate::config::context::ConfigContext;
use crate::exec::SilentExit;
use crate::{debug, warn};

#[async_trait]
pub trait Command: Args {
    async fn run(self, ctx: ConfigContext) -> Result<()>;

    fn complete_command() -> clap::Command;
}

#[derive(Parser)]
#[command(bin_name = "rox", author, about, version = env!("ROXIDE_VERSION"))]
pub struct App {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Attach(attach::AttachCommand),
    Check(check::CheckCommand),
    Config(config::ConfigCommand),
    Create(create::CreateCommand),
    Decrypt(decrypt::DecryptCommand),
    Detach(detach::DetachCommand),
    #[command(alias = "du")]
    DiskUsage(disk_usage::DiskUsageCommand),
    Encrypt(encrypt::EncryptCommand),
    Export(export::ExportCommand),
    Home(home::HomeCommand),
    Import(import::ImportCommand),
    #[command(alias = "ls")]
    List(list::ListCommand),
    Mirror(mirror::MirrorCommand),
    Open(open::OpenCommand),
    Rebase(rebase::RebaseCommand),
    #[command(alias = "rm")]
    Remove(remove::RemoveCommand),
    RunHook(run_hook::RunHookCommand),
    Squash(squash::SquashCommand),
    Stats(stats::StatsCommand),
    Switch(switch::SwitchCommand),
    Sync(sync::SyncCommand),
    Which(which::WhichCommand),
}

#[async_trait]
impl Command for App {
    async fn run(self, ctx: ConfigContext) -> Result<()> {
        match self.command {
            Commands::Attach(cmd) => cmd.run(ctx).await,
            Commands::Check(cmd) => cmd.run(ctx).await,
            Commands::Config(cmd) => cmd.run(ctx).await,
            Commands::Create(cmd) => cmd.run(ctx).await,
            Commands::Decrypt(cmd) => cmd.run(ctx).await,
            Commands::Detach(cmd) => cmd.run(ctx).await,
            Commands::DiskUsage(cmd) => cmd.run(ctx).await,
            Commands::Encrypt(cmd) => cmd.run(ctx).await,
            Commands::Export(cmd) => cmd.run(ctx).await,
            Commands::Home(cmd) => cmd.run(ctx).await,
            Commands::Import(cmd) => cmd.run(ctx).await,
            Commands::List(cmd) => cmd.run(ctx).await,
            Commands::Mirror(cmd) => cmd.run(ctx).await,
            Commands::Open(cmd) => cmd.run(ctx).await,
            Commands::Rebase(cmd) => cmd.run(ctx).await,
            Commands::Remove(cmd) => cmd.run(ctx).await,
            Commands::RunHook(cmd) => cmd.run(ctx).await,
            Commands::Squash(cmd) => cmd.run(ctx).await,
            Commands::Stats(cmd) => cmd.run(ctx).await,
            Commands::Switch(cmd) => cmd.run(ctx).await,
            Commands::Sync(cmd) => cmd.run(ctx).await,
            Commands::Which(cmd) => cmd.run(ctx).await,
        }
    }

    fn complete_command() -> clap::Command {
        clap::Command::new("rox")
            .disable_help_flag(true)
            .disable_help_subcommand(true)
            .disable_version_flag(true)
            .subcommands([
                attach::AttachCommand::complete_command(),
                check::CheckCommand::complete_command(),
                config::ConfigCommand::complete_command(),
                create::CreateCommand::complete_command(),
                decrypt::DecryptCommand::complete_command(),
                detach::DetachCommand::complete_command(),
                disk_usage::DiskUsageCommand::complete_command(),
                encrypt::EncryptCommand::complete_command(),
                export::ExportCommand::complete_command(),
                home::HomeCommand::complete_command(),
                import::ImportCommand::complete_command(),
                list::ListCommand::complete_command(),
                mirror::MirrorCommand::complete_command(),
                open::OpenCommand::complete_command(),
                rebase::RebaseCommand::complete_command(),
                remove::RemoveCommand::complete_command(),
                run_hook::RunHookCommand::complete_command(),
                squash::SquashCommand::complete_command(),
                stats::StatsCommand::complete_command(),
                switch::SwitchCommand::complete_command(),
                sync::SyncCommand::complete_command(),
                which::WhichCommand::complete_command(),
            ])
    }
}

#[derive(Debug, Default)]
pub struct CommandResult {
    pub code: u8,
    pub message: Option<String>,
}

pub async fn run() -> CommandResult {
    match complete::register_complete() {
        Ok(true) => return CommandResult::default(),
        Ok(false) => {}
        Err(e) => {
            return CommandResult {
                code: 1,
                message: Some(format!("Failed to generate init script: {e:#}")),
            };
        }
    }

    let warn_wrap = env::var("ROXIDE_WRAP").is_err();
    let app = match App::try_parse() {
        Ok(app) => app,
        Err(err) => {
            if warn_wrap {
                print_wrap_warn();
            }
            err.use_stderr();
            err.print().expect("write help message to stderr");
            if matches!(
                err.kind(),
                ArgsErrorKind::DisplayHelp
                    | ArgsErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
                    | ArgsErrorKind::DisplayVersion
            ) {
                return CommandResult::default();
            }
            return CommandResult {
                code: 2,
                message: None,
            };
        }
    };

    let skip_check_wrap = matches!(app.command, Commands::Check(_));
    if !skip_check_wrap && warn_wrap {
        print_wrap_warn();
    }

    if termion::is_tty(&io::stderr()) {
        // We only print styled message in stderr, so it is safe to enable colors forcibly
        console::set_colors_enabled(true);
    }

    let result = match ConfigContext::setup() {
        Ok(ctx) => app.run(ctx).await,
        Err(e) => Err(e),
    };
    let result = match result {
        Ok(()) => CommandResult::default(),
        Err(err) => match err.downcast::<SilentExit>() {
            Ok(SilentExit { code }) => CommandResult {
                code,
                message: None,
            },
            Err(e) => {
                let message = format!("Error: {e:#}");
                CommandResult {
                    code: 1,
                    message: Some(message),
                }
            }
        },
    };
    debug!("[cmd] Cmd result: {result:?}");
    result
}

pub fn get_shell_type() -> Option<String> {
    let shell = env::var("SHELL").ok()?;
    let name = Path::new(&shell).file_name()?.to_str()?;
    Some(name.to_string())
}

fn print_wrap_warn() {
    match get_shell_type() {
        Some(shell) if shell == "zsh" || shell == "bash" => {
            warn!(
                "Please do not use `roxide` command directly, add `source <(ROXIDE_INIT='{shell}' roxide)` to your profile and use `rox` command instead"
            );
        }
        Some(shell) => {
            warn!("Now we do not support shell {shell:?}, please use `bash` or `zsh` instead")
        }
        None => {
            warn!("Cannot detect your shell type, please make sure you are using `bash` or `zsh`");
        }
    }
}

#[derive(Debug, Args, Clone, Copy)]
pub struct CacheArgs {
    /// Force to not use cache when accessing the remote API. If you are sure the remote
    /// data is updated and want to update the local cache, you can add this flag.
    /// This only affects when the cache is enabled.
    #[arg(long, short)]
    pub force_no_cache: bool,
}

#[derive(Debug, Args, Default, Clone, Copy)]
pub struct UpstreamArgs {
    /// Enable upstream mode. Operations will target the upstream repository instead of the
    /// current repository. Only works with forked remote repositories. For example, when
    /// creating pull requests, they will be created in the upstream repository.
    #[arg(name = "upstream", long = "upstream", short = 'u')]
    pub enable: bool,
}

#[derive(Debug, Args)]
pub struct IgnoreArgs {
    /// Specify files or directories to ignore during operation. Supports multiple entries
    /// and simple wildcard matching. Examples: "*.log", "target", "src/**/test"
    #[arg(name = "ignore", long = "ignore", short = 'I')]
    pub patterns: Option<Vec<String>>,
}

#[derive(Debug, Args, Clone, Copy)]
pub struct ThinArgs {
    /// Add `--depth 1` parameter when cloning repositories for faster cloning of large
    /// repositories. Use this parameter if you only need temporary access to the repository.
    /// Note: recommended for readonly mode only, as features like rebase and squash will not
    /// work with shallow repositories.
    #[arg(name = "thin", long = "thin", short = 't')]
    pub enable: bool,
}
