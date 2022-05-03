use std::{
    collections::HashSet,
    fs::File,
    path::{Path, PathBuf},
    str::FromStr,
};

use crate::prompts::{Confirm, FuzzySelect, Input, MultiSelect};
use anyhow::{anyhow, Ok};

use crate::bitbucket::BitbucketApi;
use crate::config::CONFIG;
use crate::github::GithubApi;
use crate::{
    bitbucket,
    circleci::{
        api::Context,
        migrate::{Action, EnvVar, Migration},
    },
    github::{self, FileContents, Repository, Team},
    spinner,
};

use super::{api, config::Config};

pub struct Wizard {
    output: PathBuf,
    version: String,
    bitbucket: BitbucketApi,
    github: GithubApi,
}

pub struct WizardResult {
    pub actions: Vec<Action>,
    pub migration_file_path: PathBuf,
}

impl Wizard {
    pub fn new(output: &Path, version: &str) -> Self {
        Self {
            output: output.to_path_buf(),
            version: version.to_owned(),
            bitbucket: BitbucketApi::new(&CONFIG.bitbucket),
            github: GithubApi::new(&CONFIG.github),
        }
    }

    pub async fn run(&self) -> anyhow::Result<WizardResult> {
        println!("Welcome to CircleCi Migration Wizard!");
        let team = self.select_team().await?;
        let repositories = self.select_repositories(&team).await?;

        let spinner = spinner::create_spinner("Fetching GitHub contexts from CircleCI...");
        let gh_contexts = api::get_contexts(api::Vcs::GitHub).await?;
        spinner.finish_with_message(format!(
            "Found {} contexts defined in GitHub org",
            gh_contexts.len()
        ));
        let spinner = spinner::create_spinner("Fetching Bitbucket contexts from CircleCI...");
        let bb_contexts = api::get_contexts(api::Vcs::Bitbucket).await?;
        spinner.finish_with_message(format!(
            "Found {} contexts defined in Bitbucket org",
            bb_contexts.len()
        ));

        let mut actions: Vec<Action> = vec![];
        for repository in repositories {
            println!();
            println!("Configuring {} repository...", &repository.full_name);
            let config = self.check_config_exists(&repository).await?;
            if config.is_none() {
                println!("No config found for {}, skipping...", repository.full_name);
                continue;
            }

            let config = self.parse_config(&config.unwrap())?;

            if let Some(move_envs_action) = self.move_env_vars(&repository).await? {
                actions.push(move_envs_action);
            }

            let defined_contexts: HashSet<_> = actions
                .iter()
                .filter(|a| matches!(a, Action::CreateContext { .. }))
                .map(|a| match a {
                    Action::CreateContext { name, .. } => name.to_owned(),
                    _ => unreachable!(),
                })
                .collect();

            let create_contexts_actions = self
                .create_contexts_actions(&config, &gh_contexts, &bb_contexts, &defined_contexts)
                .await?;
            actions.extend(create_contexts_actions);

            if let Some(start_build_action) = self.start_build(&repository).await? {
                actions.push(start_build_action);
            }
        }

        let migration = Migration::new(&self.version, &actions);

        self.save_migration_file(&migration)?;

        Ok(WizardResult {
            actions,
            migration_file_path: self.output.clone(),
        })
    }

