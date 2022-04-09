use std::fmt::Display;

use reqwest::IntoUrl;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

const USERNAME: &str = "arkus7";
const PASSWORD: &str = "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";
const ORGANIZATION_NAME: &str = "moodup";

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum TeamRepositoryPermission {
    Push,
    Pull,
}

impl Display for TeamRepositoryPermission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TeamRepositoryPermission::Push => write!(f, "write"),
            TeamRepositoryPermission::Pull => write!(f, "read"),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TeamPrivacy {
    Secret,
    Closed,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
enum RepositoryVisibility {
    Private,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Team {
    pub name: String,
    pub id: u32,
    pub slug: String,
    privacy: TeamPrivacy,
}

impl Display for Team {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct CreateTeam {
    name: String,
    repo_names: Vec<String>,
    privacy: TeamPrivacy,
}

#[derive(Serialize, Deserialize, Debug)]
struct CreateRepository {
    name: String,
    auto_init: bool,
    private: bool,
    visibility: RepositoryVisibility,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Repository {
    pub id: u32,
    pub name: String,
    pub full_name: String,
    pub ssh_url: String,
}

pub async fn get_teams() -> Result<Vec<Team>, anyhow::Error> {
    let url = format!(
        "https://api.github.com/orgs/{org_name}/teams",
        org_name = ORGANIZATION_NAME
    );

    let res: Vec<Team> = send_get_request(url).await?;
    let not_secret_teams: Vec<Team> = res
        .into_iter()
        .filter(|t| t.privacy != TeamPrivacy::Secret)
        .collect::<Vec<_>>();

    Ok(not_secret_teams)
}

pub async fn create_team(name: &str, repositories: &[String]) -> Result<Team, anyhow::Error> {
    let url = format!(
        "https://api.github.com/orgs/{org_name}/teams",
        org_name = ORGANIZATION_NAME
    );

    let body = CreateTeam {
        name: name.to_string(),
        repo_names: repositories.iter().map(|r| r.to_string()).collect(),
        privacy: TeamPrivacy::Closed,
    };

    let res: Team = send_post_request(url, Some(body)).await?;

    Ok(res)
}

pub async fn assign_repository_to_team(
    team_slug: &str,
    permission: &TeamRepositoryPermission,
    repository_name: &str,
) -> Result<(), anyhow::Error> {
    let url = format!(
        "https://api.github.com/orgs/{org_name}/teams/{team_slug}/repos/{repo_name}",
        team_slug = team_slug,
        org_name = ORGANIZATION_NAME,
        repo_name = repository_name
    );

    let res: () =
        send_put_request(url, Some(serde_json::json!({ "permission": permission }))).await?;

    Ok(res)
}

pub async fn create_repository(name: &str) -> Result<Repository, anyhow::Error> {
    let url = format!(
        "https://api.github.com/orgs/{org_name}/repos",
        org_name = ORGANIZATION_NAME
    );

    let body = CreateRepository {
        name: name.to_string(),
        auto_init: false,
        private: true,
        visibility: RepositoryVisibility::Private,
    };

    let res: Repository = send_post_request(url, Some(body)).await?;

    Ok(res)
}

async fn send_get_request<T: DeserializeOwned, U: IntoUrl>(url: U) -> Result<T, reqwest::Error> {
    let client = reqwest::Client::new();
    let res = client
        .get(url)
        .basic_auth(USERNAME, Some(PASSWORD))
        .header("User-Agent", USERNAME)
        .send()
        .await?
        .error_for_status()?
        .json::<T>()
        .await?;

    Ok(res)
}

async fn send_post_request<T, U, B>(url: U, body: Option<B>) -> Result<T, reqwest::Error>
where
    T: DeserializeOwned,
    U: IntoUrl,
    B: Serialize,
{
    let client = reqwest::Client::new();
    let res = client
        .post(url)
        .basic_auth(USERNAME, Some(PASSWORD))
        .header("User-Agent", USERNAME)
        .json(&body)
        .send()
        .await?
        .error_for_status()?
        .json::<T>()
        .await?;

    Ok(res)
}

async fn send_put_request<U, B>(url: U, body: Option<B>) -> Result<(), reqwest::Error>
where
    U: IntoUrl,
    B: Serialize,
{
    let client = reqwest::Client::new();
    let _ = client
        .put(url)
        .basic_auth(USERNAME, Some(PASSWORD))
        .header("User-Agent", USERNAME)
        .json(&body)
        .send()
        .await?
        .error_for_status()?;

    Ok(())
}
