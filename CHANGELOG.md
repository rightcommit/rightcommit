# Changelog

What changed in each version of rightcommit. Releases are also listed on
[GitHub](https://github.com/rightcommit/rightcommit/releases).

## [1.5.1] - 2026-11-13

### Added

- A guide to the local A2A incident demo, `examples/a2a/README.md`: two workflows that talk
  through A2A on the bundled Northstar incident, with no API key or external service.

### Changed

- Pi 1.5.2. Its catalog adds the Qwen Token Plan Individual Cn provider (43 providers).

### Fixed

- Some labels were missing from the English interface: the workflow export button, the run
  artifacts list, the line-by-line comparison, the loading message and the screen-reader name
  of the Activity Log.
- A project file that the run did not change could appear as modified among the run's files,
  mostly on macOS, when the system reported a change to it late. Files written just before the
  run starts are now compared by content.

## [1.5.0] - 2026-11-13

### Added

- **Add Spec Kit if missing** on the Source node: when a run starts and the project has no
  `.specify` folder, rightcommit adds its Spec Kit (1.2.2, for Claude Code, Codex and Pi) to the
  workspace without changing existing files. The Spec Kit workflow templates turn it on, so they
  run on your own project; templates already in your catalog keep their settings.
- **Setup command** on the Source node: a command run in the Source folder before the first
  node, for frameworks such as OpenSpec (`openspec init --tools claude,codex,pi`). It runs
  without a shell and without agent credentials, like the test command, and the import dialog
  lists it.
- New guide: [ADD-A-FRAMEWORK.md](ADD-A-FRAMEWORK.md) on using Spec Kit, OpenSpec or another
  framework with your own project.

### Changed

- The **Skill** field of a Task takes any skill installed in the workspace, such as
  `openspec-propose`, not only Spec Kit commands. Spec Kit short names such as `plan` still work.
- A run now checks at start that every skill named in a Task exists for its Agent's runtime,
  instead of failing when it reaches that Task.
- An Agent chosen in the Agent field of a Task, Review or Gate no longer needs an edge. Dragging
  an Agent onto a Task, Review or Gate assigns it.
- Error messages from the server follow the interface language: they are shown in English when the
  interface is in English, also in the Activity Log.
- The Project Manager knows about skills, Source preparation and the new Agent edges.

### Fixed

- An error when starting a run is shown in the Start dialog, next to the button, not only in a
  short notice at the bottom of the page.

### Security

- Updated the HTML sanitizer used for Markdown (ammonia 4.2.5), which fixes a mutation XSS with
  embedded SVG and MathML
  ([RUSTSEC-2026-0533](https://rustsec.org/advisories/RUSTSEC-2026-0533)). rightcommit's
  settings did not allow the affected tags.

## [1.4.0] - 2026-11-13

### Added

- **Test command** on the Local CI step (a local Connection with the **test** operation): set the
  command that runs your project's tests, or leave it empty to detect it from the workspace root:
  `cargo test` (Cargo.toml), `npm test` (package.json), `python3 -m pytest` (pyproject.toml,
  pytest.ini or setup.py), `go test ./...` (go.mod), `mvn -q test` (pom.xml), `gradle test`
  (build.gradle) or `make test` (Makefile with a test target). The step and the Quality gate now
  work on projects in any of these languages. The command runs without a shell, and the Activity
  Log shows it with its origin. Workflows saved before this version detect the command.
- When you import a workflow, the dialog lists the test commands it would run on the server.

### Changed

- The Feature and Incident SDD templates no longer assume a Rust project: their prompts ask for
  the project's test suite, and the CI step is now called **Local CI · tests**. Templates you
  have already saved or edited keep their text.
- The test command gets only the basic environment: no agent API keys or provider credentials.
- Contributions are accepted under the Apache License 2.0. A Contributor License Agreement is
  no longer required.
- The README has a new introduction, a shorter quickstart and a demo in the light theme. It is
  also available in [Simplified Chinese](README_zh-CN.md) and credits external contributors.
- [CONTRIBUTING.md](CONTRIBUTING.md) explains how to build rightcommit and run the tests
  locally, and the [Docker install guide](INSTALL.md#tools-for-your-projects-tests) explains
  which test tools the image includes and how to add others.

### Fixed

- `rightcommit setup` and the Docker image allow Claude Code's install script, so Claude Code
  keeps working with npm 12. npm 12 blocks install scripts that are not allowed by name, and
  without that script the `claude` command only reported that its native binary was missing.

## [1.3.0] - 2026-11-09

### Added

- **Set Prompt** in the node menu: right-click a Task, Review, Gate, A2A or Workflow node
  to edit its prompt in a large editor. The workflow is saved when you confirm.
- **Dismiss suggestion**: a Manager suggestion can now be dismissed without applying it.
- Memory handoffs guide the source Task: when a Memory node follows a Task, the agent is
  asked to report decisions and their reasons, constraints, the files to read, the next
  step and open questions, and to say what it verified and what still needs checking.
  For reports longer than 16,000 characters, the agent is asked to save the full text
  under `.rightcommit/handoffs/` in the workspace and pass a summary with the file path,
  so the next agent can read it.
- When a source report is cut at 16,000 characters, the Memory node shows **Report
  truncated** and the next agent is told that the report is incomplete.

### Changed

- When a run fails, the footer shows a short **Run error · See Activity Log** message.
- The Manager view in FinOps is more compact: costs and the chart sit side by side.
- The Manager and the in-app guide explain Memory handoffs in more detail: when to place
  Task → Memory → Task, what is passed to the next agent, where long reports are saved and
  what the **Report truncated** warning means. When explaining a run, the Manager checks
  that a report file exists before saying it was saved.

### Fixed

- An applied or dismissed Manager suggestion no longer reappears after reloading the page
  or switching workflows.

### Security

- Updated the MCP client library (rmcp 3.9.0). A malicious or compromised MCP server can no
  longer exhaust memory by sending an endless HTTP response
  ([RUSTSEC-2026-0462](https://rustsec.org/advisories/RUSTSEC-2026-0462)).
- Updated the bundled SQLite (rusqlite 0.41). It fixes a read of uninitialized memory
  when a database is opened together with a crafted `-wal` file
  ([RUSTSEC-2026-0459](https://rustsec.org/advisories/RUSTSEC-2026-0459)).

## [1.2.4] - 2026-11-06

### Added

- **Skip task**: a switch on Task nodes bypasses the task without calling its agent. The
  node is shown dashed with a **Skipped** label, the run continues to the next node, and
  the task's settings are kept so it can be turned back on. It also works inside Fork
  branches. The Manager can skip and restore tasks as well.
- **Set as start**: choose the node a workflow starts from in the node's menu. A marker
  shows it on the graph.
- A badge on **Run history** shows how many workflows are running.
- Storage shows the size of files and folders.
- The Activity Log can be resized by dragging its top edge.

### Changed

- **Edit and continue** keeps working in the same project workspace, without copying files
  or recreating the Git repository. Each run keeps its own history, and earlier file
  versions stay available for diffs. Only the latest run of a project can be continued.
- New runs no longer read and copy every Source file when they start. They record file
  information and follow changes as they happen, with a check after every step so changes
  missed in Docker or shared folders are still caught. The original contents of Source
  files are no longer archived automatically. Runs from earlier versions open as before.

### Fixed

- Workflows with large Source folders stay responsive: unchanged files are no longer
  re-read after each step.
- The restart preview no longer freezes the interface on large folders.

## [1.2.3] - 2026-11-05

### Changed

- rightcommit is now licensed under the [Apache License 2.0](LICENSE), starting with this
  version. A new [NOTICE](NOTICE) file lists the bundled third-party material and its
  licenses, and is included in the crates.io package.
- The [contributor agreement used in this release](https://github.com/rightcommit/rightcommit/blob/b3c9d0ee5dac34c67fa740706679cc22f832587b/CLA.md)
  names Apache 2.0 as the project's license.
- The README is aimed at developers and small self-hosted teams, explains the new license
  and states that rightcommit is an independent personal project.

## [1.2.1] - 2026-11-05

### Added

- A **Filter** for the run's artifacts: choose which folders to show, in a tree you can
  expand and collapse. Hiding a folder also hides its subfolders, and the files are kept.
  `target/debug` folders are hidden by default.

### Changed

- New runs keep their internal files (reports and copied sources) in `.rightcommit` instead
  of `.nodo`. Runs saved with `.nodo` can still be opened, and a revision copies their
  sources into `.rightcommit`.
- The login form works with password managers like any other login, without a separate
  switch. The link under the registration form now reads **Back to login**.
- With Docker or Podman, `NODO_ALLOW_REGISTRATION`, `NODO_SECURE_COOKIE`,
  `NODO_CONNECTOR_HOSTS` and `RUST_LOG` can be set in `.env`. The defaults are unchanged:
  registration closed and cookies for local HTTP.
- `.env.example` groups the settings by purpose and documents the options for native
  installs. [INSTALL_MAC.md](INSTALL_MAC.md) explains where to put `.env` with Homebrew.
- The README is shorter and gives an overview of how to use rightcommit.

### Fixed

- A revision of a run is no longer refused when the Manager reorders the workflow or changes
  fields that do not affect its Sources.
- Error messages appear above open dialogs instead of behind them.

## [1.2.0] - 2026-11-04

### Added

- Users other than the administrator can use the administrator's saved API key. When a user
  has no personal key, Claude Code and Codex use the administrator key for the same agent,
  and Pi uses it when the provider and endpoint match. The key is never shown to the user,
  and Settings reports **Administrator API key** as the source. A personal key always takes
  precedence.
- The Manager has two permanent folders for each account, `manager/mcp-servers` and
  `manager/projects`. When writing is enabled in **Personal settings → Manager**, it builds
  requested MCP servers and projects there. They are kept when the workflow is deleted.
- Pi Agent nodes show the provider they use in the graph.
- [Provider terms and permitted use](PROVIDER_TERMS.md) and
  [Intended use](INTENDED_USE.md) explain the supported deployment scope and the provider
  terms that apply to each agent.

### Changed

- Provider configuration from the environment for Claude Code and Codex (gateways, cloud
  credentials and tokens in `.env`) applies only to the administrator. Other users need a
  personal API key or the administrator's saved key. Pi keeps reading provider variables
  from `.env` for every user.
- A saved API key for Claude Code or Codex always means direct API billing: gateways, cloud
  credentials and tokens in the environment no longer override it.
- API key fields for Claude Code, and for Pi with Anthropic, accept only Anthropic API keys,
  not Claude subscription tokens. Pi no longer uses `ANTHROPIC_AUTH_TOKEN`,
  `ANTHROPIC_OAUTH_TOKEN` or a Claude subscription login. If one is found, Pi stops with an
  error and leaves the file untouched.
- Removed the option to import the server's local Claude Code or Codex login into an account
  (`NODO_ALLOW_HOST_AUTH_IMPORT`). `.env.example` no longer lists `CODEX_ACCESS_TOKEN`,
  `ANTHROPIC_OAUTH_TOKEN` and `CLAUDE_CODE_OAUTH_TOKEN`.
- Agents and connections use labeled icons instead of provider logos. The footer notes that
  trademarks belong to their owners.
- The environment check shows native logins and login files as **External configuration**.
- The configuration guides for Claude Code, Codex and Pi list every supported Pi provider
  variable and explain credentials for multiple users.

### Fixed

- Error messages and compaction events shown to other users hide the administrator's key.
- Changing an Agent node's runtime or Pi provider updates the graph right away.
- The switch that hides the agent setup notice is centered with its label.

## [1.1.0] - 2026-11-04

### Added

- Scheduled runs. The **Run execution** menu offers **Run now** and **Scheduled**: a single
  start, or one every N minutes or hours, daily or weekly, with a time zone and an optional
  end date. Each schedule groups its runs. Pausing or deleting a schedule stops only future
  starts, and a start is skipped while the previous run is still open.
- A native ServiceNow connector. It reads incidents and catalog Requests, creates or reuses
  incidents, and updates and resolves tickets. It signs in with Basic, Bearer or OAuth Client
  Credentials, and the OAuth token is fetched automatically and reused until it expires.
  Writes still wait for approval.
- A **ServiceNow and ITSM** section in the template catalog:
  - ITSM ticket classification and resolution with Pi and an internal RAG index, in a generic
    version and a Northstar knowledge base demo. Ticket batches are deduplicated across runs.
  - Catalog Requests from configuration files.
  - A ServiceNow software requirement turned into a Spec Kit specification.
  - An OAuth connection test.
- GitHub API requests from a Connection node, with a searchable catalog of all 1,254 REST
  operations, plus GraphQL. POST, PUT, PATCH and DELETE requests wait for approval.
- New templates under **SDLC with Connections**:
  - GitHub: connection test, issue to requirements and tests, commit CI readiness, and demo
    CI setup.
  - Atlassian: Jira issue to requirements and tests, Jira + Bitbucket PR review dossier, and
    Confluence page to SDLC briefing.
- Connection nodes can set their own organization, project and repository instead of the
  personal defaults, and show the provider's icon.
- Atlassian sign-in from Personal settings with browser OAuth, scoped Bitbucket tokens and a
  **Test connection** button.
- A **Multi-provider** mode for Pi. Each Pi Agent node picks a provider from Pi's catalog
  (42 providers in Pi 1.4.0) and a model. Credentials come from `.env` or native Pi logins.
  Local models work through `PI_LOCAL_BASE_URL`.
- Outside Docker, rightcommit loads `.env` from the current folder at startup. Variables that
  are already set take precedence.
- How-to guides for ServiceNow, GitHub and Atlassian in [guides/](guides/README.md).

### Changed

- The interface starts in English on first launch. An Italian choice you saved is kept.
- The login card has a cleaner wordmark.
- The notice about agents that are not configured can be turned off, separately in each
  browser.
- The README has a demo image and describes orchestration, scheduled runs and Pi with
  multiple providers.
- The package check on GitHub skips changes that only touch documentation and caches Rust
  builds.

### Fixed

- Settings are protected from browser and password-manager autofill. Autofill that nobody
  asked for is ignored, and saving is blocked if a ServiceNow credential lands in an agent
  field.

## [1.0.2] - 2026-11-02

### Added

- Homebrew bottles for Apple Silicon Macs with macOS 15 or later: `brew install` and
  `brew upgrade` download rightcommit ready to use instead of compiling it.
- A new user starts on an empty workflow instead of the first example. The examples are still
  one click away under **+**.

### Changed

- The login card has a centered header, and the rightcommit wordmark no longer leaves a gap
  before the logo.
- The Docker image compiles its dependencies in a separate layer, so rebuilding after a code
  change takes about a minute.

## [1.0.1] - 2026-11-02

### Added

- Outside Docker, rightcommit creates and uses `./sources` next to `./data`, so Homebrew,
  crates.io and source installations no longer need `NODO_SOURCES` at every start.
- The agent versions live in one file, `distribution/agents.env`, used by both the Docker
  image and `rightcommit setup`.
- The automatic check on GitHub also builds and starts the Docker image.

### Changed

- Node.js 24 for the Docker image, the Homebrew formula and the guides (22.19 or newer still
  works).
- Codex 0.166.0 and Claude Code 2.1.316. Pi stays at 1.4.0.
- Homebrew needs Rust only to compile rightcommit, so it no longer takes the place of rustup.
- The macOS and Linux guides install with Homebrew and crates.io.

### Fixed

- pulldown-cmark 0.13.6, for [RUSTSEC-2026-0433](https://rustsec.org/advisories/RUSTSEC-2026-0433).

## [1.0.0] - 2026-11-02

First public release.

- Visual workflow canvas, with a Project Manager that drafts workflows from a description.
- Claude Code, Codex and Pi in one workspace, each step with its own agent and model.
- Verified outcomes: task lists, tests run on the server, review by a second agent and human
  approval before a commit.
- Versioned artifacts with SHA-256 fingerprints, line-by-line diffs and bounded rework.
- FinOps panel with costs and tokens per step, run, workflow, agent and model.
- Connections to GitHub, Azure DevOps, ServiceNow and Atlassian, plus MCP and A2A.
- Runs with Docker or Podman, or natively on macOS and Linux.
