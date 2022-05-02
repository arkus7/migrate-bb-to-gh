use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};

lazy_static! {
    pub static ref CONFIG: Config = {
        let config_bytes = include_bytes!("../config.encrypted.yml");
        let cfg = base64::decode(config_bytes).expect("cannot decode config");

        serde_yaml::from_slice(&cfg).expect("cannot parse configuration file")
    };
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
