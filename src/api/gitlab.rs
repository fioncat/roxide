use anyhow::{bail, Context, Result};
use reqwest::blocking::{Client, Request};
use reqwest::{Method, Url};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::api::types::{ApiRepo, MergeOptions, Provider};
use crate::config::types::Remote;

#[derive(Debug, Deserialize)]
struct GitlabRepo {
    pub path: String,
    pub default_branch: String,
    pub web_url: String,
}

impl GitlabRepo {
    fn to_api(self) -> ApiRepo {
        ApiRepo {
            default_branch: self.default_branch,
            upstream: None,
            web_url: self.web_url,
        }
    }
}

#[derive(Debug, Deserialize)]
struct MergeRequest {
    web_url: String,
}

#[derive(Debug, Serialize)]
struct CreateMergeRequest {
    id: String,
    source_branch: String,
    target_branch: String,
    title: String,
    description: String,
}

pub struct Gitlab {
    token: Option<String>,

    client: Client,

    per_page: u32,

    url: String,
}

#[derive(Debug, Deserialize)]
struct GitlabError {
    pub error: String,
}

impl Provider for Gitlab {
    fn list_repos(&self, owner: &str) -> Result<Vec<String>> {
        let owner_encode = urlencoding::encode(owner);
        let path = format!("groups/{owner_encode}/projects?per_page={}", self.per_page);
        let gitlab_repos = self.execute_get::<Vec<GitlabRepo>>(&path)?;
        let repos: Vec<String> = gitlab_repos.into_iter().map(|repo| repo.path).collect();
        Ok(repos)
    }

    fn get_repo(&self, owner: &str, name: &str) -> Result<ApiRepo> {
        let id = format!("{owner}/{name}");
        let id_encode = urlencoding::encode(&id);
        let path = format!("projects/{id_encode}");
        Ok(self.execute_get::<GitlabRepo>(&path)?.to_api())
    }

    fn get_merge(&self, merge: MergeOptions) -> Result<Option<String>> {
        if let Some(_) = merge.upstream {
            bail!("Gitlab now does not support upstream");
        }
        let id = format!("{}/{}", merge.owner, merge.name);
        let id_encode = urlencoding::encode(&id);
        let path = format!(
            "projects/{id_encode}/merge_requests?state=opened&source_branch={}&target_branch={}",
            merge.source, merge.target
        );
        let mut mrs = self.execute_get::<Vec<MergeRequest>>(&path)?;
        if mrs.is_empty() {
            return Ok(None);
        }
        Ok(Some(mrs.remove(0).web_url))
    }

    fn create_merge(&self, merge: MergeOptions, title: String, body: String) -> Result<String> {
        if let Some(_) = merge.upstream {
            bail!("Gitlab now does not support upstream");
        }
        let id = format!("{}/{}", merge.owner, merge.name);
        let id_encode = urlencoding::encode(&id);
        let path = format!("projects/{id_encode}/merge_requests");
        let create = CreateMergeRequest {
            id,
            source_branch: merge.source,
            target_branch: merge.target,
            title,
            description: body,
        };
        let mr = self.execute_post::<CreateMergeRequest, MergeRequest>(&path, create)?;
        Ok(mr.web_url)
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

    fn execute_get<T>(&self, path: &str) -> Result<T>
    where
        T: DeserializeOwned + ?Sized,
    {
        let req = self.build_request(path, Method::GET, None)?;
        self.execute(req)
    }

    fn execute_post<B, R>(&self, path: &str, body: B) -> Result<R>
    where
        B: Serialize,
        R: DeserializeOwned + ?Sized,
    {
        let body = serde_json::to_vec(&body).context("Encode Github request body")?;
        let req = self.build_request(path, Method::POST, Some(body))?;
        self.execute(req)
    }

    fn execute<T>(&self, req: Request) -> Result<T>
    where
        T: DeserializeOwned + ?Sized,
    {
        let resp = self.client.execute(req).context("Gitlab http request")?;
        let ok = resp.status().is_success();
        let data = resp.bytes().context("Read Gitlab response body")?;
        if ok {
            return serde_json::from_slice(&data).context("Decode Gitlab response data");
        }

        match serde_json::from_slice::<GitlabError>(&data) {
            Ok(err) => bail!("Gitlab api error: {}", err.error),
            Err(_err) => bail!(
                "Unknown Gitlab api error: {}",
                String::from_utf8(data.to_vec())
                    .context("Decode Gitlab response to UTF-8 string")?
            ),
        }
    }

    fn build_request(&self, path: &str, method: Method, body: Option<Vec<u8>>) -> Result<Request> {
        let url = format!("{}/{path}", self.url);
        let url = Url::parse(url.as_str()).with_context(|| format!("Parse gitlab url {url}"))?;
        let mut builder = self.client.request(method, url);
        builder = builder.header("User-Agent", "roxide-client");
        if let Some(token) = &self.token {
            builder = builder.header("PRIVATE-TOKEN", token);
        }
        if let Some(body) = body {
            builder = builder
                .body(body)
                .header("Content-Type", "application/json");
        }

        builder.build().context("Build Gitlab request")
    }
}