    async fn move_env_vars(&self, repository: &Repository) -> anyhow::Result<Option<Action>> {
        let mut repository_name = repository.full_name.clone();
        let spinner = spinner::create_spinner(format!(
            "Fetching {} environment variables",
            &repository.name
        ));
        let mut env_vars: Vec<_> =
            api::get_env_vars(api::Vcs::Bitbucket, &repository.full_name)
                .await?
                .into_iter()
                .map(|e| e.name)
                .collect();
        spinner.finish_with_message(format!(
            "Found {} environment variables in '{}' project",
            env_vars.len(),
            &repository.name
        ));

        if env_vars.is_empty() {
            println!("No environment variables found in '{}' project, making sure we're checking right project..", &repository.name);
            let spinner = spinner::create_spinner(format!(
                "Fetching {} repository from Bitbucket",
                &repository.name
            ));
            let bb_repo = self.bitbucket.get_repository(&repository.full_name).await?;
            spinner.finish_with_message(format!("Found {:?} repository in Bitbucket", bb_repo));
            if bb_repo.is_none() {
                let manually_map = Confirm::with_prompt(format!("No repository named {} found in Bitbucket, do you want to manually map it?", &repository.name))
                    .interact()?;

                if !manually_map {
                    println!(
                        "Skipping moving env variables of {} repository...",
                        &repository.name
                    );
                    return Ok(None);
                }

                let project = self.select_project().await?;
                let spinner = spinner::create_spinner(format!(
                    "Fetching repositories from {} project",
                    project
                ));
                let repositories = self
                    .bitbucket
                    .get_project_repositories(project.get_key())
                    .await?;
                spinner.finish_with_message(format!(
                    "Fetched {} repositories from {} project!",
                    repositories.len(),
                    project
                ));
                let bb_repo = FuzzySelect::with_prompt(format!(
                    "Select repository from {} project",
                    project
                ))
                    .items(&repositories)
                    .interact_opt()?;

                if bb_repo.is_none() {
                    println!("No repository selected, skipping...");
                    return Ok(None);
                }

                let bb_repo = bb_repo.unwrap();
                let spinner = spinner::create_spinner(format!(
                    "Fetching {} environment variables",
                    &bb_repo.name
                ));
                let bb_env_vars =
                    api::get_env_vars(api::Vcs::Bitbucket, &bb_repo.full_name).await?;
                spinner.finish_with_message(format!(
                    "Found {} env variables for {} repository",
                    bb_env_vars.len(),
                    &bb_repo.name
                ));
                env_vars = bb_env_vars.into_iter().map(|e| e.name).collect();
                repository_name = bb_repo.full_name.clone();
            }
        }

        if env_vars.is_empty() {
            println!(
                "No environment variables found for {}, skipping...",
                repository_name
            );
            return Ok(None);
        }

        println!(
            "Found {} environment variables in '{}' project:\n{}",
            env_vars.len(),
            repository_name,
            env_vars
                .iter()
                .map(|e| format!("  {}", e))
                .collect::<Vec<_>>()
                .join("\n")
        );
        let move_envs = Confirm::with_prompt("Do you want to move the environment variables from Bitbucket to GitHub organization?")
            .default(true)
            .interact()?;
        let action = if move_envs {
            let env_vars = self.select_env_vars(&env_vars).await?;
            let action = Action::MoveEnvironmentalVariables {
                from_repository_name: repository_name.clone(),
                to_repository_name: repository.full_name.clone(),
                env_vars,
            };
            Some(action)
        } else {
            None
        };

        Ok(action)
    }

    async fn select_project(&self) -> Result<bitbucket::Project, anyhow::Error> {
        let spinner = spinner::create_spinner("Fetching projects from Bitbucket...");
        let projects = self.bitbucket.get_projects().await?;
        spinner.finish_with_message("Fetched!");
        let project = FuzzySelect::with_prompt("Select project")
            .items(&projects)
            .default(0)
            .interact()
            .expect("at least 1 project must be selected")
            .clone();

        Ok(project)
    }

    async fn select_team(&self) -> anyhow::Result<Team> {
        let spinner = spinner::create_spinner("Fetching teams...");
        let teams = self.github.get_teams().await?;
        spinner.finish_with_message(format!("Fetched {} teams", teams.len()));

        let team = FuzzySelect::with_prompt("Select team")
            .items(&teams)
            .default(0)
            .interact()?;

        Ok(team.clone())
    }

    async fn select_repositories(&self, team: &Team) -> anyhow::Result<Vec<Repository>> {
        let spinner =
            spinner::create_spinner(format!("Fetching repositories from {} team", &team.name));
        let repositories = self.github.get_team_repositories(&team.slug).await?;
        spinner.finish_with_message("Fetched!");
        let selection =
            MultiSelect::with_prompt(format!("Select repositories from {} team", &team.name))
                .items(&repositories)
                .interact()?;
        if selection.is_empty() {
            return Err(anyhow!("At least one repository must be selected"));
        }
        let repositories: Vec<Repository> = selection.into_iter().cloned().collect::<Vec<_>>();
        Ok(repositories)
    }

    async fn check_config_exists(
        &self,
        repo: &Repository,
    ) -> anyhow::Result<Option<FileContents>> {
        const CONFIG_PATH: &str = ".circleci/config.yml";

        let spinner = spinner::create_spinner(format!("Checking {} config", &repo.name));
        let config_file = self
            .github
            .get_file_contents(&repo.full_name, CONFIG_PATH)
            .await;
        match config_file {
            Result::Ok(config_file) => {
                spinner.finish_with_message(format!(
                    "Found CircleCI config for {}, proceeding setup...",
                    &repo.name
                ));
                Ok(Some(config_file))
            }
            Err(_) => {
                spinner.finish_with_message(format!(
                    "No CircleCI config found for {}, skipping...",
                    &repo.name
                ));
                Ok(None)
            }
        }
    }

    async fn select_env_vars(&self, env_vars: &[String]) -> anyhow::Result<Vec<String>> {
        let all = Confirm::with_prompt(
            "Do you want to move all environment variables? (No = select which to move)",
        )
            .default(true)
            .interact()?;

        if all {
            Ok(env_vars.to_vec())
        } else {
            let selection = MultiSelect::with_prompt("Select environment variables to move")
                .items(env_vars)
                .interact()?;
            if selection.is_empty() {
                println!("⚠️No environment variables selected");
            }
            let env_vars: Vec<String> = selection.into_iter().cloned().collect::<Vec<_>>();
            Ok(env_vars)
        }
    }

