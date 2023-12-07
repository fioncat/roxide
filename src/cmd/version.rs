use anyhow::Result;
use clap::Args;

use crate::cmd::Run;
use crate::config::Config;
use crate::stderr;

/// Detach current path in database, don't remove directory
#[derive(Args)]
pub struct VersionArgs {}

impl Run for VersionArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
        stderr!("roxide {}", env!("ROXIDE_VERSION"));
        stderr!(
            "rustc {}-{}-{}",
            env!("VERGEN_RUSTC_SEMVER"),
            env!("VERGEN_RUSTC_LLVM_VERSION"),
            env!("VERGEN_RUSTC_CHANNEL")
        );
        stderr!();
        stderr!("Build type:   {}", env!("ROXIDE_BUILD_TYPE"));
        stderr!("Build target: {}", env!("ROXIDE_TARGET"));
        stderr!("Commit SHA:   {}", env!("ROXIDE_SHA"));
        stderr!("Build time:   {}", env!("VERGEN_BUILD_TIMESTAMP"));

        let cfg_path = match Config::get_path()? {
            Some(path) => format!("{}", path.display()),
            None => format!("N/A"),
        };
        let meta_dir = format!("{}", cfg.get_meta_dir().display());
        let workspace_dir = format!("{}", cfg.get_workspace_dir().display());

        println!();
        println!("Config path: {cfg_path}");
        println!("Meta path:   {meta_dir}");
        println!("Workspace:   {workspace_dir}");

        Ok(())
    }
}
