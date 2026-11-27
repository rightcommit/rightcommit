# Changelog

What changed in each version of rightcommit. Releases are also listed on
[GitHub](https://github.com/rightcommit/rightcommit/releases).

## [1.10.5] - 2026-11-27

### Added

- **Missing Source folders are named and can be created at start.** When a Source of type
  Folder in /sources points to a folder that does not exist, starting the run from the
  interface opens a dialog that names the Source and the folder: **Create** makes the folder,
  empty, inside /sources and starts the run; **No** starts nothing. Scheduled runs,
  sub-workflows, A2A and API calls never create folders: they stop with an error that names
  the Source and the folder. API clients can send `create_missing_source_folders: true` after
  a 409 `source_folders_missing` response.
- The Docker and Podman image includes `pip` and `venv` for its Python 3, so a Python project
  can install its dependencies, pytest included, in a virtual environment inside its
  workspace. [Tools for your project's tests](INSTALL.md#tools-for-your-projects-tests) has the
  commands and the test command to set for the Local CI step.

### Changed

- Dependency, cache and generated build folders are skipped at any depth, as `node_modules`
  already was in part: `.venv`, `venv`, `__pycache__`, `.pytest_cache`, `.tox`,
  `node_modules`, `.next`, `.gradle`, `.dart_tool`, `.terraform`, `_build` and more (the full
  list is in INSTALL.md). They are not copied from a Source into the workspace and do not
  count toward the copy size limit, they are not captured as artifacts or shown in the
  Artifacts view, in the workspace in Storage or in the result ZIP, and RAG does not index
  them. A file with one of these names is still copied. A project that commits one of these
  folders does not find it in a copied workspace; with **Permanently modify source** it stays
  in place.

### Fixed

- A Source folder that holds a Python virtual environment can start a run. The copy refused
  the symbolic links inside `.venv`, and **Permanently modify source** refused them too, also
  inside `node_modules`.
- A Source whose folder does not exist, or is a file, no longer stops the start with
  "No such file or directory": the error names the Source and the path.
- **Edit and continue** no longer fails when the workspace holds a virtual environment, and
  the result ZIP no longer fails on a `venv` or `.tox` folder.

### Security

- The server changes the ownership of files and copies them without following a symbolic
  link, also when a folder is replaced by a link while it works, and the file watcher no
  longer follows links.

## [1.10.4] - 2026-11-26

### Changed

- The results rightcommit writes into a run itself are now in English, so a workflow written in
  English no longer passes Italian text to its next agents: an approved Review and its server
  receipt, the deterministic Gate checks (convergence not reached, server tests missing, Gate
  changed the workspace after the tests), the local commit report, the requests to confirm a
  local commit or an external write, the clarification request of an A2A service, the message
  an exposed workflow sends to A2A clients while it waits for an approval, the supervisor's
  name in decisions, the RAG search notice and the placeholder prompts the Project Manager
  starts new nodes from. The interface still shows these results in its own
  language, Italian or English, with hashes, counts and file names as written, in the step
  reports, the Edit and continue preview, pending requests and the decision log. Runs from
  earlier versions keep their Italian text and are shown the same way.
- Errors from the MCP and RAG tools rightcommit gives agents are now in English: the MCP
  endpoint, the bundled workspace MCP server and the RAG search. The interface shows them in
  its own language, including the RAG test search, which showed them in Italian with the
  interface in English (#34).
- `libsais-rs` updated to 0.2.4, replacing a version withdrawn from crates.io.

### Fixed

- A supervised run no longer starts with a supervisor model chosen for another Pi provider.
  The execution mode records the provider the **Supervisor model** was chosen for; when the
  personal provider has changed since, a scheduled or manual run stops before any agent call
  with an error naming the schedule and the field, and the execution mode and schedule dialogs
  show a warning. Schedules and workflows saved by earlier versions are checked once the model
  is chosen again (#36).
- The two ITSM ticket classification templates have an Italian title with the interface in
  Italian. Existing installations keep their saved catalog until it is reset or edited in
  Manage templates (#35).
- README_zh-CN has the **Agents and the server environment** section and its two MCP
  subsections (#39).
- The Codex guide says what was tested for the ChatGPT sign-in inside the container: with
  Docker the whole procedure, from the device code to a workflow run, FinOps and an image
  update; Podman was not tested. It also says where to put the container name when it is not
  `rightcommit` (#37).

### Tests

- **Continue run** is covered with a Fork as the workflow's first step and with skipped Tasks
  in the branches; both cases already worked (#38).
- A check fails when a template catalog entry lacks its Italian or English title or
  description.

## [1.10.3] - 2026-11-26

### Added

- A stdio MCP server of the first administrator can take variables from the rightcommit
  server's environment by name: **Variables from the server environment** in
  Personal settings → MCP. The values are read each time the server starts, in runs and with
  Test connection, and never saved or shown; a missing or empty variable stops the start with
  an error that names it. Names rightcommit sets itself are refused, and other accounts can
  neither save nor use such names. See
  [Agents and the server environment](README.md#agents-and-the-server-environment).
- The **Project Manager** can propose to register an MCP server it has written and verified.
  The chat shows exactly what will be registered (command, arguments, variables) and saves it
  through the normal settings checks only when you confirm; a server with the same ID is
  replaced only after you confirm the replacement. A proposal with a literal value that looks
  like a secret is refused, and the manager is asked to name the variable instead.

- The **Model** field of an Agent node that runs Pi with OpenRouter offers the OpenRouter
  model list, searchable as in Personal settings → Agents → Pi Agent. Empty still uses the
  model from Personal settings, a model typed by hand or missing from the list is kept, and
  if the list does not load the field works as before.
- The **Supervisor model** field of the execution mode (workflow execution mode, run start and
  scheduled runs) offers the same OpenRouter model list when the supervisor runs Pi with
  OpenRouter: **Supervisor agent** set to *Pi Agent*, or *Personal preference* with Pi as the
  Default agent, and OpenRouter as the Pi provider in Personal settings (the default provider
  in Multi-provider mode). The list appears or disappears as you change the supervisor agent.
  Empty still uses the model from Personal settings, and any model can be typed by hand.

### Changed

- Agent steps and the **Project Manager** are told plainly that the **summary** of their
  answer must be the result itself in short form (the answer, the decision and its reason, the
  key points of a plan), never a description such as "Plan provided"; the long form goes in
  the details. The summary is what the run view shows first; the next steps still receive both.
- The Codex guide explains how to use your own Codex sign-in with a native install: run
  `codex login` with `CODEX_HOME` set to your account's Codex folder in the app's data folder.
  A sign-in made in your own terminal, or a `CODEX_HOME` exported before starting rightcommit,
  is not used. The README and the Project Manager say the same.
- The Codex guide also covers Docker and Podman: sign in inside the container with
  `docker exec` as your account's user, with Codex's device code sign-in
  (`codex login --device-auth`), which prints a link and a one-time code to enter in any
  browser. The sign-in stays in the data volume when you update the container. The README and
  the Project Manager say the same.
- The avatar menu entry and its dialog are written **Manage templates** in English, like the
  other menu entries.
- **FinOps** shows Codex runs made with a ChatGPT sign-in with the estimated cost at API
  prices, like any other Codex run: it is an estimate, not a charge on the ChatGPT plan.
- The first administrator's **Project Manager** also sees the environment variables of the
  rightcommit server, so the MCP servers and scripts it writes can be tested with them. The
  variables rightcommit sets itself and the credentials and endpoints of the manager's own
  agent keep their current rules, values of variables named like secrets are redacted in its
  replies, and workflow runs, other accounts and stdio MCP servers keep a clean environment.
  The manager can run commands: start rightcommit only with variables you accept it to read.
  See [Agents and the server environment](README.md#agents-and-the-server-environment).

### Fixed

- When an agent's answer ends without rightcommit's structured result, for example after a
  request such as "report the output verbatim, nothing else", rightcommit now asks the same
  session once more for the structured result only, with no new work, and keeps the text the
  agent already wrote: the **Project Manager** shows it as its reply, and a workflow step keeps
  it in its details. This works for the Project Manager and for every agent step (Task,
  Review, Gate, parallel branches and the supervisor) without using an attempt or a correction.
  The Activity log shows the extra request, the Project Manager chat shows it on the line that
  follows what the manager is doing until the reply arrives, and FinOps counts both turns. If
  the second answer has no structured result either, the error stays as before. The Project
  Manager is also told that "nothing else" applies to the content of its reply, never to the
  structured result.
- The errors shown when the OpenRouter model list cannot be loaded (unreachable, unavailable,
  invalid or empty catalog) are in English when the interface is in English.
- When a run runs its first step again, after **Continue run** from the Source or the first
  step, **Resume** of a first step that failed or was stopped, or **Edit and continue** from the
  first step, the link entering that step shows its moving dots again, as in the run's first
  start. Before, the dots appeared only from the second step on.
- **Continue run** works with parallel branches. From a **Join** it keeps the output of every
  branch and runs the Join and the steps after it again, so the next step receives all branch
  outputs as in a normal run; before, the run stopped at the Join because the branch outputs
  were discarded. From a step inside a branch it runs that step and the rest of its branch
  again, then joins them with the kept outputs of the other branches; before, the first step
  of a branch could not be chosen and a later one stopped the run at the Join. From a **Fork**
  every branch runs again, as before. If a branch did not complete, the Join cannot be chosen
  and the message says to continue from the Fork or from a step of that branch.
- **Continue run** gives the Gates and Reviews that run again their full number of
  corrections (**Rework limit**). Before, a Gate or Review that had used its corrections in the
  first pass stopped the run at its first negative outcome. Gates and Reviews before the chosen
  node keep their count, and attempts, supervisor budgets, sessions and artifacts are kept as
  before.
- **FinOps** lost the model and the cost of Codex runs whose working folder path ended with
  a slash, such as a direct source on the whole sources folder: their Codex logs were not
  read, so these runs showed only a summary with no model and no cost estimate.
- Agents now answer in the language of the workflow's own prompts and of the run request. The
  instructions rightcommit adds around a step (Task, Review, Gate, parallel branches, Memory
  handoff, supervisor) were in Italian, so a Gate could answer in Italian in an English
  workflow; they are now in English and ask the agent to follow the language of the step's
  prompt. The Project Manager answers in the language of your message.

## [1.10.2] - 2026-11-25

### Fixed

- The crates.io package builds again: it did not include the new favicon, so version 1.10.1
  could not be published on crates.io or Homebrew and is available only on GitHub and as a
  Docker image. This version has the same changes as 1.10.1 on every channel.

## [1.10.1] - 2026-11-25

### Changed

- Saving the first API key of an agent makes it the **Default** agent while the Default (Codex
  on a new account) has no credentials, and the confirmation says so. A Default you choose in
  the same save is kept, and Agent nodes with their own agent are not affected.
- When a run is refused because the Default agent has no credentials while another agent is
  configured, the message also suggests choosing that agent as Default.
- The toolbar button that opens **Personal settings** is named like the dialog, in the tooltip
  and for screen readers, instead of "Agents and connections". The guides use the same name.
- After a page reload, the run that was open in the tab opens again instead of Design. A run
  that no longer exists opens Design as before.
- The command line is in English: `rightcommit setup`, `rightcommit resetdb`, `--help`,
  argument and `.env` errors, and the server's log lines, such as `rightcommit ready` at
  startup.
- `rightcommit setup` prints one line for each step and hides npm's warnings, notices and
  summaries when the installation succeeds; when it fails, npm's own output is shown. The two
  install scripts npm reports as not allowed are not needed: the `@google/genai` one only
  prints a message and the `protobufjs` one only checks the version scheme of its dependents.

### Fixed

- A blank workflow is named "New workflow" in English instead of "Nuovo workflow", on the first
  sign-in and in **Create blank workflow**. Until it is saved, the default name follows a
  language switch.
- The Artifacts header counts the step reports listed under it, and with only reports the
  panel says that the run has not created or modified files so far. A step report no longer
  shows a **Full path**, which pointed to a file that does not exist: reports are kept by
  rightcommit and are listed in **Storage** → **Runs**.
- In **Start workflow**, a refused start is shown once, next to **Start**, instead of also in a
  notification that covered it.
- The Activity log labels the output of steps that run on the server without an agent (local
  tests and commit, Source preparation, ServiceNow, GitHub and Atlassian) **Step result**
  instead of **Agent response**.
- The Activity log no longer shows the same **Running** status twice when a step starts.
- A node added from the palette is titled in the interface language: "New Task", "New Review"
  and so on in English, "Nuovo Task", "Nuova Review" in Italian. A comment is titled "Comment"
  in English, and its empty text box invites you to write a note in English instead of
  Italian. Titles already saved stay as written.
- The Activity log names every kind of run event in the interface language, such as **Run
  created**, **Stop** and **Decision**, instead of showing internal names like `created`.
- The report of a step that runs on the server without an agent, such as local tests or a
  GitHub, ServiceNow or Atlassian request, is shown as written, in a monospaced font, in the
  artifact viewer, in **Storage** and in **Edit and continue**. A line of dashes in test output
  no longer turns into a horizontal rule, and `#`, `*` or `<...>` are no longer read as
  Markdown. The report itself, which later steps receive, is unchanged.
- The ServiceNow and GitHub guides and the ServiceNow and ITSM examples write **Personal
  settings** like the dialog.
- Error messages, help texts and the built-in ServiceNow, ITSM and GitHub templates name the
  dialog and its tabs as they appear: **Configurazioni personali → Agenti / Connessioni** in
  Italian and **Personal settings → Agents / Connections** in English, instead of "Personal
  Settings" or "impostazioni personali". Templates already saved in an installation keep their
  old text.
- `/favicon.ico` serves the rightcommit icon instead of an error on every page.
- The install guides say that the Docker image, about 1 GB to download, needs about 3.5 GB of
  disk space once unpacked.

## [1.10.0] - 2026-11-25

### Added

- Each release publishes a ready-made Docker image, `ghcr.io/rightcommit/rightcommit`, for
  64-bit Intel, AMD and ARM computers, tagged with the version and `latest`.
- `compose.build.yaml` builds the image from source instead:
  `docker compose -f compose.yaml -f compose.build.yaml up -d --build`.
- `RIGHTCOMMIT_VERSION` in `.env` picks the image version, or `latest`.
- `rightcommit resetdb` returns an installation to the state of a first install: accounts,
  settings and saved credentials, workflows, runs, schedules and each user's files are
  deleted, the workflow template catalog returns to the built-in one, and the next visitor
  creates the administrator account. The agents installed by `rightcommit setup` and the
  folders in `sources` are kept. It lists what it will delete, asks you to type `resetdb` to
  confirm (`--yes` for scripts), refuses while the app is running on the same data, and saves
  a copy of the database in `data/system/backups` unless you pass `--no-backup`. With Docker:
  `docker compose stop`, `docker compose run --rm rightcommit resetdb`, `docker compose up -d`.
- [HOW_TO_RESET_RESTORE_RIGHTCOMMIT.md](https://github.com/rightcommit/rightcommit/blob/main/HOW_TO_RESET_RESTORE_RIGHTCOMMIT.md):
  what `rightcommit resetdb` deletes and keeps, where its backup is and what it contains, how
  to restore it on a native install and with Docker or Podman, and how to delete old backups.
- The **Local CI example** is in the New workflow list, under **Get started**: a first run with
  no agent and no API key, only Python 3, without importing a file.
- **Confirm before commit**: a run stops at the local commit step and shows what the commit
  will contain: each file with its line-by-line diff (new, modified and deleted files; binary
  files named only), the edits that were already in the folder and stay out, and the command
  and output of the run's local tests. The commit is created only after you approve, and only
  if the files are still the ones shown; if they changed, the run asks again with the new
  content. Rejecting stops the run without a commit. Building the preview does not change the
  repository.

### Changed

- The Docker and Podman quick start downloads the image of your version instead of building it:
  `docker compose up -d` takes a few minutes, depending on your connection, where the first
  build took 10–30 minutes.
- To update, get the new version, then run `docker compose pull` and `docker compose up -d`.
  Your data stays in the `rightcommit-data` volume. An image you built before, `rightcommit:local`,
  is no longer used and can be removed with `docker image rm rightcommit:local`. If your
  `Dockerfile.tools` starts from it, see
  [Tools for your project's tests](https://github.com/rightcommit/rightcommit/blob/main/INSTALL.md#tools-for-your-projects-tests).
- The install guides give the same times: a few minutes for the download, 10–30 minutes for a
  first build from source.
- The README and the install guides no longer call Docker the fastest way to start. On a Mac
  they point to Homebrew first, ready in a few seconds on Apple Silicon with macOS 15 or later;
  Docker and Podman are the option for Windows, Linux or any system, and for a separate system
  user for each account. INSTALL.md has a new "Which install to choose" section.
- The configuration guides for Claude Code, Codex and Pi say up front what each agent needs: an
  API key or provider configuration, or a local model for Pi. The Claude Code guide has a new
  section on the administrator's own Claude subscription: a token created with
  `claude setup-token` and set as `CLAUDE_CODE_OAUTH_TOKEN` in `.env`, used only for the
  administrator's runs. It also explains why signing in to Claude Code on the computer is not
  enough. Other users need an API key, and the app has no ChatGPT sign-in for Codex. The README
  lists this for every agent.
- Agent settings note that the Claude Code field takes a Claude Console API key, not a
  subscription token, and that the administrator can use their own subscription through
  `CLAUDE_CODE_OAUTH_TOKEN` in `.env`; the Codex field takes an OpenAI platform API key.
  Errors about Claude subscription tokens are now also shown in English.
- With a native install (Homebrew, crates.io or source), the footer reads "Native install ·
  accounts share your system user" instead of "Development mode · File isolation disabled".
- The README and the install guides now say what to run first: the Local CI example, which
  needs no API key, then **Feature → SDD → Delivery** with one agent on the bundled Northstar
  Helpdesk project, with where to find each result. They say which templates need GitHub,
  Jira, Bitbucket, Confluence or ServiceNow, and how to use your own code from the `sources`
  folder. The Local CI example says where to find the passing output: its last lines in the
  Activity log, all of it in the step's report.

- Starting a workflow whose agents have no credentials is refused before the run starts, with
  the Agents to configure in Personal settings → Agents. Workflows that call no agent, skipped
  Tasks and human-only Reviews need none.
- A passing local test step shows the last lines of its output in the Activity log; the step's
  report keeps the full output.
- New commit nodes and the commit steps in the bundled workflows have **Confirm before commit**
  on. Workflows saved earlier commit without asking, as before: turn the option on in the node
  panel. On an existing installation the saved template catalog is not updated, so templates
  already in your catalog keep committing without asking until you turn it on. Unattended runs
  commit without waiting and say so in the step report.
- A commit that cannot be split safely (a file changed both before and during the run,
  submodules, nested repositories, conflicts) stops before the approval is asked.

### Fixed

- The local commit step commits only the changes made by the run. Edits that were already in
  the project folder when the run started, whether staged, unstaged or untracked, stay out of
  the commit and keep their state. If the run changed one of those files again, changed a
  submodule or created a nested Git repository, the step stops before committing and lists the
  files to review. When the Source is a subfolder of a larger repository, only that folder is
  committed. Runs on a project folder used directly that were started before this update stop
  at the commit step and ask you to commit manually.
- Workflow template cards list the agents the workflow's Agent nodes require, so
  **Incident → Living spec → Delivery** shows Codex.
- Activity log events written by the server, such as the run's creation, stops, resumes and
  server restarts, are shown in English in the English interface.
- The local commit step's report is shown in the interface language, in the run's artifacts,
  in Storage and in Edit and continue: the commit hash, the files committed, the edits left
  out, that nothing was pushed, and the note that an Unattended run committed without asking.
  The report now puts each of these on its own line. Reports saved by version 1.9.0 are shown
  in English too.
- The **Feature → SDD → Delivery** card says that another project also needs new Specify,
  Clarify and Checklist prompts, not only a new Source.
- Starting a workflow that has errors to fix, such as no start node, a node left unconfigured or
  no Source, reports those errors first, also through A2A. The message about agents without
  credentials now appears only for a workflow that could otherwise start.
- Edges are redrawn when a comment changes size, whether you resize it or it grows to its
  minimum height, so return routes no longer cross the comment.
- The icon guide describes every icon the interface shows, including Import workflow, Stop,
  Sign out, Workflow start, Skipped task, Restore, Open in a new tab, Expand folder, No agents
  configured, and Move, Clear selection and Reorder topic in the workflow list.
- Copying a file from the numbered file viewer, in run artifacts and in Storage, gives back
  the file exactly: no newline is added to a file that does not end with one, a file that ends
  with one keeps it, and line numbers stay out.

## [1.9.0] - 2026-11-23

### Added

- A rework edge can open a correction branch: steps that run only after a Review or Gate
  rejects, and that lead back to the Review or Gate, or to a later step. The branch can be
  given to a different Agent through a Memory node, which carries the finding to the Task
  after it. A rework edge can now end on a Memory node.
- A double click on a node that has a prompt opens **Set Prompt**. A single click still opens
  the side panel.
- **Cmd+S** or **Ctrl+S** saves the workflow, as the **Save workflow** button does.
- **Edit and continue** shows that you are working on a run: nodes keep the marks of the run
  you continue from, the canvas has a thin red edge, and the workspace label is red.

### Changed

- Workflow validation errors name the node by its title, say what is missing and what to do,
  in English and Italian. For example, a Memory with no source Task used to report
  "Node does not exist:" with nothing after it; it now reports that the Memory is not
  configured and asks for the source Task. The same applies to a Task, Gate or Review with no
  Agent, a Fork with no Join, a workflow with no entry node, and to RAG, MCP, A2A, Workflow,
  Fork, Join and link errors.
- Node titles in these messages are shown as you wrote them, also when the interface is in
  English.
- The Project Manager receives the same validation result the editor shows, so it can see why
  a draft does not run before changing it, and it knows the rules for correction branches.

### Fixed

- **Undo** in the Project Manager restores the draft from before its last change also after
  you edited the draft. It used to answer that automatic undo was unavailable.

## [1.8.0] - 2026-11-23

### Added

- The artifact viewer and the Storage viewer preview more file types:
  - Images (PNG, JPEG, GIF) are shown as images.
  - HTML files are rendered as a page. The page runs in a sandbox with no network access, so
    scripts and styles inside the file work, and resources loaded from a CDN do not.
    **Open in a new tab** shows the page in its own browser tab.
  - PDF files open in the browser's PDF viewer.
  - Text files show line numbers.
  - JSON is indented, and JSON, YAML and XML blocks can be collapsed and expanded.
  - Text files saved as Windows-1252 or UTF-16, common for COBOL and Visual Basic sources,
    are shown as text, with a note that they are not UTF-8. This changes only what the viewer
    shows.
- **Test connection** for Claude Code and Codex in Personal settings → Agents, as Pi already
  had. It sends one short request with the model and key in the form, without saving them and
  without running tools, and reports authentication, quota and model errors.

### Changed

- The artifact viewer keeps the version, the Preview/Source/Compare tabs and the full path in
  place while the content scrolls. HTML and PDF previews fill the height of the dialog in both
  viewers.
- Storage downloads are named after what they hold: the file or folder name, or the workflow
  name for a whole run. They used to be `sources.zip`.
- Hidden files are shown by default in the Artifacts panel and in Storage, and each
  **Show hidden** switch is remembered in the browser.

### Fixed

- The Artifacts panel scrolls when the window is short, and shrinks to its title when there is
  no room for the list.

## [1.7.1] - 2026-11-22

### Fixed

- After a workflow is renamed, its runs show the new name. The run list, the history, the
  activity log, FinOps, the Storage labels and the child runs of a parent run used to keep the
  name the workflow had when the run started. The copy of the workflow saved with each run is
  not changed, and the runs of a deleted workflow keep the name saved with them (#22).

## [1.7.0] - 2026-11-22

### Added

- Pi keeps one personal API key for each provider. Switching the Pi provider in Personal
  settings shows whether a key is saved for it and uses that key again, with no retyping. The
  bin icon removes only the key of the selected provider. A key saved by an earlier version
  belongs to the provider that was selected when you upgrade. Multi-provider mode is
  unchanged: it ignores personal keys, and now keeps them for when you select a single
  provider again (#18).
- FinOps shows costs by model (#19):
  - **Other** in the model breakdown and in the agent comparison opens the entries it groups.
    Each grouped agent shows its split by runtime and model. A long list shows its first ten
    agents and keeps the rest behind one row with their count and total.
  - The detail of a run has a **Costs by model** table, with cache write and reasoning tokens.
  - A usage record without a model appears as one unattributed row.
- FinOps tables show relative dates such as "3 days ago", with the exact date on hover:
  **Created** for workflows, **First used** for models and agents.
- A notice when a newer rightcommit version is published: a red bell on the account avatar
  and `vX.Y.Z available` in the account menu, linking to the release. You can dismiss it for
  that version. The server asks GitHub at most every six hours and sends nothing about your
  account or installation. Set `RIGHTCOMMIT_UPDATE_CHECK=0` in `.env` to turn the check off,
  for example on a network without Internet access (#21).

### Changed

- **Verify connection** for Pi uses the key saved for the provider you selected, also right
  after you leave Multi-provider mode.
- Changing the Pi provider clears a key typed but not yet saved, so it cannot be stored for
  the wrong provider.
- A non-administrator inherits the administrator's Pi key only for the provider the
  administrator currently has selected, not for keys saved for other providers.

### Fixed

- A personal key for the local Pi provider is used only with the server URL it was saved for.
  Compacting or resuming a session created on another local URL no longer sends it the key of
  the current one (#20). A `PI_LOCAL_API_KEY` set in the environment is not affected.
- Activity logs hide a saved key in full when another, shorter saved key is its beginning.
- In FinOps, the Manager row of a model that was not recorded reads `unknown` instead of
  ending with a separator.

### Security

- The `url` crate is updated to 2.5.9, which fixes a stack overflow when reading the origin of
  a deeply nested `blob:` URL
  ([RUSTSEC-2026-0592](https://rustsec.org/advisories/RUSTSEC-2026-0592)).

## [1.6.1] - 2026-11-19

### Added

- A Local CI example, `examples/local-ci`: import its workflow to watch the server run a
  project's tests, then change the test command to inspect an intentional failure. It needs
  no API key or connector. Contributed by
  [@Golden007-prog](https://github.com/Golden007-prog) (#17).
- A guide to spec-driven development with rightcommit, `SPEC-DRIVEN-DEVELOPMENT.md`: a worked
  example from spec to verified commit, the SDD workflow templates, and how to use Spec Kit,
  OpenSpec or another framework.
- A guide to the Local CI step, `guides/CONFIGURING_LOCAL_CI.md`: how the test command is
  chosen or detected, the tools it needs, and the example above.

### Changed

- Schedules and the FinOps timestamps use the `jiff` crate instead of `chrono` and
  `chrono-tz`, which are no longer maintained
  ([RUSTSEC-2026-0543](https://rustsec.org/advisories/RUSTSEC-2026-0543)). The time zone
  database stays built into rightcommit, and schedules still skip a local time that a daylight
  saving change removes and run once when one repeats.
- The README opens with what you can build, from any agentic setup to security reviews, and
  explains how to restart a run from a chosen step and inspect each step's files. The details
  of the Local CI step moved to the new guide.
- A test covers how the Local CI step picks `GNUmakefile`, then `makefile`, then `Makefile`,
  as make does. Contributed by [@Golden007-prog](https://github.com/Golden007-prog) (#16).

## [1.6.0] - 2026-11-15

### Added

- While the Project Manager works, one line in the chat follows the tool it is using, such as
  "Opens github.com/…" or "Runs python3 …". It goes away when the reply arrives, when you stop
  it or on an error.
- A **Stop** button replaces **Send** while the Project Manager works. It ends the current
  turn and its agent processes; your message stays in the chat and you can send a new one.
- A restart icon on your messages in the Project Manager chat restarts the conversation from
  that message: the message and the ones after it are removed, then it is sent again in a new
  agent session that knows only the earlier messages.

### Fixed

- When the provider's safety rules block a request, for example about penetration testing or
  exploit tools, the agent error now says so instead of pointing to the agent configuration.
- A Project Manager turn with Pi that times out before using any tool is retried once. Turns
  that already used tools are not repeated, and a turn that hits rightcommit's own time limit
  says so.

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
