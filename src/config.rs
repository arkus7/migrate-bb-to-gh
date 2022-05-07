use anyhow::Context;
use serde::{Deserialize, Serialize};

pub fn parse_config() -> anyhow::Result<Config> {
    let config_bytes = include_bytes!("../config.encrypted.yml");
    let cfg = decrypt_config(config_bytes)?;

    serde_yaml::from_slice(&cfg).with_context(|| "Cannot parse decrypted configuration file")
}

fn decrypt_config(config_bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    base64::decode(config_bytes).with_context(|| "cannot decrypt config")
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    pub bitbucket: BitbucketConfig,
    pub github: GitHubConfig,
    pub circleci: CircleCiConfig,
    pub git: GitConfig,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BitbucketConfig {
    pub username: String,
    pub password: String,
    pub workspace_name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GitHubConfig {
    pub username: String,
    pub password: String,
    pub organization_name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CircleCiConfig {
    pub token: String,
    pub bitbucket_org_id: String,
    pub github_org_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GitConfig {
    pub push_ssh_key: String,
    pub pull_ssh_key: String,
}
