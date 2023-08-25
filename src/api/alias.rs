use std::collections::HashMap;

use anyhow::Result;

use crate::api::types::{ApiRepo, MergeOptions, Provider};

pub struct Alias {
    upstream: Box<dyn Provider>,

    owner_map: HashMap<String, String>,
    repo_map: HashMap<String, HashMap<String, String>>,
}

impl Provider for Alias {
    fn get_repo(&self, owner: &str, name: &str) -> Result<ApiRepo> {
        let owner = self.alias_owner(owner);
        let name = self.alias_repo(owner, name);
        self.upstream.get_repo(owner, name)
    }

    fn list_repos(&self, owner: &str) -> Result<Vec<String>> {
        let owner = self.alias_owner(owner);
        self.upstream.list_repos(owner)
    }

    fn get_merge(&self, mut merge: MergeOptions) -> Result<Option<String>> {
        let owner = self.alias_owner(&merge.owner);
        let name = self.alias_repo(&merge.owner, &merge.name);

        merge.owner = owner.to_string();
        merge.name = name.to_string();

        self.upstream.get_merge(merge)
    }

    fn create_merge(&self, mut merge: MergeOptions, title: String, body: String) -> Result<String> {
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
        Box::new(Alias {
            owner_map,
            repo_map,
            upstream,
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
}
