use anyhow::{bail, Context, Result};
use reqwest::blocking::{Client, Request};
use reqwest::{Method, Url};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::api::{ApiRepo, MergeOptions, Provider};
use crate::config::{defaults, RemoteConfig};

#[derive(Debug, Deserialize)]
struct GitlabRepo {
    pub path: String,
    pub path_with_namespace: String,

    #[serde(default = "defaults::empty_string")]
    pub default_branch: String,

    pub web_url: String,
}

impl GitlabRepo {
    fn api(self) -> ApiRepo {
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
        Ok(self.execute_get::<GitlabRepo>(&path)?.api())
    }

    fn get_merge(&self, merge: MergeOptions) -> Result<Option<String>> {
        if merge.upstream.is_some() {
            bail!("GitLab now does not support upstream");
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

    fn create_merge(&mut self, merge: MergeOptions, title: String, body: String) -> Result<String> {
        if merge.upstream.is_some() {
            bail!("GitLab now does not support upstream");
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

    fn search_repos(&self, query: &str) -> Result<Vec<String>> {
        let path = format!("search?scope=projects&search={}", query);
        let gitlab_repos = self.execute_get::<Vec<GitlabRepo>>(&path)?;
        let repos: Vec<String> = gitlab_repos
            .into_iter()
            .map(|repo| repo.path_with_namespace)
            .collect();
        Ok(repos)
    }
}

impl Gitlab {
    const API_VERSION: u8 = 4;

    pub fn build(remote_cfg: &RemoteConfig) -> Box<dyn Provider> {
        let client = super::build_common_client(remote_cfg);
        let domain = match &remote_cfg.api_domain {
            Some(domain) => domain.clone(),
            None => String::from("gitlab.com"),
        };

        let url = format!("https://{domain}/api/v{}", Self::API_VERSION);

        Box::new(Gitlab {
            token: remote_cfg.token.clone(),
            client,
            url,
            per_page: remote_cfg.list_limit,
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
        let body = serde_json::to_vec(&body).context("encode GitLab request body")?;
        let req = self.build_request(path, Method::POST, Some(body))?;
        self.execute(req)
    }

    fn execute<T>(&self, req: Request) -> Result<T>
    where
        T: DeserializeOwned + ?Sized,
    {
        let resp = self.client.execute(req).context("GitLab http request")?;
        let ok = resp.status().is_success();
        let data = resp.bytes().context("read GitLab response body")?;
        if ok {
            return serde_json::from_slice(&data).context("decode GitLab response data");
        }

        match serde_json::from_slice::<GitlabError>(&data) {
            Ok(err) => bail!("GitLab api error: {}", err.error),
            Err(_err) => bail!(
                "unknown GitLab api error: {}",
                String::from_utf8(data.to_vec())
                    .context("decode GitLab response to UTF-8 string")?
            ),
        }
    }

    fn build_request(&self, path: &str, method: Method, body: Option<Vec<u8>>) -> Result<Request> {
        let url = format!("{}/{path}", self.url);
        let url = Url::parse(url.as_str()).with_context(|| format!("parse GitLab url {url}"))?;
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

        builder.build().context("build GitLab request")
    }
}
