use std::collections::HashMap;

use anyhow::Result;

use crate::api::{ApiRepo, MergeOptions, Provider};

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
        let name = self.alias_repo(owner, &merge.name);

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
        let name = self.alias_repo(owner, &merge.name);

        merge.owner = owner.to_string();
        merge.name = name.to_string();

        self.upstream.create_merge(merge, title, body)
    }

    fn search_repos(&self, query: &str) -> Result<Vec<String>> {
        self.upstream.search_repos(query)
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

#[cfg(test)]
mod tests {
    use crate::api::alias::*;
    use crate::api::api_tests::StaticProvider;

    #[test]
    fn test_alias() {
        let mut owner_map = HashMap::with_capacity(1);
        owner_map.insert(String::from("test-alias"), String::from("fioncat"));
        let mut repo_map = HashMap::with_capacity(2);
        repo_map.insert(String::from("fioncat"), HashMap::new());
        repo_map
            .get_mut("fioncat")
            .unwrap()
            .insert(String::from("ro"), String::from("roxide"));
        repo_map
            .get_mut("fioncat")
            .unwrap()
            .insert(String::from("vim"), String::from("spacenvim"));
        repo_map
            .get_mut("fioncat")
            .unwrap()
            .insert(String::from("unknown"), String::from("zzz"));

        repo_map.insert(String::from("kubernetes"), HashMap::new());
        repo_map
            .get_mut("kubernetes")
            .unwrap()
            .insert(String::from("k8s"), String::from("kubernetes"));

        let upstream = StaticProvider::mock();

        let mut alias = Alias::new(owner_map, repo_map, upstream);
        let result = alias.list_repos("test-alias").unwrap();
        let expect: Vec<String> = vec!["ro", "vim", "dotfiles", "fioncat"]
            .into_iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(result, expect);

        let result = alias.list_repos("kubernetes").unwrap();
        let expect: Vec<String> = vec!["k8s", "kube-proxy", "kubelet", "kubectl"]
            .into_iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(result, expect);

        alias.get_repo("test-alias", "vim").unwrap();
        alias.get_repo("test-alias", "ro").unwrap();
        alias.get_repo("kubernetes", "k8s").unwrap();

        let merge = MergeOptions {
            owner: format!("test-alias"),
            name: format!("ro"),
            upstream: None,
            source: format!("test"),
            target: format!("main"),
        };
        let result = alias
            .create_merge(merge.clone(), String::new(), String::new())
            .unwrap();
        assert_eq!(result, "fioncat/roxide");

        let result = alias.get_merge(merge).unwrap().unwrap();
        assert_eq!(result, "fioncat/roxide");
    }
}
