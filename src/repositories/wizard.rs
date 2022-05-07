use std::{collections::HashSet, fs::File, path::PathBuf};

use crate::{
    bitbucket::{self, BitbucketApi, Repository as BitbucketRepository},
    github::{GithubApi, Repository as GitHubRepository, TeamRepositoryPermission},
    spinner,
};

use crate::bitbucket::{Branch, Repository};
use crate::config::{BitbucketConfig, GitHubConfig};
use crate::github::Team;
use crate::prompts::{Confirm, FuzzySelect, Input, MultiSelect, Select};
use crate::repositories::action::Action;
use crate::repositories::migrator::Migration;
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
    pub fn new(
        output_path: PathBuf,
        version: &str,
        bitbucket_cfg: BitbucketConfig,
        github_config: GitHubConfig,
    ) -> Self {
        Self {
            output_path,
            version: version.to_owned(),
            bitbucket: BitbucketApi::new(&bitbucket_cfg),
            github: GithubApi::new(&github_config),
        }
    }

    pub async fn run(&self) -> Result<WizardResult, anyhow::Error> {
        println!("Welcome to Bitbucket-GitHub Migration Wizard!");
        let project = self.select_project().await?;
        let bb_repos = self.select_repositories(&project).await?;

        let mut actions = vec![];

        let repositories_names: Vec<String> =
            bb_repos.iter().map(|r| r.full_name.to_owned()).collect();

        let gh_repos = self.fetch_github_repositories().await?;
        let already_migrated = Self::already_migrated_repo_names(&bb_repos, &gh_repos);
        let repositories = Self::select_repositories_to_continue(&bb_repos, &already_migrated)?;

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

        if let Some(migrate_action) = Self::ask_clone_repos(&repositories)? {
            actions.push(migrate_action);
        }

        let teams = self.fetch_github_teams().await?;

        println!("These teams already exist on GitHub:");
        teams.iter().for_each(|t| println!("  - {}", t.name));

        if let Some(new_team) = self
            .ask_create_team(&project.name, &repositories_names, &teams)
            .await?
        {
            actions.extend(new_team);
        }

        if let Some(team_actions) = self.ask_additional_teams(&repositories_names, &teams)? {
            actions.extend(team_actions);
        }

        if let Some(branch_actions) = self.ask_change_default_branch(&repositories).await? {
            actions.extend(branch_actions);
        }

        let migration = Migration::new(&self.version, &actions);
        self.save_migration_file(&migration)?;

        Ok(WizardResult {
            actions,
            migration_file_path: self.output_path.clone(),
        })
    }

    async fn ask_change_default_branch(
        &self,
        repositories: &[Repository],
    ) -> anyhow::Result<Option<Vec<Action>>> {
        let change_branches = Confirm::with_prompt(
            "Do you want to change default branches of selected repositories?",
        )
        .interact()?;

        if change_branches {
            let for_change =
                MultiSelect::with_prompt("Select repositories to change the default branch")
                    .items(repositories)
                    .interact()?;
            if for_change.is_empty() {
                println!("No repositories selected, skipping changing default branch...");
                return Ok(None);
            }
            let mut actions = vec![];
            for repo in for_change {
                let branches = self.fetch_repo_branches(repo).await?;

                let current_idx = branches
                    .iter()
                    .position(|b| b.name == repo.main_branch.name);
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

            Ok(Some(actions))
        } else {
            Ok(None)
        }
    }

    async fn fetch_repo_branches(&self, repo: &Repository) -> anyhow::Result<Vec<Branch>> {
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

        Ok(branches)
    }

    fn ask_additional_teams(
        &self,
        repositories_names: &[String],
        teams: &[Team],
    ) -> anyhow::Result<Option<Vec<Action>>> {
        let additional_teams = Confirm::with_prompt("Do you want to add access for other teams to these repositories?\n(Consider adding tech-team for those repositories)")
            .interact()?;

        if additional_teams {
            let teams = MultiSelect::with_prompt("Select teams")
                .items(teams)
                .interact()?;

            let permission_actions = teams
                .iter()
                .flat_map(|team| {
                    self.select_permissions_action(&team.name, Some(&team.slug), repositories_names)
                })
                .collect();

            Ok(Some(permission_actions))
        } else {
            Ok(None)
        }
    }

    async fn ask_create_team(
        &self,
        project_name: &str,
        repositories_names: &[String],
        existing_teams: &[Team],
    ) -> anyhow::Result<Option<Vec<Action>>> {
        let create_team_confirm =
            Confirm::with_prompt("Do you want to create a new team for selected repositories?")
                .interact()?;
        let create_team_actions = if create_team_confirm {
            let existing_teams = existing_teams.to_vec();
            let team_name = Input::with_prompt("Team name")
                .initial_text(project_name)
                .validate_with(move |input| {
                    if existing_teams.iter().any(|t| t.name == *input) {
                        Some(format!("Team with '{}' name already exist", input))
                    } else {
                        None
                    }
                })
                .interact()?;

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
                self.select_permissions_action(&team_name, Some(&team_slug), repositories_names)?;
            let create_team = Action::CreateTeam {
                name: team_name.clone(),
                repositories: repositories_names.to_vec(),
            };
            let add_members_to_team = Action::AddMembersToTeam {
                team_name,
                team_slug,
                members,
            };
            Some(vec![create_team, add_members_to_team, permissions_action])
        } else {
            None
        };

        Ok(create_team_actions)
    }

    async fn fetch_github_teams(&self) -> anyhow::Result<Vec<Team>> {
        let spinner = spinner::create_spinner("Fetching teams...");
        let teams = self.github.get_teams().await?;
        spinner.finish_with_message(format!("Fetched {} teams from GitHub", teams.len()));

        Ok(teams)
    }

    fn ask_clone_repos(repositories: &[BitbucketRepository]) -> anyhow::Result<Option<Action>> {
        let migrate_repos = Confirm::with_prompt(
            "Do you want to mirror selected repositories from Bitbucket to GitHub?",
        )
        .interact()?;
        if migrate_repos {
            let migrate_action = Action::MigrateRepositories {
                repositories: repositories.iter().map(|r| r.clone().into()).collect(),
            };
            Ok(Some(migrate_action))
        } else {
            Ok(None)
        }
    }

    fn select_repositories_to_continue(
        repositories: &[BitbucketRepository],
        already_migrated: &[&String],
    ) -> anyhow::Result<Vec<BitbucketRepository>> {
        let repositories: Vec<BitbucketRepository> = if !already_migrated.is_empty() {
            let intersection_names = already_migrated
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
                0 => repositories.to_vec(),
                1 => repositories
                    .iter()
                    .filter(|r| !already_migrated.contains(&&r.full_name))
                    .cloned()
                    .collect::<Vec<_>>(),
                _ => unreachable!(),
            }
        } else {
            repositories.to_vec()
        };

        Ok(repositories)
    }

    fn already_migrated_repo_names<'a>(
        bb_repositories: &'a [BitbucketRepository],
        gh_repositories: &'a [GitHubRepository],
    ) -> Vec<&'a String> {
        let spinner = spinner::create_spinner("Checking for existing repositories in GitHub...");
        let selected_repo_names = bb_repositories
            .iter()
            .map(|r| &r.full_name)
            .collect::<HashSet<_>>();

        let existing_repo_names = gh_repositories
            .iter()
            .map(|r| &r.full_name)
            .collect::<HashSet<_>>();

        let intersection = selected_repo_names
            .intersection(&existing_repo_names)
            .cloned()
            .collect::<Vec<_>>();
        spinner.finish_with_message(format!(
            "{} of the {} selected repositories already exist on GitHub",
            intersection.len(),
            selected_repo_names.len(),
        ));

        intersection
    }

    async fn fetch_github_repositories(&self) -> anyhow::Result<Vec<GitHubRepository>> {
        let spinner = spinner::create_spinner("Fetching existing repositories from GitHub...");
        let github_repositories = self.github.get_repositories().await?;
        spinner.finish_with_message(format!(
            "Fetched {} existing repositories from GitHub!",
            github_repositories.len()
        ));

        Ok(github_repositories)
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
    ) -> Result<Vec<BitbucketRepository>, anyhow::Error> {
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
