use std::{collections::HashSet, fs::File, path::PathBuf};

use crate::{
    bitbucket::{self, Repository},
    github::{self, TeamRepositoryPermission},
    migrator::{Action, Migration},
    spinner,
};

use crate::bitbucket::BitbucketApi;
use crate::config::CONFIG;
use crate::github::GithubApi;
use crate::prompts::{Confirm, FuzzySelect, Input, MultiSelect, Select};
use anyhow::{anyhow, bail};

pub struct Wizard {
    output_path: PathBuf,
    version: String,
    bitbucket: BitbucketApi,
    github: GithubApi,
}

#[derive(Debug)]
pub struct WizardResult {
    pub actions: Vec<Action>,
    pub migration_file_path: PathBuf,
}

impl Wizard {
    pub fn new(output_path: PathBuf, version: &str) -> Self {
        Self {
            output_path,
            version: version.to_owned(),
            bitbucket: BitbucketApi::new(&CONFIG.bitbucket),
            github: GithubApi::new(&CONFIG.github),
        }
    }

    pub async fn run(&self) -> Result<WizardResult, anyhow::Error> {
        let project = self.select_project().await?;
        let repositories = self.select_repositories(&project).await?;

        let mut actions = vec![];

        let repositories_names: Vec<String> = repositories
            .iter()
            .map(|r| r.full_name.to_owned())
            .collect();

        let spinner = spinner::create_spinner("Fetching existing repositories from GitHub...");
        let github_repositories = self.github.get_repositories().await?;
        spinner.finish_with_message(format!(
            "Fetched {} existing repositories from GitHub!",
            github_repositories.len()
        ));

        let spinner = spinner::create_spinner("Checking for existing repositories in GitHub...");
        let selected_repo_names = repositories
            .iter()
            .map(|r| r.full_name.to_owned())
            .collect::<HashSet<_>>();

        let existing_repo_names = github_repositories
            .iter()
            .map(|r| r.full_name.to_owned())
            .collect::<HashSet<_>>();

        let intersection = selected_repo_names
            .intersection(&existing_repo_names)
            .collect::<Vec<_>>();
        spinner.finish_with_message(format!(
            "{} of the selected repositories already exist on GitHub",
            intersection.len()
        ));

        let repositories = if !intersection.is_empty() {
            let intersection_names = intersection
                .iter()
                .map(|n| n.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            let msg = format!("The following repositories already exist in GitHub: {}\nDo you want to update them?", intersection_names);
            let options = ["Update existing repositories", "Skip existing repositories"];
            let overwrite = Select::with_prompt(msg)
                .items(&options)
                .default(1)
                .interact_idx()?;
            match overwrite {
                0 => repositories,
                1 => repositories
                    .iter()
                    .filter(|r| !intersection.contains(&&r.full_name))
                    .cloned()
                    .collect(),
                _ => unreachable!(),
            }
        } else {
            repositories
        };

        if repositories.is_empty() {
            bail!("No repositories to take actions on, exiting...");
        } else {
            println!(
                "Continuing with {} repositories:\n{}",
                repositories.len(),
                repositories
                    .iter()
                    .map(|r| format!("  - {}", r.full_name))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        }

        let migrate_repos = Confirm::with_prompt(
            "Do you want to mirror selected repositories from Bitbucket to GitHub?",
        )
        .interact()?;
        if migrate_repos {
            let migrate_action = Action::MigrateRepositories {
                repositories: repositories.iter().map(|r| r.clone().into()).collect(),
            };
            actions.push(migrate_action);
        }

        let spinner = spinner::create_spinner("Fetching teams...");
        let teams = self.github.get_teams().await?;
        spinner.finish_with_message(format!("Fetched {} teams from GitHub", teams.len()));

        println!("These teams already exist on GitHub:");
        teams.iter().for_each(|t| println!("  - {}", t.name));

        let create_team_confirm =
            Confirm::with_prompt("Do you want to create a new team for selected repositories?")
                .interact()?;
        let create_team_actions = if create_team_confirm {
            let mut team_name: String;
            loop {
                team_name = Input::with_prompt("Team name")
                    .initial_text(&project.name)
                    .interact()?;

                if teams.iter().all(|t| t.name != team_name) {
                    break;
                }

                println!("Team with '{}' name already exist", team_name);
            }

            let team_slug = Wizard::team_slug(&team_name);
            let people = self.github.get_org_members().await?;

            let members = MultiSelect::with_prompt(format!(
                    "Select members for the '{}' team\n(include yourself if you should be part of the team)",
                    &team_name
                ))
                .items(&people)
                .interact()?;

            let members: Vec<String> = members
                .into_iter()
                .map(|m| m.login.clone())
                .collect::<Vec<_>>();

            let permissions_action =
                self.select_permissions_action(&team_name, Some(&team_slug), &repositories_names)?;
            let create_team = Action::CreateTeam {
                name: team_name.clone(),
                repositories: repositories_names.clone(),
            };
            let add_members_to_team = Action::AddMembersToTeam {
                team_name,
                team_slug,
                members,
            };
            vec![create_team, add_members_to_team, permissions_action]
        } else {
            vec![]
        };

        actions.extend(create_team_actions);

        let additional_teams = Confirm::with_prompt("Do you want to add access for other teams to these repositories?\n(Consider adding tech-team for those repositories)")
            .interact()?;

        if additional_teams {
            let teams = MultiSelect::with_prompt("Select teams")
                .items(&teams)
                .interact()?;

            let permission_actions = teams.iter().flat_map(|team| {
                self.select_permissions_action(&team.name, Some(&team.slug), &repositories_names)
            });

            actions.extend(permission_actions);
        }

        let change_branches = Confirm::with_prompt(
            "Do you want to change default branches of selected repositories?",
        )
        .interact()?;

        if change_branches {
            let for_change =
                MultiSelect::with_prompt("Select repositories to change the default branch")
                    .items(&repositories)
                    .interact()?;
            for repo in for_change {
                let spinner = spinner::create_spinner(format!(
                    "Fetching branches for '{}' repository...",
                    repo.full_name
                ));
                let branches = self
                    .bitbucket
                    .get_repository_branches(&repo.full_name)
                    .await?;
                spinner.finish_with_message(format!(
                    "Fetched {} branches for '{}' repository!",
                    branches.len(),
                    repo.full_name
                ));

                let current_idx = branches.iter().position(|b| b.name == repo.mainbranch.name);
                let default_idx = branches.iter().position(|b| b.name == "development");

                let default_idx = match (default_idx, current_idx) {
                    (Some(idx), _) => idx,
                    (_, Some(idx)) => idx,
                    _ => 0,
                };

                let selected_branch = FuzzySelect::with_prompt(format!(
                    "Select new default branch for '{}' repository",
                    repo.full_name
                ))
                .items(&branches)
                .default(default_idx)
                .interact()?;
                let action = Action::SetRepositoryDefaultBranch {
                    repository_name: repo.full_name.clone(),
                    branch: selected_branch.name.clone(),
                };
                actions.push(action);
            }
        }

        let migration = Migration::new(&self.version, &actions);
        self.save_migration_file(&migration)?;

        Ok(WizardResult {
            actions,
            migration_file_path: self.output_path.clone(),
        })
    }

    fn select_permissions_action(
        &self,
        team_name: &str,
        team_slug: Option<&str>,
        repositories_names: &[String],
    ) -> Result<Action, anyhow::Error> {
        let permissions = vec![
            TeamRepositoryPermission::Pull,
            TeamRepositoryPermission::Triage,
            TeamRepositoryPermission::Push,
            TeamRepositoryPermission::Maintain,
        ];
        let permission = Select::with_prompt(format!(
            "Select permission to the repositories for '{}' team",
            &team_name
        ))
        .items(&permissions)
        .default(2)
        .interact()?
        .clone();

        Ok(Action::AssignRepositoriesToTeam {
            team_name: team_name.to_string(),
            team_slug: team_slug.map_or(Wizard::team_slug(team_name), |s| s.to_owned()),
            permission,
            repositories: repositories_names.to_vec(),
        })
    }

    async fn select_repositories(
        &self,
        project: &bitbucket::Project,
    ) -> Result<Vec<Repository>, anyhow::Error> {
        let spinner =
            spinner::create_spinner(format!("Fetching repositories from {} project", project));
        let repositories = self
            .bitbucket
            .get_project_repositories(project.get_key())
            .await?;
        spinner.finish_with_message(format!(
            "Fetched {} repositories from {} project!",
            repositories.len(),
            project
        ));
        let repositories =
            MultiSelect::with_prompt(format!("Select repositories from {} project", project))
                .items(&repositories)
                .interact()?;
        if repositories.is_empty() {
            return Err(anyhow!("At least one repository must be selected"));
        }

        let repositories = repositories.into_iter().cloned().collect();

        Ok(repositories)
    }

    async fn select_project(&self) -> Result<bitbucket::Project, anyhow::Error> {
        let spinner = spinner::create_spinner("Fetching projects from Bitbucket...");
        let projects = self.bitbucket.get_projects().await?;
        spinner.finish_with_message("Fetched!");
        let project = FuzzySelect::with_prompt("Select project")
            .items(&projects)
            .default(0)
            .interact()
            .expect("at least 1 project must be selected");

        Ok(project.clone())
    }

    fn save_migration_file(&self, migration: &Migration) -> Result<(), anyhow::Error> {
        if self.output_path.exists() {
            let overwrite = Confirm::with_prompt("Migration file already exists. Overwrite?")
                .default(false)
                .interact()?;

            if !overwrite {
                return Err(anyhow!("Migration file already exists"));
            }
        }
        let mut file = File::create(&self.output_path)?;

        serde_json::to_writer(&mut file, migration)?;

        Ok(())
    }

    fn team_slug(team_name: &str) -> String {
        let regex = regex::Regex::new(r"[^a-zA-Z0-9\-]").unwrap();
        regex
            .replace_all(&team_name.to_lowercase(), "-")
            .to_string()
    }
}
