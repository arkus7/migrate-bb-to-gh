<h1 align="center">Welcome to migrate-bb-to-gh 👋</h1>
<p>
  <img alt="Version" src="https://img.shields.io/badge/version-0.7.0-blue.svg?cacheSeconds=2592000" />
  <a href="#" target="_blank">
    <img alt="License: MIT" src="https://img.shields.io/badge/License-MIT%20OR%20Apache-yellow.svg" />
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
<summary><strong>This is a security concern!</strong></summary>

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
  - username and app password of your Bitbucket account (used to fetch information about repositories)
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

```sh
cargo run -- --help
```

## Author

👤 **Arkadiusz Żmudzin**

* GitHub: [@arkus7](https://github.com/arkus7)
* LinkedIn: [@arkadiusz-zmudzin](https://linkedin.com/in/arkadiusz-zmudzin)

## Show your support

Give a ⭐️ if this project helped you in any way!

***
_This README uses template from ❤️ [readme-md-generator](https://github.com/kefranabg/readme-md-generator)_