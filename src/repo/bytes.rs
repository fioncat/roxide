use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::rc::Rc;

use anyhow::{bail, Context, Result};
use bincode::Options;
use serde::{Deserialize, Serialize};

use crate::repo::types::Repo;
use crate::utils;

#[derive(Debug, Deserialize, Serialize)]
pub struct Bytes {
    remotes: Vec<String>,
    owners: Vec<String>,
    repos: Vec<RepoBytes>,
    labels: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Serialize)]
struct RepoBytes {
    remote: u32,
    owner: u32,
    name: String,
    path: Option<String>,
    last_accessed: u64,
    accessed: f64,
    labels: Option<Vec<u32>>,
}

impl Bytes {
    // Assume a maximum size for the database. This prevents bincode from
    // throwing strange errors when it encounters invalid data.
    const MAX_SIZE: u64 = 32 << 20;

    // Use Version to ensure that decode and encode are consistent.
    const VERSION: u32 = 1;

    fn empty() -> Bytes {
        Bytes {
            remotes: Vec::new(),
            owners: Vec::new(),
            repos: Vec::new(),
            labels: None,
        }
    }

    pub fn read(path: &PathBuf) -> Result<Bytes> {
        match fs::read(path) {
            Ok(data) => Self::decode(&data),
            Err(err) if err.kind() == ErrorKind::NotFound => Ok(Bytes::empty()),
            Err(err) => Err(err).with_context(|| format!("Read index file {}", path.display())),
        }
    }

    fn decode(data: &[u8]) -> Result<Bytes> {
        let decoder = &mut bincode::options()
            .with_fixint_encoding()
            .with_limit(Self::MAX_SIZE);

        let version_size = decoder.serialized_size(&Self::VERSION).unwrap() as usize;
        if data.len() < version_size {
            bail!("corrupted data");
        }

        let (version_data, data) = data.split_at(version_size);
        let version: u32 = decoder
            .deserialize(version_data)
            .context("Decode version")?;
        if version != Self::VERSION {
            bail!("unsupported version {version}");
        }

        Ok(decoder.deserialize(data).context("Decode repo data")?)
    }

    pub fn save(&self, path: &PathBuf) -> Result<()> {
        let data = self.encode()?;
        utils::write_file(path, &data)
    }

    fn encode(&self) -> Result<Vec<u8>> {
        let buffer_size =
            bincode::serialized_size(&Self::VERSION)? + bincode::serialized_size(self)?;
        let mut buffer = Vec::with_capacity(buffer_size as usize);

        bincode::serialize_into(&mut buffer, &Self::VERSION).context("Encode version")?;
        bincode::serialize_into(&mut buffer, self).context("Encode repos")?;

        Ok(buffer)
    }
}

impl From<Bytes> for Vec<Rc<Repo>> {
    fn from(bytes: Bytes) -> Self {
        let Bytes {
            remotes,
            owners,
            repos: repo_items,
            labels: all_labels,
        } = bytes;

        let remote_index: HashMap<u32, Rc<String>> = remotes
            .into_iter()
            .enumerate()
            .map(|(idx, remote)| (idx as u32, Rc::new(remote)))
            .collect();

        let owner_index: HashMap<u32, Rc<String>> = owners
            .into_iter()
            .enumerate()
            .map(|(idx, owner)| (idx as u32, Rc::new(owner)))
            .collect();

        let label_index: Option<HashMap<u32, Rc<String>>> = match all_labels {
            Some(all_labels) => Some(
                all_labels
                    .into_iter()
                    .enumerate()
                    .map(|(idx, label)| (idx as u32, Rc::new(label)))
                    .collect(),
            ),
            None => None,
        };

        let mut repos = Vec::with_capacity(repo_items.len());
        for repo in repo_items.into_iter() {
            let remote = match remote_index.get(&repo.remote) {
                Some(remote) => Rc::clone(remote),
                None => continue,
            };
            let owner = match owner_index.get(&repo.owner) {
                Some(owner) => Rc::clone(owner),
                None => continue,
            };
            let labels = match repo.labels.as_ref() {
                Some(repo_label_index) => {
                    if let None = label_index.as_ref() {
                        None
                    } else {
                        let label_index = label_index.as_ref().unwrap();
                        let mut repo_label_set = HashSet::with_capacity(repo_label_index.len());
                        for idx in repo_label_index.iter() {
                            if let Some(label) = label_index.get(idx) {
                                repo_label_set.insert(Rc::clone(label));
                            }
                        }
                        Some(repo_label_set)
                    }
                }
                None => None,
            };

            repos.push(Rc::new(Repo {
                remote,
                owner,
                name: Rc::new(repo.name),
                path: repo.path,
                last_accessed: repo.last_accessed,
                accessed: repo.accessed,
                labels,
            }));
        }
        repos.sort_unstable_by(|repo1, repo2| repo2.score().total_cmp(&repo1.score()));

        repos
    }
}

