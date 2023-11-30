pub mod database;

use std::{collections::HashSet, rc::Rc};

use crate::config;

#[derive(Debug, PartialEq)]
pub struct Remote {
    pub name: String,

    pub cfg: config::Remote,
}

#[derive(Debug, PartialEq)]
pub struct Owner {
    pub name: String,

    pub remote: Rc<Remote>,

    pub cfg: Option<config::Owner>,
}

#[derive(Debug, PartialEq)]
pub struct Repo {
    pub name: String,

    pub remote: Rc<Remote>,
    pub owner: Rc<Owner>,

    pub path: Option<String>,

    pub last_accessed: u64,
    pub accessed: f64,

    pub labels: Option<HashSet<Rc<String>>>,
}
