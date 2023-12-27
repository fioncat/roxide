use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use anyhow::{bail, Result};

use crate::config::Config;
use crate::repo::Repo;

pub fn get_score(cfg: &Config, last_accessed: u64, accessed: f64) -> f64 {
    todo!()
}

struct RemoteBucket {
    pub name: String,
    pub owners: Vec<u64>,
}

struct OwnerBucket {
    pub remote: u64,
    pub name: String,

    pub repos: Vec<u64>,
}

struct RepoBucket {
    pub remote: u64,
    pub owner: u64,

    pub name: String,

    pub path: Option<String>,
    pub labels: Option<HashSet<u64>>,

    pub last_accessed: u64,
    pub accessed: f64,
}

struct Bucket {
    pub remotes: HashMap<u64, RemoteBucket>,
    pub owners: HashMap<u64, OwnerBucket>,

    pub next_remote: u64,
    pub next_owner: u64,

    pub labels: Vec<String>,

    pub repos: Vec<RepoBucket>,
}

pub struct Database<'a> {
    cfg: &'a Config,

    bucket: Bucket,

    path: PathBuf,

    refresh_labels: bool,
}
