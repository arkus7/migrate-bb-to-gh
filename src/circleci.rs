pub(crate) mod wizard {
    use std::{path::PathBuf, str::FromStr, collections::HashSet};

    use anyhow::{anyhow, Ok};
    use dialoguer::{theme::ColorfulTheme, Confirm, FuzzySelect, MultiSelect};

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
                dbg!(&config);
                if let None = config {
                    println!("No config found for {}, skipping...", repository.full_name);
                    continue;
                }

                let config = self.parse_config(&config.unwrap())?;

                if let Some(move_envs_action) = self.move_env_vars(&repository).await? {
                    actions.push(move_envs_action);
                }

                let create_contexts_actions = self.create_contexts_actions(&repository, &config).await?;
            }
            dbg!(&actions);

            Ok(())
        }

        async fn move_env_vars(&self, repository: &Repository) -> anyhow::Result<Option<Action>> {
            let spinner = spinner::create_spinner(format!(
                "Fetching {} environment variables",
                &repository.name
            ));
            let env_vars: Vec<_> = api::get_env_vars(&repository.full_name)
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

        async fn create_contexts_actions(&self, repository: &Repository, config: &Config) -> anyhow::Result<Vec<Action>> {
          if config.contexts.is_empty() {
            return Ok(vec![]);
          }

          println!("Found {} contexts", config.contexts.len());
          for context in &config.contexts {
            println!(" - {}", context);
          }

          let spinner = spinner::create_spinner("Fetching contexts from CircleCI...");
          let exisiting_contexts = api::get_contexts().await?;
          spinner.finish_with_message(format!("Found {} contexts", exisiting_contexts.len()));

          let existing_names = exisiting_contexts.iter().map(|context| context.name.clone()).collect::<HashSet<_>>();
          let diff = config.contexts.difference(&existing_names).cloned().collect::<Vec<_>>();

          if diff.is_empty() {
            println!("All contexts already exist, skipping...");
            return Ok(vec![]);
          }

          println!("Found {} new contexts: {}", diff.len(), diff.join(", "));

          // TODO: check which contexts are already on CircleCI
          // TODO: select contexts to create
          // TODO: select variables to fill, ask for each one value
          todo!("creating contexts not yet implemented");
        }

        fn parse_config(&self, config: &FileContents) -> anyhow::Result<super::config::Config> {
            let config = base64::decode_config(config.content.replace("\n", ""), base64::STANDARD)?;
            let config = std::str::from_utf8(&config)?;
            let config = super::config::Config::from_str(&config)?;

            Ok(config)
        }
    }
}

mod api {
    use reqwest::IntoUrl;
    use serde::{de::DeserializeOwned, Deserialize, Serialize};

    const TOKEN: &str = "6b6e68c774603758ab9c526dda94258ddfbdca8f";
    const AUTH_HEADER: &str = "Circle-Token";

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct EnvVar {
        pub name: String,
        pub value: String,
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct EnvVarsResponse {
        items: Vec<EnvVar>,
        next_page_token: Option<String>,
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct Context {
        pub name: String,
        pub id: String,
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]

    pub struct ContextsResponse {
      items: Vec<Context>,
      next_page_token: Option<String>,
    }

    pub async fn get_env_vars(full_repo_name: &str) -> anyhow::Result<Vec<EnvVar>> {
        let project_slug = format!("bitbucket/{}", full_repo_name);
        let url = format!(
            "https://circleci.com/api/v2/project/{project_slug}/envvar",
            project_slug = project_slug,
        );

        let res: EnvVarsResponse = send_get_request(url).await?;

        Ok(res.items)
    }

    pub async fn get_contexts() -> anyhow::Result<Vec<Context>> {
        const GITHUB_ORG_ID: &str = "d5d2a07e-1731-435c-8e9f-916b6d9dc197";
        let url = format!("https://circleci.com/api/v2/context?owner-id={org_id}", org_id = GITHUB_ORG_ID);

        let res: ContextsResponse = send_get_request(url).await?;

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
    use std::{collections::{HashSet, HashMap}, str::FromStr};

    use serde::{Deserialize, Serialize};

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
                .flatten()
                .for_each(|c| {
                    contexts.insert(c);
                });

            Ok(Config {
                contexts,
            })
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
            pub context: Option<Vec<String>>,
        }
    }
}
