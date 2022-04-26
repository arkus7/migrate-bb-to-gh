use serde::{Deserialize, Serialize};
use lazy_static::lazy_static;

lazy_static! {
    pub static ref CONFIG: Config = {
        let config_bytes = include_bytes!("../config.encrypted.yml");

        serde_yaml::from_slice(config_bytes).expect("cannot parse configuration file")
    };
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    pub bitbucket: BitbucketConfig,
    pub github: GitHubConfig,
    pub circleci: CircleCiConfig,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct BitbucketConfig {
    pub username: String,
    pub password: String,
    pub workspace_name: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GitHubConfig {
    pub username: String,
    pub password: String,
    pub organization_name: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CircleCiConfig {
    pub token: String,
    pub bitbucket_org_id: String,
    pub github_org_id: String,
}

