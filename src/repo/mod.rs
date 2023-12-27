pub mod database;

use std::borrow::Cow;
use std::collections::HashSet;

use crate::config::{OwnerConfig, RemoteConfig};

pub struct Repo<'a> {
    pub remote: Cow<'a, str>,
    pub owner: Cow<'a, str>,
    pub name: Cow<'a, str>,

    pub path: Option<Cow<'a, str>>,

    pub labels: Option<HashSet<Cow<'a, str>>>,

    pub last_accessed: u64,
    pub accessed: f64,

    pub remote_cfg: Option<Cow<'a, RemoteConfig>>,
    pub owner_cfg: Option<Cow<'a, OwnerConfig>>,
}
