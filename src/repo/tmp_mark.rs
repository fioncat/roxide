use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::rc::Rc;

use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::ser::PrettyFormatter;
use serde_json::Serializer;

use crate::repo::database::Database;
use crate::repo::query;
use crate::repo::types::{NameLevel, Repo};
use crate::utils::Lock;
use crate::{config, utils};

pub struct TmpMark {
    data: BTreeMap<String, BTreeSet<String>>,
    path: PathBuf,
    _lock: Lock,
}

impl TmpMark {
    pub fn read() -> Result<TmpMark> {
        let lock = Lock::acquire("tmp_mark")?;
        let path = PathBuf::from(&config::base().metadir).join("tmp_mark");
        let data: BTreeMap<String, BTreeSet<String>> = match fs::read(&path) {
            Ok(data) => serde_json::from_slice(&data).context("Invalid json for tmp_mark")?,
            Err(err) if err.kind() == ErrorKind::NotFound => BTreeMap::new(),
            Err(err) => return Err(err).context("Read tmp_mark file"),
        };
        Ok(TmpMark {
            data,
            path,
            _lock: lock,
        })
    }

    pub fn mark(&mut self, repo: &Rc<Repo>) {
        let name = repo.long_name();
        match self.data.get_mut(repo.remote.as_str()) {
            Some(set) => {
                set.insert(name);
            }
            None => {
                let mut set = BTreeSet::new();
                set.insert(name);
                self.data.insert(String::from(repo.remote.as_str()), set);
            }
        }
    }

    pub fn query_remove(
        &mut self,
        db: &Database,
        remote: &Option<String>,
        query: &Option<String>,
    ) -> Result<(Vec<Rc<Repo>>, NameLevel)> {
        let mut repos = Vec::new();
        let query_owner = match query.as_ref() {
            Some(query) => {
                if !query.ends_with("/") {
                    bail!("Tmp query must be owner (ends with /)");
                }
                Some(query.strip_suffix("/").unwrap())
            }
            None => None,
        };

        let mut new_data = BTreeMap::new();
        loop {
            let item = self.data.pop_first();
            if let None = item {
                break;
            }
            let (mark_remote, set) = item.unwrap();
            if let Some(remote) = remote.as_ref() {
                if remote != mark_remote.as_str() {
                    new_data.insert(mark_remote, set);
                    continue;
                }
            }

            let mut new_set = BTreeSet::new();
            for long_name in set {
                let (owner, name) = query::parse_owner(&long_name);
                if let Some(query_owner) = query_owner.as_ref() {
                    if owner != *query_owner {
                        new_set.insert(long_name);
                        continue;
                    }
                }
                match db.get(&mark_remote, &owner, &name) {
                    Some(repo) => repos.push(repo),
                    None => {}
                }
            }
            if new_set.is_empty() {
                continue;
            }
            new_data.insert(mark_remote, new_set);
        }
        self.data = new_data;

        if let None = remote.as_ref() {
            return Ok((repos, NameLevel::Full));
        }

        if let None = query.as_ref() {
            return Ok((repos, NameLevel::Owner));
        }

        Ok((repos, NameLevel::Name))
    }

    pub fn save(&self) -> Result<()> {
        if self.data.is_empty() {
            fs::remove_file(&self.path).context("Remove tmp_mark file")?;
            return Ok(());
        }
        let formatter = PrettyFormatter::with_indent(b"  ");
        let mut buf = Vec::new();
        let mut ser = Serializer::with_formatter(&mut buf, formatter);
        self.data
            .serialize(&mut ser)
            .context("Serialize snapshot")?;
        utils::write_file(&self.path, &buf)
    }
}
