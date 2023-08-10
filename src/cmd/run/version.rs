use anyhow::Result;
use clap::Args;

use crate::cmd::Run;
use crate::config;

/// Show version.
#[derive(Args)]
pub struct VersionArgs {}

impl Run for VersionArgs {
    fn run(&self) -> Result<()> {
        println!("roxide {}", env!("ROXIDE_VERSION"));
        println!();
        println!("Build type:   {}", env!("ROXIDE_BUILD_TYPE"));
        println!("Commit SHA:   {}", env!("ROXIDE_SHA"));
        println!("Build target: {}", env!("VERGEN_RUSTC_HOST_TRIPLE"));
        println!("Build time:   {}", env!("VERGEN_BUILD_TIMESTAMP"));
        println!();
        println!("rustc version:      {}", env!("VERGEN_RUSTC_SEMVER"));
        println!("rustc LLVM version: {}", env!("VERGEN_RUSTC_LLVM_VERSION"));
        println!("rustc channel:      {}", env!("VERGEN_RUSTC_CHANNEL"));

        let cfg = config::get();
        let config_dir = format!("{}", cfg.dir.display());
        let meta_dir = format!("{}", cfg.base.metadir);

        println!();
        println!("Config path: {config_dir}");
        println!("Meta path:   {meta_dir}");
        Ok(())
    }
}
