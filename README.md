<p align="center">
  <a href="https://rightcommit.com">
    <img src=".github/assets/rightcommit-icon.png" alt="rightcommit icon" width="128" height="128">
  </a>
</p>

<h1 align="center">
  <a href="https://rightcommit.com">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="https://github.com/rightcommit/rightcommit/raw/HEAD/.github/assets/rightcommit-wordmark-dark.svg?sanitize=true">
      <img src=".github/assets/rightcommit-wordmark-light.svg" alt="rightcommit" width="360" height="72">
    </picture>
  </a>
</h1>

English | [简体中文](README_zh-CN.md)

<!-- Keep README_zh-CN.md in sync when updating this README. -->

<p align="center">
  <strong>The open source workflow engine for AI coding agents.</strong>
</p>

<p align="center">
  Describe the job, get an editable workflow on a visual canvas, and run Claude Code, Codex and Pi through the tests, reviews and approvals you define. Works with any method or framework. Self-hosted.
</p>

<p align="center">
  <a href="https://rightcommit.com"><strong>Website</strong></a> ·
  <a href="#quickstart"><strong>Quickstart</strong></a> ·
  <a href="#documentation"><strong>Docs</strong></a> ·
  <a href="#contributing"><strong>Contribute</strong></a> ·
  <a href="https://github.com/rightcommit/rightcommit/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22"><strong>Good first issues</strong></a> ·
  <a href="#license"><strong>License</strong></a>
</p>

<p align="center">
  <img src=".github/assets/rightcommit-walkthrough.gif" alt="rightcommit demo showing the Project Manager drafting a workflow, then execution, tests, review, a local commit, and FinOps" width="780">
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-6254e8" alt="License: Apache 2.0"></a>
  <img src="https://img.shields.io/badge/built%20with-Rust-b7410e" alt="Built with Rust">
  <a href="https://github.com/rightcommit/rightcommit/actions/workflows/docker-check.yml"><img src="https://github.com/rightcommit/rightcommit/actions/workflows/docker-check.yml/badge.svg?branch=main" alt="Docker image checks"></a>
  <a href="https://github.com/rightcommit/rightcommit/actions/workflows/package-check.yml"><img src="https://github.com/rightcommit/rightcommit/actions/workflows/package-check.yml/badge.svg?branch=main" alt="Installable package checks"></a>
  <a href="https://github.com/rightcommit/rightcommit/stargazers"><img src="https://img.shields.io/github/stars/rightcommit/rightcommit?style=social" alt="GitHub stars"></a>
</p>

---

## What you can build

Start from more than 20 ready-made workflows, or describe a new one to the Project Manager.

- **Any agentic setup.** Combine agents, reviews, gates and connections on a canvas. Run steps
  in parallel, call other workflows, schedule runs, or publish a workflow over A2A.
- **Your preferred \*-driven development.** Spec-driven, test-driven, behavior-driven or your own method:
  each phase is a step, and a gate decides when the next one can start. Spec Kit is built in;
  OpenSpec or any other framework installs with a setup command. See
  [Spec-driven development](SPEC-DRIVEN-DEVELOPMENT.md).
- **From tickets to tested code.** Turn GitHub, Jira or ServiceNow tickets into requirements,
  specs and tests, then deliver features, changes and incident fixes with regression tests.
- **Legacy code analysis.** Reconstruct behavior, rules and tests from the source, checked by a
  second agent, as a baseline for new work.
- **Security reviews.** Run scanners on the server, let an agent triage and fix the findings,
  and block the commit until the scan is clean or a person accepts the risk.
- **Release, review and IT service management.** Release notes and rollback plans, pull request
  review dossiers, Confluence briefings, and ServiceNow ticket classification with a local
  knowledge base.

## Quickstart

### macOS: Homebrew

```sh
brew install rightcommit/tap/rightcommit
mkdir -p ~/rightcommit && cd ~/rightcommit
rightcommit setup
rightcommit
```

