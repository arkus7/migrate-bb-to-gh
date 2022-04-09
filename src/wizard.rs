use std::{fs::File, path::PathBuf};

use dialoguer::{theme::ColorfulTheme, Confirm, FuzzySelect, Input, MultiSelect, Select};

use crate::{
    bitbucket::{self, Repository},
    github::{self, TeamRepositoryPermission},
    migrator::Action,
    spinner,
};

use anyhow::anyhow;

pub struct Wizard {
    output_path: PathBuf,
    theme: ColorfulTheme,
}

#[derive(Debug)]
pub struct WizardResult {
    pub actions: Vec<Action>,
    pub migration_file_path: PathBuf,
}

impl Wizard {
    pub fn new(output_path: PathBuf) -> Self {
        Self {
            output_path,
            theme: ColorfulTheme::default(),
        }
    }

    pub async fn run(&self) -> Result<WizardResult, anyhow::Error> {
        let project = self.select_project().await?;
        let repositories = self.select_repositories(&project).await?;

        let repositories_names: Vec<String> = repositories
            .iter()
            .map(|r| r.full_name.to_owned())
            .collect();

        let migrate_action = Action::MigrateRepositories { repositories };
        let team_action = self.select_team(project, repositories_names).await?;

        let actions = vec![migrate_action, team_action];

        self.save_migration_file(&actions)?;

        Ok(WizardResult {
            actions,
            migration_file_path: self.output_path.clone(),
        })
    }

    async fn select_team(
        &self,
        project: bitbucket::Project,
        repositories_names: Vec<String>,
    ) -> Result<Action, anyhow::Error> {
        let team_choice = Select::with_theme(&self.theme)
            .with_prompt("Team settings")
            .item("Create new team")
            .item("Select existing team")
            .default(0)
            .interact()?;
        let team_action = match team_choice {
            0 => {
                let team_name: String = Input::with_theme(&self.theme)
                    .with_prompt("Team name")
                    .with_initial_text(&project.name)
                    .interact()?;

                Action::CreateTeam {
                    name: team_name,
                    repositories: repositories_names,
                }
            }
            1 => {
                let spinner = spinner::create_spinner("Fetching teams...");
                let teams = github::get_teams().await?;
                spinner.finish_with_message(format!("Fetched {} teams", teams.len()));

                let team_selection = FuzzySelect::with_theme(&self.theme)
                    .with_prompt("Select team")
                    .items(&teams)
                    .default(0)
                    .interact()?;

                let team = teams.get(team_selection).expect("Invalid team selected");

                let permission = Select::with_theme(&self.theme)
                    .with_prompt("Select permission to the repositories for selected team")
                    .item("Read")
                    .item("Write")
                    .default(1)
                    .interact()?;

                let permission = match permission {
                    0 => TeamRepositoryPermission::Pull,
                    1 => TeamRepositoryPermission::Push,
                    _ => unreachable!(),
                };

                Action::AssignRepositoriesToTeam {
                    team_name: team.name.clone(),
                    team_slug: team.slug.clone(),
                    permission,
                    repositories: repositories_names,
                }
            }
            _ => unreachable!(),
        };
        Ok(team_action)
    }

    async fn select_repositories(
        &self,
        project: &bitbucket::Project,
    ) -> Result<Vec<Repository>, anyhow::Error> {
        let spinner =
            spinner::create_spinner(format!("Fetching repositories from {} project", project));
        let repositories = bitbucket::get_repositories(project.get_key()).await?;
        spinner.finish_with_message("Fetched!");
        let selection = MultiSelect::with_theme(&self.theme)
            .with_prompt(format!("Select repositories from {} project", project))
            .items(&repositories)
            .interact()?;
        if selection.is_empty() {
            return Err(anyhow!("At least one repository must be selected"));
        }
        let repositories: Vec<Repository> = selection
            .into_iter()
            .flat_map(|idx| repositories.get(idx))
            .cloned()
            .collect::<Vec<_>>();
        Ok(repositories)
    }

    async fn select_project(&self) -> Result<bitbucket::Project, anyhow::Error> {
        let spinner = spinner::create_spinner("Fetching projects from Bitbucket...");
        let projects = bitbucket::get_projects().await?;
        spinner.finish_with_message("Fetched!");
        let selection = FuzzySelect::with_theme(&self.theme)
            .with_prompt("Select project")
            .items(&projects)
            .default(0)
            .interact()
            .expect("at least 1 project must be selected");
        let project = projects
            .get(selection)
            .expect("No project selected")
            .clone();
        Ok(project)
    }

    fn save_migration_file(&self, actions: &[Action]) -> Result<(), anyhow::Error> {
        if self.output_path.exists() {
            let overwrite = Confirm::with_theme(&self.theme)
                .with_prompt("Migration file already exists. Overwrite?")
                .default(false)
                .interact()?;

            if !overwrite {
                return Err(anyhow!("Migration file already exists"));
            }
        }
        let mut file = File::create(&self.output_path)?;

        serde_json::to_writer(&mut file, actions)?;

        Ok(())
    }
}
