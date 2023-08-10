pub struct BuildInfo {
    pub version: &'static str,

    pub rustc_semver: &'static str,
    pub rustc_llvm: &'static str,
    pub rustc_channel: &'static str,

    pub build_target: &'static str,
    pub build_timestamp: &'static str,

    pub git_commit_hash: &'static str,
}

impl BuildInfo {
    pub fn new() -> BuildInfo {
        BuildInfo {
            version: env!("CARGO_PKG_VERSION"),

            rustc_semver: env!("VERGEN_RUSTC_SEMVER"),
            rustc_llvm: env!("VERGEN_RUSTC_LLVM_VERSION"),
            rustc_channel: env!("VERGEN_RUSTC_CHANNEL"),

            build_target: env!("VERGEN_RUSTC_HOST_TRIPLE"),
            build_timestamp: env!("VERGEN_BUILD_TIMESTAMP"),

            git_commit_hash: env!("VERGEN_GIT_SHA"),
        }
    }

    pub fn show(&self) {
        println!("roxide {}", self.version);
        println!();
        println!("Commit SHA:   {}", self.git_commit_hash);
        println!("Build Target: {}", self.build_target);
        println!("Build Time:   {}", self.build_timestamp);
        println!();
        println!("rustc version:      {}", self.rustc_semver);
        println!("rustc LLVM version: {}", self.rustc_llvm);
        println!("rustc channel:      {}", self.rustc_channel);
    }
}
