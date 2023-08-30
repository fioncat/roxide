use std::collections::HashMap;

use anyhow::Result;

use crate::api::types::{ApiRepo, MergeOptions, Provider};

pub struct Alias {
    upstream: Box<dyn Provider>,

    owner_map: HashMap<String, String>,
    repo_map: HashMap<String, HashMap<String, String>>,

    repo_map_revert: HashMap<String, HashMap<String, String>>,
}

impl Provider for Alias {
    fn get_repo(&self, raw_owner: &str, raw_name: &str) -> Result<ApiRepo> {
        let owner = self.alias_owner(raw_owner);
        let name = self.alias_repo(owner, raw_name);
        self.upstream.get_repo(owner, name)
    }

    fn list_repos(&self, owner: &str) -> Result<Vec<String>> {
        let owner = self.alias_owner(owner);
        let names = self.upstream.list_repos(owner)?;
        let names = names
            .into_iter()
            .map(|name| self.raw_repo(owner, name))
            .collect();
        Ok(names)
    }

    fn get_merge(&self, mut merge: MergeOptions) -> Result<Option<String>> {
        let owner = self.alias_owner(&merge.owner);
        let name = self.alias_repo(&merge.owner, &merge.name);

        merge.owner = owner.to_string();
        merge.name = name.to_string();

        self.upstream.get_merge(merge)
    }

    fn create_merge(
        &mut self,
        mut merge: MergeOptions,
        title: String,
        body: String,
    ) -> Result<String> {
        let owner = self.alias_owner(&merge.owner);
        let name = self.alias_repo(&merge.owner, &merge.name);

        merge.owner = owner.to_string();
        merge.name = name.to_string();

        self.upstream.create_merge(merge, title, body)
    }
}

impl Alias {
    pub fn new(
        owner_map: HashMap<String, String>,
        repo_map: HashMap<String, HashMap<String, String>>,
        upstream: Box<dyn Provider>,
    ) -> Box<dyn Provider> {
        let repo_map_revert: HashMap<_, _> = repo_map
            .iter()
            .map(|(owner, map)| {
                (
                    owner.clone(),
                    map.iter()
                        .map(|(key, val)| (val.clone(), key.clone()))
                        .collect(),
                )
            })
            .collect();
        Box::new(Alias {
            owner_map,
            repo_map,
            upstream,
            repo_map_revert,
        })
    }

    fn alias_owner<'a>(&'a self, raw: &'a str) -> &'a str {
        match self.owner_map.get(raw) {
            Some(name) => name.as_str(),
            None => raw,
        }
    }

    fn alias_repo<'a>(&'a self, owner: &str, name: &'a str) -> &'a str {
        if let Some(map) = self.repo_map.get(owner) {
            if let Some(name) = map.get(name) {
                return name.as_str();
            }
        }
        name
    }

    fn raw_repo(&self, owner: &str, name: String) -> String {
        if let Some(map) = self.repo_map_revert.get(owner) {
            if let Some(raw) = map.get(&name) {
                return raw.clone();
            }
        }
        name
    }
}