On Apple Silicon Macs with macOS 15 or later, Homebrew downloads rightcommit ready to use in a
few seconds; on Intel Macs and older macOS versions it compiles it, which takes several minutes.
Run `rightcommit setup` once to install the agents. Launch `rightcommit` from the same folder
on future starts to reuse your data. See the [macOS install guide](INSTALL_MAC.md).
To start over with empty data, stop the app and run `rightcommit resetdb`; see
[Reset and restore](HOW_TO_RESET_RESTORE_RIGHTCOMMIT.md) for what it deletes and how to
restore its backup.

### Windows, Linux or any system: Docker

Docker runs rightcommit on macOS, Windows and Linux with everything included, and runs each
account as a separate system user. Start Docker, then run the command below to clone and start
rightcommit.

```sh
git clone https://github.com/rightcommit/rightcommit.git && cd rightcommit && docker compose up -d
```

The first start downloads the prebuilt image of the latest release (about 1 GB), so it takes a few
minutes, depending on your connection. For a ZIP download, step-by-step instructions or a build
from source (10–30 minutes the first time), see the [Docker install guide](INSTALL.md).
To choose between the install methods, see [Which install to choose](INSTALL.md#which-install-to-choose).

### Run your first workflow

1. Open <http://localhost:8080>. The first account becomes the administrator, and public sign-up
   closes by default.
2. **Check the install, without an API key.** Press **+**, choose **Local CI example** under
   **Get started** and press **Save**. Press **Run workflow** → **Run now** → **Start**. The
   server runs the tests of a small bundled Python project: select **Local CI tests**, and the
   Activity log shows the last lines of the output, with `Ran 1 test` and `OK`. Its report under
   **Artifacts** has all of it.
3. In **Personal settings → Agents**, configure an API key or authorized provider for at least
   one agent and pick a model. See [Configuring the agents](#configuring-the-agents).
4. **Run an agent on the bundled example.** Press **+** and choose **Feature → SDD → Delivery**.
   It works on Northstar Helpdesk, a small Rust project bundled with rightcommit: the agent
   writes the specification and the code for a feature, the server runs `cargo test`, and the
   run ends with a local commit, with no push. Start it as in step 2. When it stops for your
   approval, press **Review required**.

In the **+** list, the workflows tagged **Local** need no external account. When a card also
names an agent, such as **Local · Codex**, that agent must be configured too: a workflow whose
agents are not configured is refused at start, with a message naming them. Those tagged GitHub,
Jira, Bitbucket, Confluence or ServiceNow need that service connected first, so keep them for
later. You can also describe a workflow to the **Project Manager**.
[Your first run](INSTALL.md#your-first-run) has every step, what each run needs and where to find
the results.

**Use your own code.** Copy or clone your project into the `sources` folder: the one next to
`compose.yaml` with Docker or Podman, `~/rightcommit/sources` with Homebrew. In the workflow,
select the **Source** node, set **Source type** to **Folder in /sources** and pick your project.
See [Your own project folders](INSTALL.md#your-own-project-folders).

**[Star rightcommit](https://github.com/rightcommit/rightcommit)** if you find it useful.
[Report a bug or suggest an idea](https://github.com/rightcommit/rightcommit/issues),
or see [how to contribute](CONTRIBUTING.md).

<a name="why-rightcommit"></a>
<a name="what-you-get"></a>

## How you stay in control

### AI agents say done. rightcommit proves it.

- **Assign agents and tools.** Pick Claude Code, Codex or Pi for each agent step. Build the workflow on the canvas or edit a draft from Project Manager.
- **Set quality gates.** Require closed tasks, [run your project's tests on the server](guides/CONFIGURING_LOCAL_CI.md) and require fresh test results if files change. Add agent reviews, and approve the exact files and diff before the local commit is created.
- **Inspect every artifact.** Each step records the files it creates or changes, with versions, SHA-256 fingerprints and line-by-line diffs for supported text files. Filter, compare and download them from the run view. Route failed checks back for a limited number of retries.
- **Restart from any step.** Continue a stopped run from the node you choose. Earlier results, files and agent sessions are kept, so only that node and the ones after it run again, and you don't pay twice for work already done. You can also edit the workflow and continue in the same workspace.
- **Choose when to intervene.** Run in interactive, assisted or unattended mode. Pause for human decisions and resume from the saved state.
- **Track usage and costs.** FinOps shows recorded tokens and available costs by step, run, workflow, agent and model. It shows reported costs and estimates separately.

## Connections

- **Agents:** Claude Code, Codex and Pi, with an API key or an authorized gateway (Anthropic,
  OpenAI, OpenRouter, AWS Bedrock, Google Vertex, Microsoft Foundry and more).
- **Services:** GitHub, Azure DevOps, ServiceNow, and Jira, Confluence and Bitbucket through
  the Atlassian CLI. By default, built-in external writes wait for confirmation of the exact content.
- **Open standards:** MCP to give agents tools from your MCP servers, and A2A to publish a
  workflow that other agents can call.

## Who it's for

rightcommit supports individual use and internal organizational deployments with separate
user accounts. Read [Provider terms and permitted use](PROVIDER_TERMS.md) and
[Intended use and third-party provider obligations](INTENDED_USE.md). Provider names identify integrations
and do not imply affiliation or endorsement.

- **Developers** running agents on their own laptop, with their own keys.
- **Small teams** who prefer to self-host, with separate users, keys and workspaces.

## Other ways to install

| Guide | For |
|---|---|
| [INSTALL_MAC.md](INSTALL_MAC.md) | macOS without containers: Homebrew, crates.io or source |
| [INSTALL.md](INSTALL.md) | Docker on macOS, Windows and Linux, step by step |
| [INSTALL_LINUX.md](INSTALL_LINUX.md) | Linux and WSL2 without containers: crates.io or source |
| [INSTALL_PODMAN.md](INSTALL_PODMAN.md) | Podman instead of Docker |

### Managing the Docker install

| What | Command |
|---|---|
| Stop the app | `docker compose down` |
| Update after getting a new version | `docker compose pull`, then `docker compose up -d` |
| Build the image from source | `docker compose -f compose.yaml -f compose.build.yaml up -d --build` |
| Use another port | set `RIGHTCOMMIT_PORT=9090` in a `.env` file, then `docker compose up -d` |

> [!WARNING]
> Never run `docker compose down -v`: it deletes the `rightcommit-data` volume, with all
> accounts, workflows and keys.

By default, Docker Compose binds the app to localhost.

## Configuring the agents

Start with [Provider terms and permitted use](PROVIDER_TERMS.md). All three agents support multiple users;
provider authorization and credential scope still apply.

The settings take API keys, not Claude Pro/Max or ChatGPT subscriptions, and the app has no
Claude or ChatGPT sign-in. The administrator can run Claude Code on their own Claude subscription
with a token in `.env`; every other user needs an API key.

| Agent | What you need | Guide |
|---|---|---|
| Claude Code | An Anthropic API key from the Claude Console, or a gateway, Amazon Bedrock, Google Vertex AI or Microsoft Foundry set up by the administrator. Only the administrator can use their own Claude subscription, through `CLAUDE_CODE_OAUTH_TOKEN` in `.env`; the API key field refuses subscription tokens. | [HOW_TO_CONFIGURE_CLAUDECODE.md](HOW_TO_CONFIGURE_CLAUDECODE.md) |
| Codex | An OpenAI platform API key, or a compatible provider set up by the administrator. No ChatGPT sign-in. | [HOW_TO_CONFIGURE_CODEX.md](HOW_TO_CONFIGURE_CODEX.md) |
| Pi | An API key or cloud credentials for the selected provider (Anthropic, OpenAI, Gemini and more). With Anthropic, Claude subscription tokens are refused. | [HOW_TO_CONFIGURE_PI.md](HOW_TO_CONFIGURE_PI.md) |
| Pi with OpenRouter | An OpenRouter API key with credit. | [HOW_TO_CONFIGURE_PI_OPENROUTER.md](HOW_TO_CONFIGURE_PI_OPENROUTER.md) |
| Pi with a local model | A model server on your computer (Ollama, LM Studio, vLLM). No API key. | [HOW_TO_CONFIGURE_PI_LOCAL.md](HOW_TO_CONFIGURE_PI_LOCAL.md) |

## Documentation

| Document | What it covers |
|---|---|
| [SPEC-DRIVEN-DEVELOPMENT.md](SPEC-DRIVEN-DEVELOPMENT.md) | Run spec-driven development with Spec Kit, OpenSpec or any other framework: a worked example and the SDD workflow templates |
| [CHANGELOG.md](CHANGELOG.md) | What changed in each version |
| [HOW_TO_RESET_RESTORE_RIGHTCOMMIT.md](HOW_TO_RESET_RESTORE_RIGHTCOMMIT.md) | Start over with empty data, restore the backup a reset leaves, delete old backups |
| [ADD_A_SKILL.md](ADD_A_SKILL.md) | Give a skill to Claude Code, Codex and Pi in one step |
| [ADD-PI-PACKAGES.md](ADD-PI-PACKAGES.md) | Add packages from pi.dev to the Pi agent |
| [ADD-A-FRAMEWORK.md](ADD-A-FRAMEWORK.md) | Use Spec Kit, OpenSpec or another framework with your own project |
| [ADD-OPENSPEC.md](ADD-OPENSPEC.md) | Use OpenSpec in your workflows |
| [UPDATE-PI-DEFAULT-PACKAGES.md](UPDATE-PI-DEFAULT-PACKAGES.md) | Update the Pi tools rightcommit ships (maintainers) |
| [CONFIGURATION GUIDES](guides/README.md) | Configure access and run workflow examples for ServiceNow (ITSM, catalog Requests and Spec Kit), Atlassian (Jira, Bitbucket and Confluence), GitHub (issues and CI) and Local CI tests |

## License

Copyright (c) 2026 Lorenzo Toscano

rightcommit is open source, released under the **[Apache License, Version 2.0](LICENSE)**.

You can use, modify, self-host and distribute the code, including for commercial purposes,
under the terms of the license. See [LICENSE](LICENSE) for the full terms.

The permissions above concern rightcommit itself. Any third-party product or service used
with it remains subject to its own terms and the [supported deployment scope](INTENDED_USE.md).

Third-party notices for bundled icons and Spec Kit files are in [`licenses/`](licenses).

## Contributing

Pull requests are welcome. Contributions are accepted under [Apache-2.0](LICENSE).
See [CONTRIBUTING.md](CONTRIBUTING.md).

New to rightcommit? [Browse open good first issues](https://github.com/rightcommit/rightcommit/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22).

Found a bug or have an idea? [Open an issue](https://github.com/rightcommit/rightcommit/issues).

Questions and feedback: [hello@rightcommit.com](mailto:hello@rightcommit.com).

## Contributors

<a href="https://github.com/Shards-inc"><img src="https://avatars.githubusercontent.com/u/226203219?v=4&amp;s=96" alt="Shards-inc" width="48" height="48"></a>
<a href="https://github.com/dannyward630"><img src="https://avatars.githubusercontent.com/u/71604164?v=4&amp;s=96" alt="dannyward630" width="48" height="48"></a>
<a href="https://github.com/Golden007-prog"><img src="https://avatars.githubusercontent.com/u/140976895?v=4&amp;s=96" alt="Golden007-prog" width="48" height="48"></a>

---

rightcommit is an independent side project created and maintained by Lorenzo Toscano in his personal capacity. It is developed in his own time, with his own resources, and is not affiliated with, endorsed by, or sponsored by his employer, any of its clients, or any other organization. Views and choices expressed in this project are his own.

<sub>Claude Code, Codex, Pi, GitHub, Azure DevOps, ServiceNow, Jira, Confluence, Bitbucket,
Spec Kit, Rust, Docker and other names and marks belong to their owners. rightcommit is not
affiliated with them.</sub>
