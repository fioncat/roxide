pub mod default;
pub mod types;

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;

use crate::config::types::{Base, Config, Remote};
use crate::utils;

static mut CONFIG: Option<Config> = None;

pub fn get() -> &'static Config {
    unsafe {
        if let None = CONFIG {
            CONFIG = match Config::read() {
                Ok(cfg) => Some(cfg),
                Err(err) => {
                    utils::error_exit(err.context("Init config"));
                    unreachable!();
                }
            };
        }
        CONFIG.as_ref().unwrap()
    }
}

pub fn base() -> &'static Base {
    &get().base
}

static mut NOW: Option<Duration> = None;

pub fn now() -> &'static Duration {
    unsafe {
        if let None = NOW {
            NOW = match utils::current_time() {
                Ok(now) => Some(now),
                Err(err) => {
                    utils::error_exit(err.context("Get current time"));
                    unreachable!();
                }
            };
        }
        NOW.as_ref().unwrap()
    }
}

pub fn now_secs() -> u64 {
    now().as_secs()
}

#[cfg(test)]
pub fn set_now(now: Duration) {
    unsafe { NOW = Some(now) }
}

static mut CURRENT_DIR: Option<PathBuf> = None;

pub fn current_dir() -> &'static PathBuf {
    unsafe {
        if let None = CURRENT_DIR {
            CURRENT_DIR = match utils::current_dir() {
                Ok(now) => Some(now),
                Err(err) => {
                    utils::error_exit(err);
                    unreachable!();
                }
            };
        }
        CURRENT_DIR.as_ref().unwrap()
    }
}

pub fn list_remotes() -> Vec<&'static str> {
    get().list_remotes()
}

pub fn get_remote(name: impl AsRef<str>) -> Result<Option<Remote>> {
    get().get_remote(name)
}

pub fn must_get_remote(name: impl AsRef<str>) -> Result<Remote> {
    get().must_get_remote(name)
}
