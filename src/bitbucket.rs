use std::fmt::{Display, Formatter};

use crate::config::CONFIG;
use reqwest::IntoUrl;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Project {
    pub uuid: String,
    pub key: String,
    pub name: String,
}

impl Project {
    pub fn get_key(&self) -> &str {
        &self.key
    }
}

impl Display for Project {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (Key: {})", self.name, self.key)
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct ProjectResponse {
    values: Vec<Project>,
    next: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct RepositoriesResponse {
    values: Vec<Repository>,
    next: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Repository {
    links: RepositoryLinks,
    pub full_name: String,
    pub name: String,
    pub mainbranch: Branch,
}

impl Display for Repository {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (branch: {})", self.name, self.mainbranch)
    }
}

impl Repository {
    pub fn get_ssh_url(&self) -> Option<String> {
        for link in &self.links.clone {
            if let CloneLink::Ssh(url) = link {
                return Some(url.clone());
            }
        }
        None
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct RepositoryLinks {
    clone: Vec<CloneLink>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "name", content = "href")]
#[serde(rename_all = "snake_case")]
enum CloneLink {
    Ssh(String),
    Https(String),
}

#[derive(Serialize, Deserialize, Debug)]
struct BranchesResponse {
    values: Vec<Branch>,
    next: Option<String>,
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Branch {
    pub name: String,
}

impl Display for Branch {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

pub async fn get_projects() -> Result<Vec<Project>, anyhow::Error> {
    let url = format!(
        "https://api.bitbucket.org/2.0/workspaces/{workspace}/projects",
        workspace = &CONFIG.bitbucket.workspace_name
    );
    let mut projects_res: ProjectResponse = send_get_request(url).await?;

    let mut projects = projects_res.values.clone();
    while projects_res.next.is_some() {
        projects_res = send_get_request(projects_res.next.unwrap()).await?;
        projects.append(&mut projects_res.values);
    }

    Ok(projects)
}

pub async fn get_project_repositories(project_key: &str) -> Result<Vec<Repository>, anyhow::Error> {
    let url = format!("https://api.bitbucket.org/2.0/repositories/{workspace}?q=project.key=\"{key}\"&pagelen={pagelen}", workspace = &CONFIG.bitbucket.workspace_name, key = project_key, pagelen = 100);
    let res: RepositoriesResponse = send_get_request(url).await?;

    Ok(res.values)
}

pub async fn get_repository_branches(full_repo_name: &str) -> anyhow::Result<Vec<Branch>> {
    let url = format!("https://api.bitbucket.org/2.0/repositories/{full_repo_name}/refs/branches?pagelen={pagelen}", full_repo_name = full_repo_name, pagelen = 100);

    let mut branches_res: BranchesResponse = send_get_request(url).await?;

    let mut branches = branches_res.values.clone();
    while branches_res.next.is_some() {
        branches_res = send_get_request(branches_res.next.unwrap()).await?;
        branches.append(&mut branches_res.values);
    }

    Ok(branches)
}

pub async fn get_repository(repo_name: &str) -> anyhow::Result<Option<Repository>> {
    let url = format!(
        "https://api.bitbucket.org/2.0/repositories/{repo_name}",
        repo_name = repo_name
    );
    let res = send_get_request(url).await;

    match res {
        Ok(res) => Ok(Some(res)),
        Err(err) => match err.status() {
            Some(status) => {
                if status.as_u16() == 404 {
                    Ok(None)
                } else {
                    Err(anyhow::anyhow!(
                        "Error: Repository {} was not found in Bitbucket account: {}",
                        &repo_name,
                        err
                    ))
                }
            }
            None => Err(anyhow::anyhow!("Unknown error: {}", err)),
        },
    }
}

async fn send_get_request<T: DeserializeOwned, U: IntoUrl>(url: U) -> Result<T, reqwest::Error> {
    let client = reqwest::Client::new();
    let res = client
        .get(url)
        .basic_auth(&CONFIG.bitbucket.username, Some(&CONFIG.bitbucket.password))
        .send()
        .await?
        .error_for_status()?
        .json::<T>()
        .await?;

    Ok(res)
}
