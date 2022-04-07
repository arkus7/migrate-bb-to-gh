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

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TeamPrivacy {
    Secret,
    Closed,
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

async fn send_get_request<T: DeserializeOwned, U: IntoUrl>(url: U) -> Result<T, reqwest::Error> {
    let client = reqwest::Client::new();
    let res = client
        .get(url)
        .basic_auth(USERNAME, Some(PASSWORD))
        .header("User-Agent", USERNAME)
        .send()
        .await?
        .json::<T>()
        .await?;

    Ok(res)
}
