use std::{error::Error, process::Command};
use vergen::EmitBuilder;

fn uncommitted_count() -> Result<usize, Box<dyn Error>> {
    let mut cmd = Command::new("git");
    let output = cmd.args(&["status", "-s"]).output()?;
    let output = String::from_utf8(output.stdout)?;
    let lines = output.trim().split("\n");
    Ok(lines.count())
}

fn main() -> Result<(), Box<dyn Error>> {
    // Emit the instructions
    EmitBuilder::builder()
        .rustc_semver()
        .rustc_llvm_version()
        .rustc_channel()
        .rustc_host_triple()
        .build_timestamp()
        .git_sha(false)
        .git_describe(false, true, None)
        .emit()?;

    let uncommitted_count = uncommitted_count()?;
    if uncommitted_count > 0 {
        println!("cargo:rustc-env=ROXIDE_UNCOMMITTED=true");
    } else {
        println!("cargo:rustc-env=ROXIDE_UNCOMMITTED=false");
    }

    Ok(())
}
