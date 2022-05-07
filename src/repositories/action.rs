use crate::bitbucket;
use crate::github::TeamRepositoryPermission;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Repository {
    pub(crate) clone_link: String,
    name: String,
    pub(crate) full_name: String,
}

impl From<bitbucket::Repository> for Repository {
    fn from(repository: bitbucket::Repository) -> Self {
        Self {
            name: repository.name.clone(),
            clone_link: repository
                .get_ssh_url()
                .unwrap_or_else(|| panic!("missing SSH clone url for {}", repository.full_name)),
            full_name: repository.full_name,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    MigrateRepositories {
        repositories: Vec<Repository>,
    },
    CreateTeam {
        name: String,
        repositories: Vec<String>,
    },
    AddMembersToTeam {
        team_name: String,
        team_slug: String,
        members: Vec<String>,
    },
    AssignRepositoriesToTeam {
        team_name: String,
        team_slug: String,
        permission: TeamRepositoryPermission,
        repositories: Vec<String>,
    },
    SetRepositoryDefaultBranch {
        repository_name: String,
        branch: String,
    },
}

impl Action {
    pub(crate) fn describe(&self) -> String {
        match self {
            Action::MigrateRepositories { repositories } => {
                let repositories_list = repositories
                    .iter()
                    .map(|r| format!("  - {}", r.full_name))
                    .collect::<Vec<_>>()
                    .join("\n");
                format!(
                    "Migrate {} repositories:\n{}",
                    repositories.len(),
                    repositories_list
                )
            }
            Action::CreateTeam { name, repositories } => {
                let repositories_list = repositories
                    .iter()
                    .map(|r| format!("  - {}", r))
                    .collect::<Vec<_>>()
                    .join("\n");
                format!(
                    "Create team named '{}' with access to {} repositories:\n{}",
                    name,
                    repositories.len(),
                    repositories_list
                )
            }
            Action::AssignRepositoriesToTeam {
                team_name,
                permission,
                repositories,
                ..
            } => {
                let repositories_list = repositories
                    .iter()
                    .map(|r| format!("  - {}", r))
                    .collect::<Vec<_>>()
                    .join("\n");
                format!(
                    "Assign {} repositories to team {} ({}):\n{}",
                    repositories.len(),
                    team_name,
                    permission,
                    repositories_list
                )
            }
            Action::AddMembersToTeam {
                team_name, members, ..
            } => {
                let members_list = members
                    .iter()
                    .map(|r| format!("  - {}", r))
                    .collect::<Vec<_>>()
                    .join("\n");

                format!(
                    "Add {} members to {} team:\n{}",
                    members.len(),
                    team_name,
                    members_list
                )
            }
            Action::SetRepositoryDefaultBranch {
                repository_name,
                branch,
            } => {
                format!(
                    "Set default branch of '{}' repository to '{}'",
                    repository_name, branch
                )
            }
        }
    }
}

pub fn describe_actions(actions: &[Action]) -> String {
    let actions_list = actions
        .iter()
        .enumerate()
        .map(|(idx, action)| format!("{}. {}", idx + 1, action.describe()))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "There are {} actions to be done during migration:\n{}",
        actions.len(),
        actions_list
    )
}
