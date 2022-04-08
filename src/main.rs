use std::borrow::Cow;

use anyhow::anyhow;
use bitbucket::Repository;
use dialoguer::{theme::ColorfulTheme, FuzzySelect, Input, MultiSelect, Select};
use github::TeamRepositoryPermission;
use indicatif::ProgressBar;
use serde::{Deserialize, Serialize};

mod bitbucket;
mod github;

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
enum Action {
    MigrateRepositories {
        repositories: Vec<Repository>,
    },
    CreateTeam {
        name: String,
        repositories: Vec<String>,
    },
    AssignRepositoriesToTeam {
        team_name: String,
        team_id: u32,
        permission: TeamRepositoryPermission,
        repositories: Vec<String>,
    },
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let theme = ColorfulTheme::default();
    let spinner = create_spinner("Fetching projects from Bitbucket...");
    let projects = bitbucket::get_projects().await?;

    spinner.finish_with_message("Fetched!");

    let selection = FuzzySelect::with_theme(&theme)
        .with_prompt("Select project")
        .items(&projects)
        .default(0)
        .interact()
        .expect("at least 1 project must be selected");

    let project = projects.get(selection).expect("No project selected");

    let spinner = create_spinner(format!("Fetching repositories from {} project", project));
    let repositories = bitbucket::get_repositories(project.get_key()).await?;
    spinner.finish_with_message("Fetched!");

    let selection = MultiSelect::with_theme(&theme)
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

    dbg!(&repositories);

    let repositories_names: Vec<String> = repositories
        .iter()
        .map(|r| r.full_name.to_owned())
        .collect();

    let migrate_action = Action::MigrateRepositories { repositories };

    let team_choice = Select::with_theme(&theme)
        .with_prompt("Team settings")
        .item("Create new team")
        .item("Select existing team")
        .default(0)
        .interact()?;

    let team_action = match team_choice {
        0 => {
            let team_name: String = Input::with_theme(&theme)
                .with_prompt("Team name")
                .with_initial_text(&project.name)
                .interact()?;

            Action::CreateTeam {
                name: team_name,
                repositories: repositories_names,
            }
        }
        1 => {
            let spinner = create_spinner("Fetching teams...");
            let teams = github::get_teams().await?;
            spinner.finish_with_message(format!("Fetched {} teams", teams.len()));

            let team_selection = FuzzySelect::with_theme(&theme)
                .with_prompt("Select team")
                .items(&teams)
                .default(0)
                .interact()?;

            let team = teams.get(team_selection).expect("Invalid team selected");

            let permission = Select::with_theme(&theme)
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
                team_id: team.id,
                permission,
                repositories: repositories_names,
            }
        }
        _ => unreachable!(),
    };

    let actions = vec![migrate_action, team_action];

    dbg!(&actions);

    Ok(())
}

fn create_spinner<T: Into<Cow<'static, str>>>(message: T) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.enable_steady_tick(120);

    pb.set_message(message);

    pb
}
