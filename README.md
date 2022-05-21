<h1 align="center">Welcome to migrate-bb-to-gh 👋</h1>
<p>
  <img alt="Version" src="https://img.shields.io/badge/version-0.7.0-blue.svg?cacheSeconds=2592000" />
  <a href="#" target="_blank">
    <img alt="License: MIT" src="https://img.shields.io/badge/License-MIT%20OR%20Apache%202.0-yellow.svg" />
  </a>
</p>

> A CLI tool for migration of repositories from Bitbucket to GitHub for organizations
> 
> `migrate-bb-to-gh` guides you through migration process by interactive wizard, 
> where you can select what repositories should be moved to the GitHub account

### 🏠 [Homepage](https://github.com/arkus7/migrate-bb-to-gh)

## Configuration
This project uses `config.yml` file stored inside the root of the project directory for including configuration (such as API keys, tokens etc.) inside the binary itself.

<details>
<summary><strong>This is a security concern!</strong> (click to expand)</summary>

Usually, you wouldn't put any secrets inside the binary file, as it's rather easy to extract them from the binary.

This setup enables to build the binary file with common configuration, without a need to have the config file next to a binary.

**If you want to share the build binary, share it only with people you trust.**
</details>

Example configuration file can be found in [sample.config.yml](./sample.config.yml) file.

Config file contains:
- Git configuration:
  - SSH key used to pull repositories from Bitbucket organization
  - SSH key used to push repositories to GitHub organization
- Bitbucket configuration:
  - username and app password of your Bitbucket account (used to fetch information about projects and repositories)
- GitHub configuration:
  - username and personal access token of your GitHub account (used to manage repositories and teams in GitHub organization)

Optionally, when you want to use the `circleci` feature, which gives you additional commands 
to move your CircleCI configuration between Bitbucket and GitHub organizations, you'll need:
- CircleCI personal token of the admin account (it should be the same account for Bitbucket and GitHub)
- Bitbucket's organization ID on CircleCI
- GitHub's organization ID on CircleCI

## Building

In order to build a binary executable, you need to have Rust installed.

Follow [the official guide](https://www.rust-lang.org/tools/install) on how to install Rust on your machine.

When you have Rust installed (with Cargo), simply run the following command to build the binary

```sh
cargo build --release
```

This will produce a single binary file called `migrate-bb-to-gh` in `target/release` directory.

### Features

This project has one feature, named `circleci` which gives you additional command in the CLI 
to migrate CircleCI configuration between Bitbucket and GitHub project in CircleCI.

If you'd like to use this feature, you need to build the binary with `--features circleci` option:

```sh
cargo build --features circleci --release
```

## Usage

You can list available commands by using `--help` option

```sh
./migrate-bb-to-gh --help
```

### Wizard

First, you need to go through a `wizard`, which will ask you to select repositories you want to migrate from Bitbucket,
letting you select who in your GitHub organization should have access to the selected repositories 
(using [Teams](https://docs.github.com/en/organizations/organizing-members-into-teams/about-teams)), and change the default branch.

```shell
./migrate-bb-to-gh wizard
```

The wizard results with a migration file named (by default) `migration.json`, 
which contains all the details about what needs to be done during the migration.
You can inspect the file to see what will be done when the migration starts.

At the end of the wizard, the CLI will list all the actions in human-readable form, so you can review it there as well.

You can change the default name of the created file by providing an `--output` (or `-o`) option, passing a path to a file where it should be stored.

```shell
./migrate-bb-to-gh wizard --output my-migration-file.json
```

If the migration file already exists, the `wizard` command will ask if you want to override it or not.
Not overriding file in this case results with cancellation of the wizard.

### Migrate

When your `migration.json` file is ready, you can start the migration by using `migrate` command, 
passing the path to the migration file as a positional argument:

```shell
./migrate-bb-to-gh migrate migration.json
```

Similar to the end of the `wizard` command, CLI will print a list of actions that will be taken during the migration.
You need to confirm whether the CLI should start the migration.

The `migrate` command (apart from first confirmation) is not interactive.

### CircleCI commands (with `circleci` feature)

The project has a optional `circleci` feature (check [Features](#features) section to see how to enable it),
which gives you additional `circleci` command.

The `circleci` command, similarly to the main app, has 2 subcommands:
- `wizard` which guides you through migration of CircleCI configuration from Bitbucket to GitHub
- `migrate` which executes the migration

You can run them as shown below
```shell
./migrate-bb-to-gh circleci wizard
# wizard creates (by default) `ci-migration.json` file (can be changed with --output option)
./migrate-bb-to-gh circleci migrate ci-migration.json
```

## Author

👤 **Arkadiusz Żmudzin**

* GitHub: [@arkus7](https://github.com/arkus7)
* LinkedIn: [@arkadiusz-zmudzin](https://linkedin.com/in/arkadiusz-zmudzin)

## Show your support

Give a ⭐️ if this project helped you in any way!

## 📝 License

Copyright © 2022 [Arkadiusz Żmudzin](https://github.com/arkus7).

This project is [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) licensed.

***
_This README uses template from ❤️ [readme-md-generator](https://github.com/kefranabg/readme-md-generator)_