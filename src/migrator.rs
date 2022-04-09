use std::{fs::File, path::PathBuf, process::Command, thread, time::Duration};

use dialoguer::Confirm;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use serde::{Deserialize, Serialize};
use tempdir::TempDir;

use crate::{
    bitbucket::Repository,
    github::{self, TeamRepositoryPermission},
    spinner,
};

use anyhow::anyhow;

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    MigrateRepositories {
        repositories: Vec<Repository>,
    },
    CreateTeam {
        name: String,
        repositories: Vec<String>,
    },
    AssignRepositoriesToTeam {
        team_name: String,
        team_slug: String,
        permission: TeamRepositoryPermission,
        repositories: Vec<String>,
    },
}

impl Action {
    fn describe(&self) -> String {
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
                    "Assign {} repositories to team {} ({}):\n {}",
                    repositories.len(),
                    team_name,
                    permission,
                    repositories_list
                )
            }
        }
    }

    async fn run(&self) -> Result<(), anyhow::Error> {
        match self {
            Action::CreateTeam { name, repositories } => create_team(name, repositories).await?,
            Action::MigrateRepositories { repositories } => {
                migrate_repositories(repositories).await?
            }
            Action::AssignRepositoriesToTeam {
                team_name,
                team_slug,
                permission,
                repositories,
            } => {
                assign_repositories_to_team(team_name, team_slug, permission, repositories).await?
            }
        }
        Ok(())
    }
}

pub async fn migrate(migration_file: &PathBuf) -> Result<(), anyhow::Error> {
    let file = File::open(migration_file)?;
    let actions: Vec<Action> = serde_json::from_reader(file)
        .map_err(|e| anyhow!("Error when parsing {:?} file: {}", migration_file, e))?;

    // println!(
    //     "There are {} actions to be done during migration:",
    //     actions.len()
    // );
    // for (idx, action) in actions.iter().enumerate() {
    //     println!("{}. {}", &idx + 1, action.describe());
    // }

    println!("{}", describe_actions(&actions));

    let confirmed = Confirm::new()
        .with_prompt("Are you sure you want to migrate?")
        .interact()?;

    if !confirmed {
        return Err(anyhow!("Migration canceled"));
    }

    for action in actions {
        let _ = action.run().await?;
    }

    Ok(())
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

async fn create_team(name: &str, repositories: &Vec<String>) -> Result<(), anyhow::Error> {
    let spinner = spinner::create_spinner(format!("Creating team {}", name));
    // github::create_team(name, repositories).await?;
    std::thread::sleep(Duration::from_secs(2));
    spinner.finish_with_message("Created!");
    Ok(())
}

async fn migrate_repositories(repositories: &Vec<Repository>) -> Result<(), anyhow::Error> {
    println!("Migrating {} repositories", repositories.len());
    let multi_progress = MultiProgress::new();

    multi_progress.println("Migrating...");

    let handles = repositories
        .iter()
        .map(|repo| migrate_repository(repo, &multi_progress))
        .collect::<Vec<_>>();
    for h in handles {
        let _ = h.await??;
    }

    multi_progress.clear()?;
    Ok(())
}

async fn assign_repositories_to_team(
    team_name: &str,
    team_slug: &String,
    permission: &TeamRepositoryPermission,
    repositories: &Vec<String>,
) -> Result<(), anyhow::Error> {
    let spinner = spinner::create_spinner(format!("Assigning repositories to team {}", team_name));
    // github::assign_repository_to_team(team_slug, permission, repositories).await?;
    std::thread::sleep(Duration::from_secs(2));
    spinner.finish_with_message("Assigned!");
    Ok(())
}

fn migrate_repository<'a>(
    repository: &Repository,
    multi_progress: &MultiProgress,
) -> tokio::task::JoinHandle<Result<(), anyhow::Error>> {
    let style = ProgressStyle::with_template(
        "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}",
    )
    .unwrap()
    .progress_chars("##-");
    let pb = multi_progress.add(ProgressBar::new(10));
    pb.set_prefix(format!("[{}] ", repository.full_name));
    pb.set_style(style);
    let repo = repository.clone();
    tokio::spawn(async move {
        let tempdir = TempDir::new(&repo.name)?;
        pb.set_message(format!(
            "[1/?] Cloning {} into {}",
            repo.full_name,
            tempdir.path().display()
        ));

        let _ = clone_mirror(&repo, &tempdir);
        pb.inc(1);

        pb.set_message(format!("[2/?] Creating {} repository in GitHub", repo.full_name));
        thread::sleep(Duration::from_secs(2));
        pb.finish_with_message("Migrated!");

        Ok(())
    })
}

fn clone_mirror(
    repo: &Repository,
    tempdir: &TempDir,
) -> Result<(), anyhow::Error> {
    let clone_url = repo.get_ssh_url().expect("Cannot find SSH clone url");

    let clone_command = Command::new("git")
        .arg("clone")
        .arg("--mirror")
        .arg(&clone_url)
        .arg(tempdir.path())
        .output()?;

    if !clone_command.status.success() {
        return Err(anyhow!(
            "Error when cloning {} into {}: {}",
            clone_url,
            tempdir.path().display(),
            clone_command.status
        ));
    }

    Ok(())
}
