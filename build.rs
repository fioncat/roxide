use std::error::Error;
use vergen::EmitBuilder;

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
    Ok(())
}
