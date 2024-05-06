use std::borrow::Cow;
use std::collections::HashMap;
use std::time::Duration;
use std::{env, fs, io};

use anyhow::{Context, Result};
use clap::Args;
use serde::Serialize;
use sysinfo::System;

use crate::cmd::Run;
use crate::config::Config;
use crate::term;
use crate::utils;

/// Show some global info
#[derive(Args)]
pub struct InfoArgs {
    /// Show the output in json format.
    #[clap(short, long)]
    pub json: bool,
}

impl Run for InfoArgs {
    fn run(&self, cfg: &Config) -> Result<()> {
        let info = Info::build(cfg)?;
        term::show_json(info)?;
        Ok(())
    }
}

#[derive(Debug, Serialize)]
struct Info {
    build: BuildInfo,
    system: SystemInfo,
    config: ConfigInfo,
}

#[derive(Debug, Serialize)]
struct BuildInfo {
    version: &'static str,
    #[serde(rename = "type")]
    build_type: &'static str,
    target: &'static str,
    commit: &'static str,
    time: &'static str,
    rust: RustInfo,
    tls_vendored: bool,
    size: String,
}

#[derive(Debug, Serialize)]
struct RustInfo {
    version: &'static str,
    channel: &'static str,
    llvm_version: &'static str,
}

#[derive(Debug, Serialize)]
struct SystemInfo {
    name: Cow<'static, str>,
    uptime: String,
    kernel_version: Cow<'static, str>,
    os_version: Cow<'static, str>,
    cpu: CpuInfo,
    memory: MemoryInfo,
}

#[derive(Debug, Serialize)]
struct CpuInfo {
    brands: Vec<String>,
    arch: Cow<'static, str>,
    total: usize,
}

#[derive(Debug, Serialize)]
struct MemoryInfo {
    total: String,
    used: String,
    free: String,
    available: String,
}

#[derive(Debug, Serialize)]
struct ConfigInfo {
    path: String,
    meta_path: String,
    is_default: bool,
    size: ConfigSizeInfo,
}

#[derive(Debug, Serialize)]
struct ConfigSizeInfo {
    config: String,
    meta: String,
    database: String,
    cache: String,
}

impl Info {
    fn build(cfg: &Config) -> Result<Self> {
        let exec_path = env::current_exe().context("get current exec path")?;
        let exec_meta = fs::metadata(&exec_path)
            .with_context(|| format!("get metadata for current exec '{}'", exec_path.display()))?;

        let build = BuildInfo {
            version: env!("ROXIDE_VERSION"),
            build_type: env!("ROXIDE_BUILD_TYPE"),
            target: env!("ROXIDE_TARGET"),
            commit: env!("ROXIDE_SHA"),
            time: env!("VERGEN_BUILD_TIMESTAMP"),
            rust: RustInfo {
                version: env!("VERGEN_RUSTC_SEMVER"),
                channel: env!("VERGEN_RUSTC_CHANNEL"),
                llvm_version: env!("VERGEN_RUSTC_LLVM_VERSION"),
            },
            tls_vendored: cfg!(feature = "tls-vendored"),
            size: utils::human_bytes(exec_meta.len()),
        };

        let sysinfo = System::new_all();

        let cpus = sysinfo.cpus();
        let total_cpu = cpus.len();
        let mut cpu_brands: HashMap<String, usize> = HashMap::with_capacity(1);
        for cpu in cpus {
            let brand = cpu.brand();
            if let Some(count) = cpu_brands.get_mut(brand) {
                *count += 1;
                continue;
            }
            cpu_brands.insert(String::from(brand), 1);
        }

        let mut cpu_brands: Vec<String> = cpu_brands
            .into_iter()
            .map(|(name, count)| format!("{name} x {count}"))
            .collect();
        cpu_brands.sort_unstable();

        let system = SystemInfo {
            name: Self::option_info(System::name()),
            uptime: utils::format_elapsed(Duration::from_secs(System::uptime())),
            kernel_version: Self::option_info(System::kernel_version()),
            os_version: System::os_version()
                .map(Cow::Owned)
                .unwrap_or(Cow::Borrowed("rolling")),
            cpu: CpuInfo {
                brands: cpu_brands,
                arch: Self::option_info(System::cpu_arch()),
                total: total_cpu,
            },
            memory: MemoryInfo {
                total: utils::human_bytes(sysinfo.total_memory()),
                used: utils::human_bytes(sysinfo.used_memory()),
                free: utils::human_bytes(sysinfo.free_memory()),
                available: utils::human_bytes(sysinfo.available_memory()),
            },
        };

        let config_path = Config::get_path().context("get config path")?;
        let config_size = utils::dir_size(config_path.clone())?;

        let meta_path = cfg.get_meta_dir();
        let meta_size = utils::dir_size(meta_path.clone())?;

        let cache_path = meta_path.join("cache");
        let cache_size = utils::dir_size(cache_path)?;

        let database_path = meta_path.join("database");
        let database_size = match fs::metadata(database_path) {
            Ok(meta) => meta.len(),
            Err(err) if err.kind() == io::ErrorKind::NotFound => 0,
            Err(err) => return Err(err).context("get metadata for database"),
        };

        let config = ConfigInfo {
            path: format!("{}", Config::get_path()?.display()),
            is_default: cfg.is_default,
            meta_path: format!("{}", cfg.get_meta_dir().display()),
            size: ConfigSizeInfo {
                config: utils::human_bytes(config_size),
                meta: utils::human_bytes(meta_size),
                cache: utils::human_bytes(cache_size),
                database: utils::human_bytes(database_size),
            },
        };

        Ok(Self {
            build,
            system,
            config,
        })
    }

    #[inline]
    fn option_info(s: Option<String>) -> Cow<'static, str> {
        s.map(Cow::Owned).unwrap_or(Cow::Borrowed("Unknown"))
    }
}
