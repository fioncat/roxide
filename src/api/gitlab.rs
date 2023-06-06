#![allow(dead_code)]

use anyhow::{bail, Context, Result};
use reqwest::blocking::{Client, Request};
use reqwest::{Method, Url};
use serde::de::DeserializeOwned;
use serde::Deserialize;

use crate::config::types::Remote;

use super::ApiRepo;
use super::Provider;

#[derive(Debug, Deserialize)]
struct GitlabRepo {
    pub name: String,
    pub default_branch: String,
    pub web_url: String,
}

impl GitlabRepo {
    fn to_api(self) -> ApiRepo {
        ApiRepo {
            name: self.name,
            default_branch: self.default_branch,
            upstream: None,
            web_url: self.web_url,
        }
    }
}

pub struct Gitlab {
    token: Option<String>,

    client: Client,

    per_page: u32,

    url: String,
}

#[derive(Debug, Deserialize)]
struct GitlabError {
    pub message: String,
}

impl Provider for Gitlab {
    fn list_repos(&self, owner: &str) -> Result<Vec<String>> {
        let path = format!("groups/{owner}/projects?per_page={}", self.per_page);
        let gitlab_repos = self.execute::<Vec<GitlabRepo>>(&path)?;
        let repos: Vec<String> = gitlab_repos.into_iter().map(|repo| repo.name).collect();
        Ok(repos)
    }

    fn get_repo(&self, owner: &str, name: &str) -> Result<ApiRepo> {
        let id = format!("{owner}/{name}");
        let id_encode = urlencoding::encode(&id);
        let path = format!("projects/{id_encode}");
        Ok(self.execute::<GitlabRepo>(&path)?.to_api())
    }
}

impl Gitlab {
    const API_VERSION: u8 = 4;

    pub fn new(remote: &Remote) -> Box<dyn Provider> {
        let client = super::build_common_client(remote);
        let domain = match &remote.api_domain {
            Some(domain) => domain.clone(),
            None => String::from("gitlab.com"),
        };

        let url = format!("https://{domain}/api/v{}", Self::API_VERSION);

        Box::new(Gitlab {
            token: remote.token.clone(),
            client,
            url,
            per_page: remote.list_limit,
        })
    }

    fn execute<T>(&self, path: &str) -> Result<T>
    where
        T: DeserializeOwned + ?Sized,
    {
        let req = self.build_request(path)?;
        let resp = self.client.execute(req).context("Gitlab http request")?;
        let ok = resp.status().is_success();
        let data = resp.bytes().context("Read Gitlab response body")?;
        if ok {
            return serde_json::from_slice(&data).context("Decode Gitlab response data");
        }

        match serde_json::from_slice::<GitlabError>(&data) {
            Ok(err) => bail!("Gitlab api error: {}", err.message),
            Err(_err) => bail!(
                "Unknown Gitlab api error: {}",
                String::from_utf8(data.to_vec())
                    .context("Decode Gitlab response to UTF-8 string")?
            ),
        }
    }

    fn build_request(&self, path: &str) -> Result<Request> {
        let url = format!("{}/{path}", self.url);
        let url = Url::parse(url.as_str()).with_context(|| format!("Parse gitlab url {url}"))?;
        let mut builder = self.client.request(Method::GET, url);
        builder = builder.header("User-Agent", "roxide-client");
        if let Some(token) = &self.token {
            builder = builder.header("PRIVATE-TOKEN", token);
        }

        builder.build().context("Build Gitlab request")
    }
}
