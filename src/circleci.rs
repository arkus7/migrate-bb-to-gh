pub(crate) mod wizard {
    use std::{any, collections::HashSet, fs::File, hash::Hash, path::PathBuf, str::FromStr};

    use anyhow::{anyhow, Ok};
    use dialoguer::{
        theme::ColorfulTheme, Confirm, FuzzySelect, Input, MultiSelect, Password, Select,
    };

    use crate::{
        circleci::migrate::Action,
        github::{self, FileContents, Repository, Team},
        spinner,
    };

    use super::{api, config::Config};

    pub struct Wizard {
        output: PathBuf,
        theme: ColorfulTheme,
    }

    impl Wizard {
        pub fn new(output: &PathBuf) -> Self {
            Self {
                output: output.clone(),
                theme: ColorfulTheme::default(),
            }
        }

        pub async fn run(&self) -> anyhow::Result<()> {
            println!("Welcome to CircleCi Migration Wizard!");
            let team = self.select_team().await?;
            let repositories = self.select_repositories(&team).await?;

            let mut actions: Vec<Action> = vec![];
            for repository in repositories {
                println!("Configuring {} repository...", &repository.full_name);
                let config = self.check_config_exists(&repository).await?;
                if let None = config {
                    println!("No config found for {}, skipping...", repository.full_name);
                    continue;
                }

                let config = self.parse_config(&config.unwrap())?;

                if let Some(move_envs_action) = self.move_env_vars(&repository).await? {
                    actions.push(move_envs_action);
                }

                let create_contexts_actions =
                    self.create_contexts_actions(&config, &actions).await?;
                actions.extend(create_contexts_actions);

                if let Some(start_build_action) = self.start_build(&repository).await? {
                    actions.push(start_build_action);
                }
            }

            self.save_migration_file(&actions)?;

            Ok(())
        }

        async fn move_env_vars(&self, repository: &Repository) -> anyhow::Result<Option<Action>> {
            let spinner = spinner::create_spinner(format!(
                "Fetching {} environment variables",
                &repository.name
            ));
            let env_vars: Vec<_> = api::get_env_vars(api::Vcs::Bitbucket, &repository.full_name)
                .await?
                .into_iter()
                .map(|e| e.name)
                .collect();
            spinner.finish_with_message(format!("Found {} environment variables", env_vars.len()));

            if env_vars.is_empty() {
                println!(
                    "No environment variables found for {}, skipping...",
                    &repository.full_name
                );
                return Ok(None);
            }

            let move_envs = Confirm::with_theme(&self.theme)
                .with_prompt("Do you want to move the environment variables?")
                .interact()?;
            let action = if move_envs {
                let env_vars = self.select_env_vars(&env_vars).await?;
                let action = Action::MoveEnvironmentalVariables {
                    repository_name: repository.full_name.clone(),
                    env_vars,
                };
                Some(action)
            } else {
                None
            };

            Ok(action)
        }

        async fn select_team(&self) -> anyhow::Result<Team> {
            let spinner = spinner::create_spinner("Fetching teams...");
            let teams = github::get_teams().await?;
            spinner.finish_with_message(format!("Fetched {} teams", teams.len()));

            let team_selection = FuzzySelect::with_theme(&self.theme)
                .with_prompt("Select team")
                .items(&teams)
                .default(0)
                .interact()?;

            let team = teams.get(team_selection).expect("Invalid team selected");
            Ok(team.clone())
        }

        async fn select_repositories(
            &self,
            team: &github::Team,
        ) -> anyhow::Result<Vec<Repository>> {
            let spinner =
                spinner::create_spinner(format!("Fetching repositories from {} team", &team.name));
            let repositories = github::get_team_repositories(&team.slug).await?;
            spinner.finish_with_message("Fetched!");
            let selection = MultiSelect::with_theme(&self.theme)
                .with_prompt(format!("Select repositories from {} team", &team.name))
                .items(&repositories)
                .interact()?;
            if selection.is_empty() {
                return Err(anyhow!("At least one repository must be selected"));
            }
            let repositories: Vec<github::Repository> = selection
                .into_iter()
                .flat_map(|idx| repositories.get(idx))
                .cloned()
                .collect::<Vec<_>>();
            Ok(repositories)
        }

        async fn check_config_exists(
            &self,
            repo: &Repository,
        ) -> anyhow::Result<Option<FileContents>> {
            const CONFIG_PATH: &str = ".circleci/config.yml";

            let spinner = spinner::create_spinner(format!("Checking {} config", &repo.name));
            let config_file = github::get_file_contents(&repo.full_name, CONFIG_PATH).await;
            match config_file {
                Result::Ok(config_file) => {
                    spinner.finish_with_message("Found!");
                    Ok(Some(config_file))
                }
                Result::Err(_) => {
                    spinner.finish_with_message("Not found!");
                    Ok(None)
                }
            }
        }

        async fn select_env_vars(&self, env_vars: &[String]) -> anyhow::Result<Vec<String>> {
            let all = Confirm::with_theme(&self.theme)
                .with_prompt(
                    "Do you want to move all environment variables? (No = select which to move)",
                )
                .interact()?;

            if all {
                Ok(env_vars.to_vec())
            } else {
                let selection = MultiSelect::with_theme(&self.theme)
                    .with_prompt("Select environment variables to move")
                    .items(&env_vars)
                    .interact()?;
                if selection.is_empty() {
                    return Err(anyhow!(
                        "At least one environment variable must be selected"
                    ));
                }
                let env_vars: Vec<String> = selection
                    .into_iter()
                    .flat_map(|idx| env_vars.get(idx))
                    .cloned()
                    .collect::<Vec<_>>();
                Ok(env_vars)
            }
        }

        async fn create_contexts_actions(
            &self,
            config: &Config,
            actions: &[Action],
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

            let spinner = spinner::create_spinner("Fetching GitHub contexts from CircleCI...");
            let exisiting_contexts = api::get_contexts(api::Vcs::GitHub).await?;
            spinner.finish_with_message(format!(
                "Found {} contexts defined in GitHub org",
                exisiting_contexts.len()
            ));

            let existing_names = exisiting_contexts
                .iter()
                .map(|context| context.name.clone())
                .collect::<HashSet<_>>();

            let diff = config
                .contexts
                .difference(&existing_names)
                .cloned()
                .collect::<HashSet<_>>();

            let migrated_contexts = actions
                .iter()
                .filter(|action| match action {
                    Action::CreateContext { .. } => true,
                    _ => false,
                })
                .map(|action| match action {
                    Action::CreateContext { name, .. } => name.clone(),
                    _ => unreachable!(),
                })
                .collect::<HashSet<_>>();

            let diff = diff
                .difference(&migrated_contexts)
                .cloned()
                .collect::<Vec<_>>();

            if diff.is_empty() {
                println!("All contexts already exist, skipping...");
                return Ok(vec![]);
            }

            println!("Found {} new contexts: {}", diff.len(), diff.join(", "));

            let context_selection = MultiSelect::with_theme(&self.theme)
                .with_prompt("Select contexts to create")
                .items(&diff)
                .interact()?;

            if context_selection.is_empty() {
                println!("No contexts selected, skipping...");
                return Ok(vec![]);
            }

            let contexts: Vec<String> = context_selection
                .into_iter()
                .flat_map(|idx| diff.get(idx))
                .cloned()
                .collect::<Vec<_>>();

            let input_variables_values = Confirm::with_theme(&self.theme)
                .with_prompt("Do you want to input variable values to the new contexts? (No = creating empty contexts)")
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

            let spinner = spinner::create_spinner("Fetching Bitbucket contexts from CircleCI...");
            let exisiting_contexts = api::get_contexts(api::Vcs::Bitbucket).await?;
            spinner.finish_with_message(format!(
                "Found {} contexts in Bitbucket",
                exisiting_contexts.len()
            ));

            let mut actions: Vec<Action> = vec![];

            for context in contexts {
                if let Some(bb_context) = exisiting_contexts.iter().find(|c| c.name == context) {
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
                            let variable = variable.variable;
                            let value = Input::with_theme(&self.theme)
                                .with_prompt(format!("Input value for '{}' variable:", variable))
                                .interact()
                                .expect("invalid input for variable value");
                            (variable, value)
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
            let confirm = Confirm::with_theme(&self.theme)
                .with_prompt(format!(
                    "Do you want to start a build for {} repository on CircleCI?",
                    &repo.name
                ))
                .interact()?;

            if !confirm {
                return Ok(None);
            }

            let use_default_branch = Confirm::with_theme(&self.theme)
                .with_prompt(format!(
                    "Do you want to use default branch for build ({})?",
                    &repo.default_branch
                ))
                .interact()?;

            if use_default_branch {
                return Ok(Some(Action::RunFirstBuild {
                    repository_name: repo.full_name.clone(),
                    branch: repo.default_branch.clone(),
                }));
            }

            let spinner = spinner::create_spinner("Fetching branches from GitHub...");
            let branches = github::get_repo_branches(&repo.full_name).await?;
            spinner.finish_with_message(format!("Found {} branches", branches.len()));

            let branches = branches
                .into_iter()
                .map(|branch| branch.name.clone())
                .collect::<Vec<_>>();

            let default_idx = branches
                .iter()
                .position(|branch| branch == &repo.default_branch)
                .unwrap_or(0);

            let branch_selection = Select::with_theme(&self.theme)
                .with_prompt("Select branch to build")
                .items(&branches)
                .default(default_idx)
                .interact()?;

            Ok(Some(Action::RunFirstBuild {
                repository_name: repo.full_name.clone(),
                branch: branches[branch_selection].clone(),
            }))
        }

        fn parse_config(&self, config: &FileContents) -> anyhow::Result<super::config::Config> {
            let config = base64::decode_config(config.content.replace("\n", ""), base64::STANDARD)?;
            let config = std::str::from_utf8(&config)?;

            let config = super::config::Config::from_str(&config)?;

            Ok(config)
        }

        fn save_migration_file(&self, actions: &[Action]) -> Result<(), anyhow::Error> {
            if self.output.exists() {
                let overwrite = Confirm::with_theme(&self.theme)
                    .with_prompt("Migration file already exists. Overwrite?")
                    .default(false)
                    .interact()?;

                if !overwrite {
                    return Err(anyhow!("Migration file already exists"));
                }
            }
            let mut file = File::create(&self.output)?;

            serde_json::to_writer(&mut file, actions)?;

            Ok(())
        }
    }
}

mod api {
    use reqwest::IntoUrl;
    use serde::{de::DeserializeOwned, Deserialize, Serialize};

    const TOKEN: &str = "6b6e68c774603758ab9c526dda94258ddfbdca8f";
    const AUTH_HEADER: &str = "Circle-Token";

    pub enum Vcs {
        Bitbucket,
        GitHub,
    }

    impl Vcs {
        const fn org_id(&self) -> &str {
            match self {
                Vcs::Bitbucket => "0cb7bbc7-b867-455b-a6cb-fa51b56d65af",
                Vcs::GitHub => "d5d2a07e-1731-435c-8e9f-916b6d9dc197",
            }
        }

        const fn slug_prefix(&self) -> &str {
            match self {
                Vcs::Bitbucket => "bitbucket",
                Vcs::GitHub => "gh",
            }
        }
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct EnvVar {
        pub name: String,
        pub value: String,
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    struct EnvVarsResponse {
        items: Vec<EnvVar>,
        next_page_token: Option<String>,
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct Context {
        pub name: String,
        pub id: String,
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    struct ContextsResponse {
        items: Vec<Context>,
        next_page_token: Option<String>,
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    struct ContextVariablesResponse {
        items: Vec<ContextVariable>,
        next_page_token: Option<String>,
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]

    pub struct ContextVariable {
        pub variable: String,
        pub context_id: String,
    }

    pub async fn get_env_vars(vcs: Vcs, full_repo_name: &str) -> anyhow::Result<Vec<EnvVar>> {
        let project_slug = format!("{}/{}", vcs.slug_prefix(), full_repo_name);
        let url = format!(
            "https://circleci.com/api/v2/project/{project_slug}/envvar",
            project_slug = project_slug,
        );

        let res: Result<EnvVarsResponse, reqwest::Error> = send_get_request(url).await;
        let items = match res {
            Ok(res) => res.items,
            Err(err) => {
                if let Some(code) = err.status() {
                    if code == 404 {
                        return Ok(vec![]);
                    }
                }
                return Err(anyhow::anyhow!("Failed to get env vars: {}", err));
            }
        };

        Ok(items)
    }

    pub async fn get_contexts(vcs: Vcs) -> anyhow::Result<Vec<Context>> {
        let url = format!(
            "https://circleci.com/api/v2/context?owner-id={org_id}",
            org_id = vcs.org_id()
        );

        let res: ContextsResponse = send_get_request(url).await?;

        Ok(res.items)
    }

    pub async fn get_context_variables(context_id: &str) -> anyhow::Result<Vec<ContextVariable>> {
        let url = format!(
            "https://circleci.com/api/v2/context/{context_id}/environment-variable",
            context_id = context_id
        );

        let res: ContextVariablesResponse = send_get_request(url).await?;

        Ok(res.items)
    }

    async fn send_get_request<T: DeserializeOwned, U: IntoUrl>(
        url: U,
    ) -> Result<T, reqwest::Error> {
        let client = reqwest::Client::new();
        let res = client
            .get(url)
            .header(AUTH_HEADER, TOKEN)
            .send()
            .await?
            .error_for_status()?
            .json::<T>()
            .await?;

        Ok(res)
    }
}

mod migrate {
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Debug)]
    #[serde(rename_all = "snake_case")]
    pub enum Action {
        MoveEnvironmentalVariables {
            repository_name: String,
            env_vars: Vec<String>,
        },
        CreateContext {
            name: String,
            variables: Vec<(String, String)>,
        },
        RunFirstBuild {
            repository_name: String,
            branch: String,
        },
    }
}

mod config {
    use std::{
        collections::{HashMap, HashSet},
        str::FromStr,
    };

    use serde::{Deserialize, Serialize};

    use self::raw::Context;

    #[derive(Serialize, Deserialize, Debug)]
    pub struct Config {
        pub contexts: HashSet<String>,
    }

    impl FromStr for Config {
        type Err = anyhow::Error;

        fn from_str(s: &str) -> anyhow::Result<Self> {
            let raw = serde_yaml::from_str::<raw::RawConfig>(s)?;

            let mut contexts = HashSet::<String>::new();

            raw.workflows
                .into_values()
                .map(|w| w.jobs)
                .flatten()
                .map(|j| j.into_values())
                .flatten()
                .flat_map(|j| j.context)
                .for_each(|c| match c {
                    Context::String(ctx) => {
                        contexts.insert(ctx);
                    }
                    Context::Vec(ctx) => {
                        ctx.into_iter().for_each(|c| {
                            contexts.insert(c);
                        });
                    }
                });

            Ok(Config { contexts })
        }
    }

    mod raw {
        use std::collections::BTreeMap;

        use serde::{Deserialize, Serialize};

        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        pub(crate) struct RawConfig {
            pub workflows: BTreeMap<String, Workflow>,
        }

        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        pub(crate) struct Workflow {
            pub jobs: Vec<BTreeMap<String, Job>>,
        }

        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        pub(crate) struct Job {
            pub context: Option<Context>,
        }

        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        #[serde(untagged)]
        pub(crate) enum Context {
            String(String),
            Vec(Vec<String>),
        }
    }
}
