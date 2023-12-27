use std::borrow::Cow;
use std::collections::HashSet;
use std::path::PathBuf;

use anyhow::{bail, Result};

use crate::config::Config;
use crate::repo::Repo;

struct RemoteBucket {
    pub name: String,
    pub owners: Vec<u32>,
}

struct OwnerBucket {
    pub name: String,
    pub repos: Vec<u32>,
}

struct RepoBucket {
    pub remote: u32,
    pub owner: u32,

    pub name: String,

    pub path: Option<String>,
    pub labels: Option<HashSet<u32>>,

    pub last_accessed: u64,
    pub accessed: f64,
}

struct Bucket {
    pub remotes: Vec<RemoteBucket>,
    pub owners: Vec<OwnerBucket>,

    pub labels: Vec<String>,

    pub repos: Vec<RepoBucket>,
}

pub struct Database<'a> {
    cfg: &'a Config,

    bucket: Bucket,

    path: PathBuf,
}

impl Database<'_> {
    // broken
    const BROKEN_ERROR: &'static str = "database is broken, please try to recover it";
}
