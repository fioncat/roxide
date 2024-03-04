use anyhow::Result;
use clap::Args;

use crate::cmd::Run;
use crate::config::Config;

/// Show version info.
#[derive(Args)]
pub struct VersionArgs {}

impl Run for VersionArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
        eprintln!("roxide {}", env!("ROXIDE_VERSION"));
        eprintln!(
            "rustc {}-{}-{}",
            env!("VERGEN_RUSTC_SEMVER"),
            env!("VERGEN_RUSTC_LLVM_VERSION"),
            env!("VERGEN_RUSTC_CHANNEL")
        );
        eprintln!();
        eprintln!("Build type:   {}", env!("ROXIDE_BUILD_TYPE"));
        eprintln!("Build target: {}", env!("ROXIDE_TARGET"));
        eprintln!("Commit SHA:   {}", env!("ROXIDE_SHA"));
        eprintln!("Build time:   {}", env!("VERGEN_BUILD_TIMESTAMP"));

        let cfg_path = Config::get_path()?;
        let meta_dir = format!("{}", cfg.get_meta_dir().display());
        let workspace_dir = format!("{}", cfg.get_workspace_dir().display());

        println!();
        println!("Config path: {}", cfg_path.display());
        println!("Meta path:   {meta_dir}");
        println!("Workspace:   {workspace_dir}");

        Ok(())
    }
}