impl From<Vec<Rc<Repo>>> for Bytes {
    fn from(repos: Vec<Rc<Repo>>) -> Self {
        let mut remotes: Vec<String> = Vec::new();
        let mut remote_index: HashMap<String, u32> = HashMap::new();

        let mut owners: Vec<String> = Vec::new();
        let mut owner_index: HashMap<String, u32> = HashMap::new();

        let mut all_labels: Vec<String> = Vec::new();
        let mut all_labels_index: HashMap<String, u32> = HashMap::new();

        let mut repo_items: Vec<RepoBytes> = Vec::with_capacity(repos.len());

        for repo in repos.into_iter() {
            let remote = match remote_index.get(repo.remote.as_str()) {
                Some(idx) => *idx,
                None => {
                    let remote_item = format!("{}", repo.remote);
                    let idx = remotes.len() as u32;
                    remote_index.insert(remote_item.clone(), idx);
                    remotes.push(remote_item);
                    idx
                }
            };
            let owner = match owner_index.get(repo.owner.as_str()) {
                Some(idx) => *idx,
                None => {
                    let owner_item = format!("{}", repo.owner);
                    let idx = owners.len() as u32;
                    owner_index.insert(owner_item.clone(), idx);
                    owners.push(owner_item);
                    idx
                }
            };
            let labels = match repo.labels.as_ref() {
                Some(labels_set) => {
                    let mut labels_index: Vec<u32> = Vec::with_capacity(labels_set.len());
                    for label in labels_set.iter() {
                        let idx = match all_labels_index.get(label.as_str()) {
                            Some(idx) => *idx,
                            None => {
                                let label_item = format!("{}", label);
                                let idx = all_labels.len() as u32;
                                all_labels_index.insert(label_item.clone(), idx);
                                all_labels.push(label_item);
                                idx
                            }
                        };
                        labels_index.push(idx);
                    }

                    Some(labels_index)
                }
                None => None,
            };

            let repo = Rc::try_unwrap(repo).expect("Unwrap repo Rc failed");
            let Repo {
                remote: _,
                owner: _,
                name,
                path,
                last_accessed,
                accessed,
                labels: _,
            } = repo;
            let name = Rc::try_unwrap(name).expect("Unwrap repo name failed");

            repo_items.push(RepoBytes {
                remote,
                owner,
                path,
                name,
                last_accessed,
                accessed,
                labels,
            });
        }

        Bytes {
            remotes,
            owners,
            repos: repo_items,
            labels: if all_labels.is_empty() {
                None
            } else {
                Some(all_labels)
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use serial_test::serial;

    use crate::repo::database::tests::list_repos;

    use super::*;

    #[test]
    #[serial]
    fn test_repo_bytes() {
        let expect_repos = list_repos();

        let bytes: Bytes = list_repos().into();
        let bytes = bytes.encode().unwrap();

        let expect_bytes = Bytes::decode(&bytes).unwrap();
        let results: Vec<Rc<Repo>> = expect_bytes.into();

        assert_eq!(results, expect_repos);
    }
}
