use crate::config;

pub struct BuildInfo {
    pub version: &'static str,

    pub rustc_semver: &'static str,
    pub rustc_llvm: &'static str,
    pub rustc_channel: &'static str,

    pub build_type: &'static str,
    pub build_target: &'static str,
    pub build_timestamp: &'static str,

    pub git_commit_hash: &'static str,
}

impl BuildInfo {
    pub fn new() -> BuildInfo {
        let sha = env!("VERGEN_GIT_SHA");
        let cargo_version = env!("CARGO_PKG_VERSION");
        let git_desc = env!("VERGEN_GIT_DESCRIBE");
        let (mut version, mut build_type) = if cargo_version == git_desc {
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
            let short_sha = &sha[..7];
            (format!("{cargo_version}-dev_{short_sha}"), "dev")
        };
        if env!("ROXIDE_UNCOMMITTED") == "true" {
            version = format!("{version}-uncommitted");
            build_type = "dev-uncommitted";
        }
        BuildInfo {
            version: Box::leak(version.into_boxed_str()),

            rustc_semver: env!("VERGEN_RUSTC_SEMVER"),
            rustc_llvm: env!("VERGEN_RUSTC_LLVM_VERSION"),
            rustc_channel: env!("VERGEN_RUSTC_CHANNEL"),

            build_type,
            build_target: env!("VERGEN_RUSTC_HOST_TRIPLE"),
            build_timestamp: env!("VERGEN_BUILD_TIMESTAMP"),

            git_commit_hash: env!("VERGEN_GIT_SHA"),
        }
    }

    pub fn show(&self) {
        println!("roxide {}", self.version);
        println!();
        println!("Build type:   {}", self.build_type);
        println!("Commit SHA:   {}", self.git_commit_hash);
        println!("Build target: {}", self.build_target);
        println!("Build time:   {}", self.build_timestamp);
        println!();
        println!("rustc version:      {}", self.rustc_semver);
        println!("rustc LLVM version: {}", self.rustc_llvm);
        println!("rustc channel:      {}", self.rustc_channel);

        let cfg = config::get();
        let config_dir = format!("{}", cfg.dir.display());
        let meta_dir = format!("{}", cfg.base.metadir);

        println!();
        println!("Config path: {config_dir}");
        println!("Meta path:   {meta_dir}");
    }
}
