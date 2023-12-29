use anyhow::Result;
use clap::Args;

use crate::cmd::Run;
use crate::config::Config;
use crate::stderrln;

/// Show version info.
#[derive(Args)]
pub struct VersionArgs {}

impl Run for VersionArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
        stderrln!("roxide {}", env!("ROXIDE_VERSION"));
        stderrln!(
            "rustc {}-{}-{}",
            env!("VERGEN_RUSTC_SEMVER"),
            env!("VERGEN_RUSTC_LLVM_VERSION"),
            env!("VERGEN_RUSTC_CHANNEL")
        );
        stderrln!();
        stderrln!("Build type:   {}", env!("ROXIDE_BUILD_TYPE"));
        stderrln!("Build target: {}", env!("ROXIDE_TARGET"));
        stderrln!("Commit SHA:   {}", env!("ROXIDE_SHA"));
        stderrln!("Build time:   {}", env!("VERGEN_BUILD_TIMESTAMP"));

        let cfg_path = match Config::get_path()? {
            Some(path) => format!("{}", path.display()),
            None => "N/A".to_string(),
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