    async fn create_contexts_actions(
        &self,
        config: &Config,
        gh_contexts: &[Context],
        bb_contexts: &[Context],
        defined_contexts: &HashSet<String>,
    ) -> anyhow::Result<Vec<Action>> {
        if config.contexts.is_empty() {
            return Ok(vec![]);
        }

        println!(
            "Found {} contexts in .circleci/config.yml file",
            config.contexts.len()
        );
        for context in &config.contexts {
            println!(" - {}", context);
        }

        let existing_names = gh_contexts
            .iter()
            .map(|context| context.name.clone())
            .collect::<HashSet<_>>();

        let diff = config
            .contexts
            .difference(&existing_names)
            .cloned()
            .collect::<HashSet<_>>();

        let diff = diff
            .difference(defined_contexts)
            .cloned()
            .collect::<Vec<_>>();

        if diff.is_empty() {
            println!("All contexts already exist, skipping...");
            return Ok(vec![]);
        }

        println!(
            "Found {} undefined contexts in GitHub organization: {}",
            diff.len(),
            diff.join(", ")
        );

        let context_selection = MultiSelect::with_prompt("Select contexts to create")
            .items(&diff)
            .interact()?;

        if context_selection.is_empty() {
            println!("No contexts selected, skipping...");
            return Ok(vec![]);
        }

        let contexts: Vec<String> = context_selection.into_iter().cloned().collect::<Vec<_>>();

        let input_variables_values = Confirm::with_prompt("Do you want to input variable values to the new contexts? (No = creating empty contexts)")
            .interact()?;

        if !input_variables_values {
            println!("Creating empty contexts...");
            return Ok(contexts
                .into_iter()
                .map(|context| Action::CreateContext {
                    name: context,
                    variables: vec![],
                })
                .collect());
        }

        let mut actions: Vec<Action> = vec![];

        for context in contexts {
            println!("Creating {} context...", context);
            if let Some(bb_context) = bb_contexts.iter().find(|c| c.name == context) {
                let spinner =
                    spinner::create_spinner(format!("Fetching {} context variables", &context));
                let variables = api::get_context_variables(&bb_context.id).await?;
                spinner.finish_with_message(format!(
                    "Found {} variables for '{}' context",
                    variables.len(),
                    &context
                ));

                let variables = variables
                    .into_iter()
                    .map(|variable| {
                        let name = variable.variable;
                        let value =
                            Input::with_prompt(format!("Input value for '{}' variable:", name))
                                .interact()
                                .expect("invalid input for variable value");
                        EnvVar { name, value }
                    })
                    .collect::<Vec<_>>();
                actions.push(Action::CreateContext {
                    name: context,
                    variables,
                });
            } else {
                println!(
                    "Context {} not found in Bitbucket, adding empty context",
                    context
                );
                actions.push(Action::CreateContext {
                    name: context,
                    variables: vec![],
                });
            }
        }
        Ok(actions)
    }

    async fn start_build(&self, repo: &Repository) -> anyhow::Result<Option<Action>> {
        let confirm = Confirm::with_prompt(format!(
            "Do you want to start a build for {} repository on CircleCI?",
            &repo.name
        ))
            .default(true)
            .interact()?;

        if !confirm {
            return Ok(None);
        }

        let use_default_branch = Confirm::with_prompt(format!(
            "Do you want to use default branch for build ({})?",
            &repo.default_branch
        ))
            .default(true)
            .interact()?;

        if use_default_branch {
            return Ok(Some(Action::StartPipeline {
                repository_name: repo.full_name.clone(),
                branch: repo.default_branch.clone(),
            }));
        }

        let spinner = spinner::create_spinner("Fetching branches from GitHub...");
        let branches = self.github.get_repo_branches(&repo.full_name).await?;
        spinner.finish_with_message(format!("Found {} branches", branches.len()));

        let branches = branches
            .into_iter()
            .map(|branch| branch.name)
            .collect::<Vec<_>>();

        let default_idx = branches
            .iter()
            .position(|branch| branch == &repo.default_branch)
            .unwrap_or(0);

        let branch = FuzzySelect::with_prompt("Select branch to build")
            .items(&branches)
            .default(default_idx)
            .interact()?;

        Ok(Some(Action::StartPipeline {
            repository_name: repo.full_name.clone(),
            branch: branch.clone(),
        }))
    }

    fn parse_config(&self, config: &FileContents) -> anyhow::Result<Config> {
        let config = base64::decode_config(config.content.replace('\n', ""), base64::STANDARD)?;
        let config = std::str::from_utf8(&config)?;

        let config = Config::from_str(config)?;

        Ok(config)
    }

    fn save_migration_file(&self, migration: &Migration) -> Result<(), anyhow::Error> {
        if self.output.exists() {
            let overwrite = Confirm::with_prompt("Migration file already exists. Overwrite?")
                .default(false)
                .interact()?;

            if !overwrite {
                return Err(anyhow!("Migration file already exists"));
            }
        }
        let mut file = File::create(&self.output)?;

        serde_json::to_writer(&mut file, migration)?;

        Ok(())
    }
}

