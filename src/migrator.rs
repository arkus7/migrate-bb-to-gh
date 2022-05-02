use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::{fs, fs::File, path::Path, process::Command, time::Instant};

use dialoguer::Confirm;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use serde::{Deserialize, Serialize};
use tempdir::TempDir;

use crate::{
    bitbucket::{self},
    config::CONFIG,
    github::{self, TeamRepositoryPermission},
    spinner,
};

use anyhow::{anyhow, Context};
use crate::config::GitHubConfig;
use crate::github::GithubApi;

#[derive(Serialize, Deserialize, Debug)]
pub struct Migration {
    version: String,
    actions: Vec<Action>,
}

impl Migration {
    pub fn new(version: &str, actions: &[Action]) -> Self {
        Self {
            version: version.to_string(),
            actions: actions.to_vec(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Repository {
    clone_link: String,
    name: String,
    full_name: String,
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

pub struct Migrator {
    migration_file: PathBuf,
    version: String,
    github: GithubApi,
}

impl Migrator {
    pub fn new(migration_file: &PathBuf, version: &str) -> Self {
        Self {
            migration_file: migration_file.clone(),
            version: version.to_string(),
            github: GithubApi::new(&CONFIG.github),
        }
    }

    async fn add_members_to_team(
        &self,
        team_name: &str,
        team_slug: &str,
        members: &[String],
    ) -> anyhow::Result<()> {
        println!("Adding {} members to {} team", members.len(), team_name, );
        let pb = ProgressBar::new(members.len() as u64);
        pb.set_style(progress_bar_style());
        for member in members {
            self.github.update_team_membership(team_slug, member).await?;
            pb.inc(1);
        }
        Ok(())
    }

    async fn set_default_branch(&self, repo_name: &str, branch: &str) -> anyhow::Result<()> {
        println!(
            "Setting '{}' as default branch for '{}' repository",
            branch, repo_name,
        );
        let spinner = spinner::create_spinner(format!(
            "Setting '{}' as default branch for '{}' repository",
            branch, repo_name
        ));
        self.github.set_repository_default_branch(repo_name, branch).await?;
        spinner.finish_with_message(format!(
            "Set '{}' as default branch for '{}' repository",
            branch, repo_name
        ));
        Ok(())
    }

    pub async fn migrate(self, migration_file: &Path, version: &str) -> Result<(), anyhow::Error> {
        let file = File::open(migration_file)?;
        let migration: Migration = serde_json::from_reader(file).with_context(|| format!("Error when parsing {:?} file.\nIs this a JSON file?\nDoes the version match the program version ({})?\nConsider re-generating the migration file with `wizard` subcommand.", migration_file, version))?;
        if migration.version != version {
            return Err(anyhow!("Migration file version is not compatible with current version, expected: {}, found: {}", version, migration.version));
        }
        let actions = migration.actions;

        println!("{}", self.describe_actions(&actions));

        let confirmed = Confirm::new()
            .with_prompt("Are you sure you want to migrate?")
            .interact()?;

        if !confirmed {
            return Err(anyhow!("Migration canceled"));
        }

        let start = Instant::now();
        for action in actions {
            let _ = self.run(&action).await?;
        }
        let duration = start.elapsed();

        println!("Migration completed in {} seconds!", duration.as_secs());

        Ok(())
    }

    pub fn describe_actions(&self, actions: &[Action]) -> String {
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

    async fn create_team(&self, name: &str, repositories: &[String]) -> Result<(), anyhow::Error> {
        let spinner = spinner::create_spinner(format!("Creating team {}", name));
        self.github.create_team(name, repositories).await?;
        spinner.finish_with_message("Created!");
        Ok(())
    }

    async fn migrate_repositories(&self, repositories: &[Repository]) -> Result<(), anyhow::Error> {
        println!("Migrating {} repositories", repositories.len());
        let multi_progress = MultiProgress::new();

        let push_key = &CONFIG.git.push_ssh_key;
        let pull_key = &CONFIG.git.pull_ssh_key;

        let tmp_dir = TempDir::new("migrate-bb-to-gh")?;

        let push_key_path = self.store_ssh_key("push", push_key, tmp_dir.path())?;
        let pull_key_path = self.store_ssh_key("pull", pull_key, tmp_dir.path())?;

        let handles = repositories
            .iter()
            .map(|repo| self.migrate_repository(repo, &multi_progress, &pull_key_path, &push_key_path));

        let handles = futures::future::join_all(handles).await;
        for h in handles {
            let res = h;
            if let Err(e) = res {
                eprintln!("Failed to migrate repository: {}", e)
            }
        }

        multi_progress.clear()?;
        Ok(())
    }

    fn store_ssh_key(&self, name: &str, key: &str, path: &Path) -> Result<PathBuf, anyhow::Error> {
        let file_path = path.join(name);
        let mut key_file = File::create(&file_path)?;
        key_file.write_all(key.as_ref())?;

        let mut perms = key_file.metadata()?.permissions();
        perms.set_mode(0o400);
        key_file.set_permissions(perms)?;

        Ok(file_path)
    }

    async fn assign_repositories_to_team(
        &self,
        team_name: &str,
        team_slug: &str,
        permission: &TeamRepositoryPermission,
        repositories: &[String],
    ) -> Result<(), anyhow::Error> {
        println!(
            "Assigning {} repositories to team {} ({})",
            repositories.len(),
            team_name,
            permission
        );
        let pb = ProgressBar::new(repositories.len() as u64);
        pb.set_style(progress_bar_style());
        for repository in repositories {
            self.github.assign_repository_to_team(team_slug, permission, repository).await?;
            pb.inc(1);
        }
        Ok(())
    }

    async fn migrate_repository(
        &self,
        repository: &Repository,
        multi_progress: &MultiProgress,
        pull_key_path: &Path,
        push_key_path: &Path,
    ) -> Result<Repository, anyhow::Error> {
        let steps_count = 4;
        let pb = multi_progress.add(ProgressBar::new(steps_count));
        pb.set_prefix(format!("[{}] ", repository.full_name));
        pb.set_style(progress_bar_style());

        let repo = repository.clone();
        let pull_key_path = pull_key_path.to_path_buf();
        let push_key_path = push_key_path.to_path_buf();

        // tokio::spawn(async move {
            let temp_dir = TempDir::new(&repo.full_name.to_owned().replace('/', "_"))?;
            pb.set_message(format!("[1/{}] Cloning {}", steps_count, repo.full_name, ));
            let _ = self.clone_mirror(&repo.clone_link, temp_dir.path(), &pull_key_path);
            pb.inc(1);

            pb.set_message(format!(
                "[2/{}] Creating {} repository in GitHub",
                steps_count, repo.full_name
            ));
            let gh_repo =
                self.github.create_repository(&repo.full_name.to_owned().replace("moodup/", "")).await?;
            pb.inc(1);

            pb.set_message(format!(
                "[3/{}] Mirroring {} repository to GitHub",
                steps_count, repo.full_name
            ));
            let _ = self.push_mirror(temp_dir.path(), &gh_repo.ssh_url, &push_key_path)?;
            pb.inc(1);

            pb.set_message(format!(
                "[4/{}] Deleting {} repository from temp directory",
                steps_count, repo.full_name
            ));
            temp_dir.close()?;

            pb.finish_with_message("✅ Migrated successfully!");

            Ok(repo)
        // })
    }

    fn clone_mirror(
        &self,
        remote_url: &str,
        target_path: &Path,
        key_path: &Path,
    ) -> Result<(), anyhow::Error> {
        let ssh_command = self.prepare_ssh_command(key_path)?;
        let clone_command = Command::new("git")
            .arg("-c")
            .arg(format!("core.sshCommand={}", ssh_command))
            .arg("clone")
            .arg("--mirror")
            .arg(remote_url)
            .arg(target_path)
            .output()?;

        // println!("{}", String::from_utf8(clone_command.stdout)?);

        if !clone_command.status.success() {
            let err_output = String::from_utf8(clone_command.stderr)?;
            return Err(anyhow!(
            "Error when cloning {} into {}: {}\noutput: {}",
            remote_url,
            target_path.display(),
            clone_command.status,
            err_output
        ));
        }

        Ok(())
    }

    fn prepare_ssh_command(&self, key_path: &Path) -> Result<String, anyhow::Error> {
        let cmd = format!(
            "ssh -i '{private_key_file}' -o IdentitiesOnly=yes -o StrictHostKeyChecking=no -o UserKnownHostsFile='/dev/null' -F '/dev/null'",
            private_key_file = fs::canonicalize(key_path)?.display()
        );
        Ok(cmd)
    }

    fn push_mirror(&self, repo_path: &Path, remote_url: &str, key_path: &Path) -> Result<(), anyhow::Error> {
        let ssh_command = self.prepare_ssh_command(key_path)?;
        let push_command = Command::new("git")
            .arg("-c")
            .arg(format!("core.sshCommand={}", ssh_command))
            .arg("push")
            .arg("--mirror")
            .arg(remote_url)
            .current_dir(repo_path)
            .output()?;

        // println!("{}", String::from_utf8(push_command.stdout)?);

        if !push_command.status.success() {
            let err_output = String::from_utf8(push_command.stderr)?;
            return Err(anyhow!(
            "Error when pushing {} to {}: {}\noutput: {}",
            repo_path.display(),
            remote_url,
            push_command.status,
            err_output
        ));
        }

        Ok(())
    }

    async fn run(&self, action: &Action) -> Result<(), anyhow::Error> {
        match action {
            Action::CreateTeam { name, repositories } => self.create_team(name, repositories).await?,
            Action::MigrateRepositories { repositories } => {
                self.migrate_repositories(repositories).await?
            }
            Action::AssignRepositoriesToTeam {
                team_name,
                team_slug,
                permission,
                repositories,
            } => {
                self.assign_repositories_to_team(team_name, team_slug, permission, repositories).await?
            }
            Action::AddMembersToTeam {
                team_name,
                team_slug,
                members,
            } => self.add_members_to_team(team_name, team_slug, members).await?,
            Action::SetRepositoryDefaultBranch {
                repository_name,
                branch,
            } => self.set_default_branch(repository_name, branch).await?,
        }
        Ok(())
    }
}

fn progress_bar_style() -> ProgressStyle {
    ProgressStyle::with_template("[{elapsed}] {bar:20.cyan/blue} {pos:>7}/{len:7} {msg}")
        .unwrap()
        .progress_chars("##-")
}
