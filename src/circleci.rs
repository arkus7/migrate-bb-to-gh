pub(crate) mod wizard {
    use std::{
        collections::HashSet,
        fs::File,
        path::{Path, PathBuf},
        str::FromStr,
    };

    use anyhow::{anyhow, Ok};
    use dialoguer::{theme::ColorfulTheme, Confirm, FuzzySelect, Input, MultiSelect};

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
        theme: ColorfulTheme,
        version: String,
    }

    pub struct WizardResult {
        pub actions: Vec<Action>,
        pub migration_file_path: PathBuf,
    }

    impl Wizard {
        pub fn new(output: &Path, version: &str) -> Self {
            Self {
                output: output.to_path_buf(),
                theme: ColorfulTheme::default(),
                version: version.to_owned(),
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
                let bb_repo = bitbucket::get_repository(&repository.full_name).await?;
                spinner.finish_with_message(format!("Found {:?} repository in Bitbucket", bb_repo));
                if bb_repo.is_none() {
                    let manually_map = Confirm::with_theme(&self.theme)
                        .with_prompt(format!("No repository named {} found in Bitbucket, do you want to manually map it?", &repository.name))
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
                    let repositories =
                        bitbucket::get_project_repositories(project.get_key()).await?;
                    spinner.finish_with_message(format!(
                        "Fetched {} repositories from {} project!",
                        repositories.len(),
                        project
                    ));
                    let selection = FuzzySelect::with_theme(&self.theme)
                        .with_prompt(format!("Select repository from {} project", project))
                        .items(&repositories)
                        .interact()?;

                    let bb_repo = repositories.get(selection);
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
            let move_envs = Confirm::with_theme(&self.theme)
                .with_prompt("Do you want to move the environment variables from Bitbucket to GitHub organization?")
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
                    spinner.finish_with_message(format!(
                        "Found CircleCI config for {}, proceeding setup...",
                        &repo.name
                    ));
                    Ok(Some(config_file))
                }
                Result::Err(_) => {
                    spinner.finish_with_message(format!(
                        "No CircleCI config found for {}, skipping...",
                        &repo.name
                    ));
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
                    .items(env_vars)
                    .interact()?;
                if selection.is_empty() {
                    println!("⚠️No environment variables selected");
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
                            let value = Input::with_theme(&self.theme)
                                .with_prompt(format!("Input value for '{}' variable:", name))
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
                return Ok(Some(Action::StartPipeline {
                    repository_name: repo.full_name.clone(),
                    branch: repo.default_branch.clone(),
                }));
            }

            let spinner = spinner::create_spinner("Fetching branches from GitHub...");
            let branches = github::get_repo_branches(&repo.full_name).await?;
            spinner.finish_with_message(format!("Found {} branches", branches.len()));

            let branches = branches
                .into_iter()
                .map(|branch| branch.name)
                .collect::<Vec<_>>();

            let default_idx = branches
                .iter()
                .position(|branch| branch == &repo.default_branch)
                .unwrap_or(0);

            let branch_selection = FuzzySelect::with_theme(&self.theme)
                .with_prompt("Select branch to build")
                .items(&branches)
                .default(default_idx)
                .interact()?;

            Ok(Some(Action::StartPipeline {
                repository_name: repo.full_name.clone(),
                branch: branches[branch_selection].clone(),
            }))
        }

        fn parse_config(&self, config: &FileContents) -> anyhow::Result<super::config::Config> {
            let config = base64::decode_config(config.content.replace('\n', ""), base64::STANDARD)?;
            let config = std::str::from_utf8(&config)?;

            let config = super::config::Config::from_str(config)?;

            Ok(config)
        }

        fn save_migration_file(&self, migration: &Migration) -> Result<(), anyhow::Error> {
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

            serde_json::to_writer(&mut file, migration)?;

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

    #[derive(Serialize, Deserialize, Debug, Clone)]
    struct ExportEnvironmentBody {
        /// List of URLs to the projects where env variables should be exported to
        projects: Vec<String>,
        #[serde(rename = "env-vars")]
        env_vars: Vec<String>,
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    struct StartPipelineBody {
        branch: String,
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    struct CreateContextBody {
        name: String,
        owner: ContextOwnerBody,
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    struct ContextOwnerBody {
        id: String,
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    struct UpdateContextVariableBody {
        value: String,
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

    pub async fn export_environment(
        from_repo_name: &str,
        to_repo_name: &str,
        env_vars: &[String],
    ) -> Result<(), anyhow::Error> {
        let url = format!(
            "https://circleci.com/api/v1.1/project/bitbucket/{repo_name}/info/export-environment",
            repo_name = from_repo_name
        );
        let body = ExportEnvironmentBody {
            projects: vec![format!("https://github.com/{}", to_repo_name)],
            env_vars: env_vars.to_vec(),
        };

        let _: serde_json::Value = send_post_request(url, Some(body)).await?;
        Ok(())
    }

    pub async fn start_pipeline(repo_name: &str, branch: &str) -> Result<(), anyhow::Error> {
        let url = format!(
            "https://circleci.com/api/v1.1/project/gh/{repo_name}/follow",
            repo_name = repo_name
        );
        let body = StartPipelineBody {
            branch: branch.to_string(),
        };

        let _: serde_json::Value = send_post_request(url, Some(body)).await?;
        Ok(())
    }

    pub async fn create_context(name: &str, vcs: Vcs) -> Result<Context, anyhow::Error> {
        let url = "https://circleci.com/api/v2/context";
        let body = CreateContextBody {
            name: name.to_string(),
            owner: ContextOwnerBody {
                id: vcs.org_id().to_string(),
            },
        };

        let ctx = send_post_request(url, Some(body)).await?;
        Ok(ctx)
    }

    pub async fn add_context_variable(
        context_id: &str,
        name: &str,
        value: &str,
    ) -> Result<ContextVariable, anyhow::Error> {
        let url = format!(
            "https://circleci.com/api/v2/context/{context_id}/environment-variable/{env_var_name}",
            context_id = context_id,
            env_var_name = name
        );
        let body = UpdateContextVariableBody {
            value: value.to_string(),
        };

        let var = send_put_request(url, Some(body)).await?;
        Ok(var)
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

    async fn send_post_request<T: DeserializeOwned, U: IntoUrl, B: Serialize>(
        url: U,
        body: Option<B>,
    ) -> Result<T, reqwest::Error> {
        let client = reqwest::Client::new();
        let res = client
            .post(url)
            .header(AUTH_HEADER, TOKEN)
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .json::<T>()
            .await?;

        Ok(res)
    }

    async fn send_put_request<T: DeserializeOwned, U: IntoUrl, B: Serialize>(
        url: U,
        body: Option<B>,
    ) -> Result<T, reqwest::Error> {
        let client = reqwest::Client::new();
        let res = client
            .put(url)
            .header(AUTH_HEADER, TOKEN)
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .json::<T>()
            .await?;

        Ok(res)
    }
}

pub(crate) mod migrate {
    use anyhow::{anyhow, Context};
    use std::{fs::File, path::Path};

    use dialoguer::Confirm;
    use serde::{Deserialize, Serialize};

    use crate::spinner;

    use super::api;

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
    #[serde(rename_all = "snake_case")]
    pub enum Action {
        MoveEnvironmentalVariables {
            from_repository_name: String,
            to_repository_name: String,
            env_vars: Vec<String>,
        },
        CreateContext {
            name: String,
            variables: Vec<EnvVar>,
        },
        StartPipeline {
            repository_name: String,
            branch: String,
        },
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct EnvVar {
        pub name: String,
        pub value: String,
    }

    impl Action {
        pub fn describe(&self) -> String {
            match self {
                Action::MoveEnvironmentalVariables {
                    from_repository_name,
                    to_repository_name,
                    env_vars,
                } => format!(
                    "Move environmental variables from '{}' project in Bitbucket to '{}' project Github\n  Envs: {}",
                    from_repository_name,
                    to_repository_name,
                    env_vars.join(", ")
                ),
                Action::CreateContext { name, variables } => format!(
                    "Create context named '{}' with {} variables:\n{}",
                    name,
                    variables.len(),
                    variables
                        .iter()
                        .map(|e| format!("  {}={}", e.name, e.value))
                        .collect::<Vec<_>>()
                        .join(",\n"),
                ),
                Action::StartPipeline { repository_name, branch } => format!(
                    "Start pipeline for {} on branch {}",
                    repository_name,
                    branch,
                ),
            }
        }

        pub async fn run(&self) -> anyhow::Result<()> {
            match self {
                Action::CreateContext { name, variables } => {
                    let spinner = spinner::create_spinner(format!("Creating '{}' context", name));
                    let ctx = api::create_context(name, api::Vcs::GitHub).await?;
                    spinner.finish_with_message(format!(
                        "Created context '{}' (id: {})",
                        &ctx.name, &ctx.id
                    ));

                    for var in variables {
                        let spinner = spinner::create_spinner(format!(
                            "Adding '{}' variable to '{}' context",
                            &var.name, &name
                        ));
                        let _ = api::add_context_variable(&ctx.id, &var.name, &var.value).await?;
                        spinner.finish_with_message(format!("Added '{}' variable", &var.name));
                    }

                    Ok(())
                }
                Action::MoveEnvironmentalVariables {
                    from_repository_name,
                    to_repository_name,
                    env_vars,
                } => {
                    let spinner = spinner::create_spinner(format!("Moving {} environmental variables from '{}' project on Bitbucket to '{}' project on Github", env_vars.len(), &from_repository_name, &to_repository_name));
                    let _ =
                        api::export_environment(from_repository_name, to_repository_name, env_vars)
                            .await?;
                    spinner.finish_with_message(format!("Moved {} environmental variables from '{}' project on Bitbucket to '{}' project on Github", env_vars.len(), &from_repository_name, &to_repository_name));
                    Ok(())
                }
                Action::StartPipeline {
                    repository_name,
                    branch,
                } => {
                    let spinner = spinner::create_spinner(format!(
                        "Starting pipeline for {} on branch {}",
                        &repository_name, &branch
                    ));
                    let _ = api::start_pipeline(repository_name, branch).await?;
                    spinner.finish_with_message(format!(
                        "Started pipeline for {} on branch {}",
                        &repository_name, &branch
                    ));
                    Ok(())
                }
            }
        }
    }

    pub async fn migrate(migration_file: &Path, version: &str) -> anyhow::Result<()> {
        let file = File::open(migration_file)?;
        let migration: Migration = serde_json::from_reader(file).with_context(|| format!("Error when parsing {:?} file.\nIs this a JSON file?\nDoes the version match the program version ({})?\nConsider re-generating the migration file with `wizard` subcommand.", migration_file, version))?;
        if migration.version != version {
            return Err(anyhow!("Migration file version is not compatible with current version, expected: {}, found: {}", version, migration.version));
        }
        let actions = migration.actions;

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
}

mod config {
    use std::{collections::HashSet, str::FromStr};

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
                .filter(|w| matches!(w, raw::WorkflowEntry::Workflow(_)))
                .flat_map(|w| match w {
                    raw::WorkflowEntry::Workflow(w) => w.jobs,
                    _ => unreachable!(),
                })
                .flat_map(|j| j.into_values())
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
            pub workflows: BTreeMap<String, WorkflowEntry>,
        }

        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        #[serde(untagged)]
        pub(crate) enum WorkflowEntry {
            Workflow(Workflow),
            Other(serde_yaml::Value),
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
