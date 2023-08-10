use std::{error::Error, process::Command};
use vergen::EmitBuilder;

fn uncommitted_count() -> Result<usize, Box<dyn Error>> {
    let mut cmd = Command::new("git");
    let output = cmd.args(&["status", "-s"]).output()?;
    let output = String::from_utf8(output.stdout)?;
    let lines = output.trim().split("\n");
    Ok(lines.filter(|line| !line.trim().is_empty()).count())
}

fn exec_git(args: &[&str]) -> Result<String, Box<dyn Error>> {
    let mut cmd = Command::new("git");
    let output = cmd.args(args).output()?;
    let output = String::from_utf8(output.stdout)?;
    Ok(output.trim().to_string())
}

fn main() -> Result<(), Box<dyn Error>> {
    // Emit the instructions
    EmitBuilder::builder()
        .rustc_semver()
        .rustc_llvm_version()
        .rustc_channel()
        .rustc_host_triple()
        .build_timestamp()
        // .git_sha(false)
        // .git_describe(false, true, None)
        .emit()?;

    let descibe = exec_git(&["descibe", "--tags"])?;
    let sha = exec_git(&["rev-parse", "HEAD"])?;
    let short_sha = exec_git(&["rev-parse", "--short", "HEAD"])?;

    let cargo_version = env!("CARGO_PKG_VERSION");
    let stable_tag = format!("v{cargo_version}");
    let (mut version, mut build_type) = if stable_tag == descibe {
        if cargo_version.ends_with("alpha") {
            (cargo_version.to_string(), "alpha")
        } else if cargo_version.ends_with("beta") {
            (cargo_version.to_string(), "beta")
        } else if cargo_version.ends_with("rc") {
            (cargo_version.to_string(), "pre-release")
        } else {
            (cargo_version.to_string(), "stable")
        }
    } else {
        (format!("{cargo_version}-dev_{short_sha}"), "dev")
    };

    let uncommitted_count = uncommitted_count()?;
    if uncommitted_count > 0 {
        version = format!("{version}-uncommitted");
        build_type = "dev-uncommitted";
    }

    println!("cargo:rustc-env=ROXIDE_VERSION={version}");
    println!("cargo:rustc-env=ROXIDE_BUILD_TYPE={build_type}");
    println!("cargo:rustc-env=ROXIDE_SHA={sha}");

    Ok(())
}
