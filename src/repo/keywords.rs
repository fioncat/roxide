use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::utils;

pub struct Keywords {
    data: HashMap<String, HashMap<String, Record>>,

    path: PathBuf,

    disable: bool,

    now: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct Record {
    pub last_accessed: u64,
    pub accessed: u64,
}

impl Keywords {
    const COMPLETE_ACCESSED: u64 = 1;

    pub fn load(cfg: &Config) -> Result<Keywords> {
        let path = cfg.get_meta_dir().join("keywords");

        let mut not_found = false;
        let data: HashMap<String, HashMap<String, Record>> = match fs::read(&path) {
            Ok(data) => bincode::deserialize(&data).unwrap_or(HashMap::new()),
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                not_found = true;
                HashMap::new()
            }
            Err(err) => {
                return Err(err).with_context(|| format!("read keywords file '{}'", path.display()))
            }
        };

        if cfg.keyword_expire == 0 {
            // The user disable keyword, delete record file if it is existing
            if !not_found {
                fs::remove_file(&path)
                    .with_context(|| format!("delete keywords file '{}'", path.display()))?;
            }
            return Ok(Keywords {
                data,
                path,
                disable: true,
                now: 0,
            });
        }

        let now = cfg.now();
        let mut filter_data: HashMap<String, HashMap<String, Record>> =
            HashMap::with_capacity(data.len());
        for (remote, records) in data {
            let filter_records: HashMap<String, Record> = records
                .into_iter()
                .filter_map(|(kw, record)| {
                    let expire_time = record.last_accessed + cfg.keyword_expire;
                    if expire_time < now {
                        return None;
                    }
                    Some((kw, record))
                })
                .collect();
            if filter_records.is_empty() {
                continue;
            }
            filter_data.insert(remote, filter_records);
        }

        Ok(Keywords {
            data: filter_data,
            path,
            disable: false,
            now,
        })
    }

    pub fn upsert<R, K>(&mut self, remote: R, keyword: K)
    where
        R: AsRef<str>,
        K: AsRef<str>,
    {
        if self.disable {
            return;
        }
        let (remote, mut records) = self
            .data
            .remove_entry(remote.as_ref())
            .unwrap_or((remote.as_ref().to_string(), HashMap::with_capacity(1)));

        let (keyword, mut record) = records.remove_entry(keyword.as_ref()).unwrap_or((
            keyword.as_ref().to_string(),
            Record {
                last_accessed: 0,
                accessed: 0,
            },
        ));

        record.last_accessed = self.now;
        record.accessed += 1;

        records.insert(keyword, record);
        self.data.insert(remote, records);
    }

    pub fn complete(mut self, remote: impl AsRef<str>) -> Vec<String> {
        let records = match self.data.remove(remote.as_ref()) {
            Some(records) => records,
            None => return vec![],
        };

        let mut records: Vec<_> = records.into_iter().collect();
        records.sort_unstable_by(|(kw0, record0), (kw1, record1)| {
            if record0.last_accessed != record1.last_accessed {
                return record1.last_accessed.cmp(&record0.last_accessed);
            }
            if record0.accessed != record1.accessed {
                return record1.accessed.cmp(&record0.accessed);
            }
            kw0.cmp(kw1)
        });

        records
            .into_iter()
            .filter_map(|(kw, record)| {
                if record.accessed < Self::COMPLETE_ACCESSED {
                    return None;
                }
                Some(kw)
            })
            .collect()
    }

    pub fn save(self) -> Result<()> {
        let data = bincode::serialize(&self.data).context("encode keywords data")?;
        utils::write_file(&self.path, &data)?;
        Ok(())
    }
}

#[cfg(test)]
mod keywords_tests {
    use crate::config::config_tests;
    use crate::repo::keywords::*;

    #[test]
    fn test_complete() {
        let cfg = config_tests::load_test_config("keywords/completion");

        let mut disable_cfg = cfg.clone();
        disable_cfg.keyword_expire = 0;
        let _ = Keywords::load(&disable_cfg).unwrap(); // remove old file

        let mut keywords = Keywords::load(&cfg).unwrap();
        let cases = vec![
            ("", "go"),
            ("", "go"),
            ("", "vim"),
            ("", "vim"),
            ("", "vim"),
            ("", "vim"),
            ("", "vim"),
            ("", "vim"),
            ("test", "hello"),
            ("test", "hello"),
            ("", "rox"),
            ("", "rox"),
            ("", "rox"),
            ("test", "rust"),
            ("test", "rust"),
            ("test", "rust"),
        ];
        for (remote, keyword) in cases {
            keywords.upsert(remote, keyword);
        }
        keywords.save().unwrap();

        let expects = vec![("", vec!["vim", "rox"]), ("test", vec!["rust"])];
        for (remote, expect) in expects {
            let keywords = Keywords::load(&cfg).unwrap();
            let keywords = keywords.complete(remote);
            let result: Vec<_> = keywords.iter().map(|s| s.as_str()).collect();
            assert_eq!(result, expect);
        }
    }

    #[test]
    fn test_expire() {
        let mut cfg = config_tests::load_test_config("keywords/expire");
        cfg.keyword_expire = 3;

        let mut keywords = Keywords::load(&cfg).unwrap();
        keywords.upsert("", "rox");
        keywords.upsert("", "some");

        keywords.save().unwrap();

        cfg.set_now(cfg.now() + 5);
        let keywords = Keywords::load(&cfg).unwrap();
        // All keywords should be expired
        assert!(keywords.complete("").is_empty());
    }
}
