use std::{collections::HashSet, fs::File, path::PathBuf};

use dialoguer::{theme::ColorfulTheme, Confirm, FuzzySelect, Input, MultiSelect, Select};

use crate::{
    bitbucket::{self, Repository},
    github::{self, TeamRepositoryPermission},
    migrator::{Action, Migration},
    spinner,
};

use anyhow::anyhow;

pub struct Wizard {
    output_path: PathBuf,
    theme: ColorfulTheme,
    version: String,
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
            theme: ColorfulTheme::default(),
            version: version.to_owned(),
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

        let assign_action = if let Action::CreateTeam { name, repositories } = &team_action {
            Some(self.select_permissions_action(name, None, repositories)?)
        } else {
            None
        };

        let mut actions = vec![migrate_action, team_action];
        if let Some(assign_action) = assign_action {
            actions.push(assign_action);
        }

        let migration = Migration::new(&self.version, &actions);
        self.save_migration_file(&migration)?;

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
                    .with_prompt("Select team\n[You can fuzzy search here by typing]")
                    .items(&teams)
                    .default(0)
                    .interact()?;

                let team = teams.get(team_selection).expect("Invalid team selected");

                self.select_permissions_action(&team.name, Some(&team.slug), &repositories_names)?
            }
            _ => unreachable!(),
        };
        Ok(team_action)
    }

    fn select_permissions_action(
        &self,
        team_name: &str,
        team_slug: Option<&str>,
        repositories_names: &[String],
    ) -> Result<Action, anyhow::Error> {
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
        let repositories = bitbucket::get_project_repositories(project.get_key()).await?;
        spinner.finish_with_message(format!(
            "Fetched {} repositories from {} project!",
            repositories.len(),
            project
        ));
        let selection = MultiSelect::with_theme(&self.theme)
            .with_prompt(format!("Select repositories from {} project\n[Space = select, Enter = continue]", project))
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

        let spinner = spinner::create_spinner("Fetching existing repositories from GitHub...");
        let github_repositories = github::get_repositories().await?;
        spinner.finish_with_message(format!(
            "Fetched {} existing repositories from GitHub!",
            github_repositories.len()
        ));

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
        let repositories = if !intersection.is_empty() {
            let intersection_names = intersection
                .iter()
                .map(|n| n.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            let msg = format!("The following repositories already exist in GitHub: {}\nDo you want to overwrite them?", intersection_names);
            let overwrite = Select::with_theme(&self.theme)
                .with_prompt(msg)
                .item("Overwrite existing repositories")
                .item("Skip existing repositories")
                .default(1)
                .interact()?;
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

        Ok(repositories)
    }

    async fn select_project(&self) -> Result<bitbucket::Project, anyhow::Error> {
        let spinner = spinner::create_spinner("Fetching projects from Bitbucket...");
        let projects = bitbucket::get_projects().await?;
        spinner.finish_with_message("Fetched!");
        let selection = FuzzySelect::with_theme(&self.theme)
            .with_prompt("Select project\n[You can fuzzy search here by typing]")
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

    fn save_migration_file(&self, migration: &Migration) -> Result<(), anyhow::Error> {
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
