# Installing rightcommit with Docker

rightcommit supports one or more users with separate settings and agent homes. Pi, Codex
and Claude Code are available to every user. Before connecting an agent, read
[Provider terms and permitted use](PROVIDER_TERMS.md).

Docker runs rightcommit the same way on **macOS, Windows or Linux**. The container holds
everything the app needs: besides Docker, you don't install anything else.

**You need:** a 64-bit computer (Intel, AMD or ARM) and an Internet connection. The first
start downloads the ready-made rightcommit image, about 1 GB, which needs about 3.5 GB of disk
space once unpacked.

## Which install to choose

- **Mac:** start with [Homebrew](INSTALL_MAC.md#install-with-homebrew). On Apple Silicon Macs
  with macOS 15 or later it downloads rightcommit ready to use in a few seconds, without
  Docker Desktop. On Intel Macs and older macOS versions it compiles rightcommit, which takes
  several minutes and installs Rust.
- **Windows:** Docker (this guide) or [Podman](INSTALL_PODMAN.md). Without containers,
  rightcommit runs inside WSL2: see [INSTALL_LINUX.md](INSTALL_LINUX.md).
- **Linux:** Docker or [Podman](INSTALL_PODMAN.md) download a ready-made image. Without
  containers, [INSTALL_LINUX.md](INSTALL_LINUX.md) compiles the app on your computer.
- **Any system:** Docker or Podman, when you want the same setup everywhere or a separate
  system user for each account. A native installation runs every account as your own user.

The installations also differ in the tools your project's tests use: the image includes a fixed
set (see [Tools for your project's tests](#tools-for-your-projects-tests)), while a native
installation uses the tools installed on your machine. Docker and native installations keep
separate data.

## Step 1 — Install Docker

### macOS

1. Download [Docker Desktop](https://www.docker.com/products/docker-desktop/).
   If it asks which version, check your Mac in the **Apple** menu () → **About This Mac**:
   next to **Chip** you see *Apple* (M1, M2…) or *Intel*.
2. Open the downloaded file and drag Docker into the **Applications** folder.
3. Open **Docker** from Applications and wait until it says it is running.

### Windows

You need 64-bit Windows 10 or 11.

1. Download [Docker Desktop for Windows](https://www.docker.com/products/docker-desktop/).
2. Run the installer and keep the option that uses **WSL 2** selected.
3. If the installer asks, **restart your computer**.
4. Open **Docker Desktop** from the Start menu and wait until it says it is running.
   If it asks you to update WSL, accept.

### Linux

Open a terminal and run Docker's official installation script:

```sh
curl -fsSL https://get.docker.com | sudo sh
sudo usermod -aG docker $USER
```

**Log out and log back in** (or restart), then check that `docker compose version` shows a
version number.

## Step 2 — Download rightcommit

1. On the [project's GitHub page](https://github.com/rightcommit/rightcommit) click **Code** → **Download ZIP**.
2. Extract the ZIP file (double-click on macOS; right-click → **Extract All** on Windows).

## Step 3 — Start the app

Open a terminal in the extracted folder (the one that contains **compose.yaml**):

- **macOS or Linux:** in the terminal type `cd ` (with a space), drag the folder into the
  window and press **Return**.
- **Windows:** in File Explorer right-click an empty area of the folder → **Open in Terminal**.

Then run:

```sh
docker compose up -d
```

The first time, Docker downloads the rightcommit image of the version you downloaded: it takes
a few minutes, depending on your connection. When the cursor comes back the app is running, and
you can close the terminal. To build the image from the source code instead, see
[Build the image yourself](#build-the-image-yourself).

## Step 4 — Open the app

1. Open your browser at **[http://localhost:8080](http://localhost:8080)**.
2. Create the first account with an email and a password: it becomes the **administrator**.
3. Open **Personal settings** (the sliders button left of **Run workflow**) → **Agents**, paste
   the API key of the agent you want to use (Codex, Claude Code or Pi) and save. While the
   Default agent (Codex on a new account) has no credentials, the first agent you save a key for
   becomes the Default.
   You can also do this later: the first run below needs no API key.

Next: [your first run](#your-first-run).

## Your first run

Two runs on projects bundled with rightcommit, to check the install before you use your own
code. The first needs no API key; the second needs one agent. Neither needs GitHub, Jira,
ServiceNow or any other external account.

### 1. Local CI example: no API key

This workflow runs the tests of a small Python project on the server. It calls no model and no
external service. It needs Python 3 where rightcommit runs: the Docker and Podman image
includes it. On a native installation, check that `python3 --version` works.

1. If the **No agents configured** dialog is open, press **Close**.
2. Press **+** (**New workflow**). Under **Get started**, choose **Local CI example**. If your
   list does not include it, as on an installation upgraded from an earlier version, choose
   **Import workflow** instead, select
   [`workflow.json`](https://github.com/rightcommit/rightcommit/blob/main/examples/local-ci/workflow.json),
   press **Import** and go to step 4.
3. Keep or change the proposed name, and press **Save**.
4. The new workflow opens with two steps. **Bundled calculator project** is a Source of type
   **Folder in examples**: the project is already inside rightcommit, and each run copies it
   into its own workspace. **Local CI tests** runs the test command on the
   server, `python3 -m unittest -v test_calculator`.
5. Press **Run workflow**, then **Run now**. In **Start workflow**, leave the fields as they are
   and press **Start**.
6. The bar above the canvas shows **Working on Run** with the run's short ID, then
   **Completed**. Both steps get a check mark.
7. Select **Local CI tests**. The **Activity log** below the canvas shows the command that ran
   and the last lines of its output, with `Ran 1 test` and `OK`. For all of the output, open
   **Report** under **Artifacts** in the side panel and select `.rightcommit/reports/tests.md`:
   it also shows `test_add ... ok`.

To see a failing test, follow [Run a failing test](examples/local-ci/README.md#run-a-failing-test):
the run ends with **Error**, and the Activity log shows the failed assertion.

### 2. Feature → SDD → Delivery: one agent

This template works on Northstar Helpdesk, a small Rust project bundled with rightcommit. The
agent uses Spec Kit to write the constitution, specification, plan and tasks for the feature
described in the project's README, then implements it. The server runs `cargo test`, and the
last step commits the result in the run's workspace. Nothing is pushed, merged or deployed.

You need one agent configured in **Personal settings → Agents**, with a model (see
[Configuring the agents](README.md#configuring-the-agents)): every step uses your **Default**
agent. Saving the first API key of an agent makes it the Default while the Default agent (Codex on
a new account) has no credentials. If your agent's credentials come from elsewhere, such as
`CLAUDE_CODE_OAUTH_TOKEN` in `.env` or the administrator's key, turn on **Default** next to it in
**Personal settings → Agents**.
The test step needs Rust (`cargo`) where rightcommit runs: the Docker and Podman image includes
it. The run has many agent steps, so it uses more tokens than a single prompt.

1. Press **+**. Under **Develop and evolve with SDD**, choose **Feature → SDD → Delivery**, keep or
   change the name, and press **Save**.
2. The Source node is **Northstar Helpdesk** (**Preloaded repository**). Press
   **Run workflow** → **Run now** → **Start**, without filling in the request: the steps already
   describe the feature.
3. The run stops for you at **Design review** and **Code review**, and when the agent has a
   question. **Review required** appears above the canvas and **Awaiting review** counts the
   steps waiting: press **Review required**, read the request, then **Approve** or **Reject**
   with a note.
4. Select any step: the side panel lists under **Artifacts** the files the run created or
   changed, with their versions and differences, and under **Report** one report per step,
   such as `.rightcommit/reports/tests.md` for the tests. **FinOps** shows the tokens and costs
   of the run. **Storage** → **Runs** lets you browse the run's files and download them with
   **Download result ZIP**.

If no agent is configured, **Start** is refused before the run begins, with a message that names
the agents to configure in **Personal settings → Agents**; when another agent is configured, it
also suggests choosing that one as **Default**. If a step fails, the **Activity log**
shows why. With a wrong API key, the first agent step (**Constitution**) stops with an error
from the provider.

**Existing software → SDD baseline** has fewer steps: it analyzes the code and writes its
documents under `analysis/` without changing the application code, then ends with one review.

### Which templates need what

| Tag in the **+** list | What you need |
|---|---|
| **Local** | One agent, except **Local CI example**, which needs none. When the card also names an agent, that agent too: **Incident → Living spec → Delivery** shows **Local · Codex** because it uses Codex for triage. |
| **Claude**, **Codex** | Those agents, configured. No external account. |
| GitHub, Jira, Bitbucket, Confluence, ServiceNow | That service connected in **Personal settings → Connections**, and the values to replace that the card lists. See the [configuration guides](guides/README.md). |

## Everyday use

Run these commands from the rightcommit folder, while Docker is running.

- **Stop the app:** `docker compose down`
- **Start it again:** `docker compose up -d`
- **Update it** after downloading a new version: `docker compose pull`, then `docker compose up -d`
- **See errors:** `docker compose logs --tail=100`
- **Start over with empty data**, as on a first install, without removing the volume:

  ```sh
  docker compose stop
  docker compose run --rm rightcommit resetdb
  docker compose up -d
  ```

  `resetdb` lists what it will delete and asks you to type `resetdb` to confirm. Accounts,
  settings and saved credentials, workflows, runs and each user's files are deleted; the
  folders in `sources` are kept. A copy of the database stays in the volume under
  `/data/system/backups` (add `--no-backup` to skip it). Every reset adds a new copy there (the
  database and its encryption key, in a folder named `resetdb-` with the date and time); delete
  old copies yourself when you no longer need them. Stop the app first: while it runs it holds
  the database, so `docker compose exec` is refused. At the next start, the first account you
  create becomes the administrator.
  To restore the backup a reset leaves, or to delete old backups, see
  [Reset and restore](HOW_TO_RESET_RESTORE_RIGHTCOMMIT.md).

`compose.yaml` names the image of its own version, so downloading a new version is what moves
you to it. To change version without downloading the files again, set
`RIGHTCOMMIT_VERSION=X.Y.Z` in `.env` (or `latest`, for the newest release), then run the two
update commands.

> ⚠️ Never use `docker compose down -v`: it **deletes all the app's data**
> (the `rightcommit-data` volume).

When a newer version is published on GitHub, a red bell appears on your account avatar and
the account menu shows `vX.Y.Z available`, with a link to the release. The server asks
GitHub at most every six hours; nothing about your account or installation is sent. To turn
the check off, for example on a network without Internet access, set
`RIGHTCOMMIT_UPDATE_CHECK=0` in `.env` and run `docker compose up -d`.

## Build the image yourself

`compose.yaml` downloads the image that GitHub builds for each release
(`ghcr.io/rightcommit/rightcommit`). To build it from the source code in the folder instead,
for example to try your own changes, run:

```sh
docker compose -f compose.yaml -f compose.build.yaml up -d --build
```

The first build takes 10–30 minutes; later builds reuse what is already compiled. Use this
command each time you start or update a build of your own: plain `docker compose up -d` goes
back to the downloaded image. Your data stays in the `rightcommit-data` volume either way.

## Your own project folders

To work on your own code, put the project in the `sources` folder inside the rightcommit folder
(the one with `compose.yaml`). The container sees it as `/sources`. For example, from the
rightcommit folder:

```sh
git clone https://github.com/your-name/your-project.git sources/your-project
```

You can also copy the folder there with your file manager, or upload it from the browser:
**Storage** → **sources** → **Upload folder** (at most 50 MB per file).

Then, in a workflow:

1. Select the **Source** node and set **Source type** to **Folder in /sources**.
2. Click **Path or URL** and pick your project from the list of folders in `/sources`, or type a
   subfolder path. If the folder does not exist yet, **Start** asks whether to create it:
   **Create** makes the empty folder in `/sources` and starts the run, **No** starts nothing.
   Scheduled runs never create folders: they stop with an error that names the Source and the
   folder.
3. Choose **Source handling**: **Create a copy of the source** works on a copy in the run's
   workspace and leaves your folder unchanged; **Permanently modify source** lets the run edit
   your folder directly. When a folder is larger than the copy threshold, the app asks before
   copying it and suggests working directly in `/sources`, because each run makes a new copy.
4. When you start the run, write the goal in **Request and additional context**.

The Local CI example works with your project too: point its Source to your folder and clear
**Test command** on **Local CI tests**, so the step picks the command from your project's files
(see [Configuring Local CI tests](guides/CONFIGURING_LOCAL_CI.md)). For an agent, start from
**Existing software → SDD baseline** or **Idea → Approved specification**, which read the
request you write at the start. The **Specify**, **Clarify** and **Checklist** steps of
**Feature → SDD → Delivery** describe the Northstar feature: rewrite their prompts before using
it on another project.

A public repository also works without copying: set **Source type** to
**HTTPS Git repository** and enter its HTTPS URL, without credentials. Each run clones it.

## Tools for your project's tests

The **Local CI** step runs your project's tests inside the container, with the tools installed
there. The image includes Rust (`cargo`), Node.js with npm, Python 3 with `pip` and `venv`, make
and gcc. It does not include Go, Java, Maven or Gradle.

A Python project installs its dependencies, pytest included, in a virtual environment inside
its workspace, the way a Node.js project installs them in `node_modules`:

```sh
python3 -m venv .venv
.venv/bin/pip install -r requirements.txt pytest
```

`pip install` outside a virtual environment is refused by Debian. Create virtual environments in the
workspace, not under `/tmp`: it is mounted without permission to run programs. When the tests
need the virtual environment, set the Local CI step's test command to `.venv/bin/python -m pytest`:
the command detected for `pyproject.toml`, `pytest.ini` or `setup.py` is `python3 -m pytest`, which
uses the system Python.

When a run copies a Source folder into its workspace, it leaves out `.git`, `target` and these
dependency, cache and generated build folders, at any depth: `.venv`, `venv`, `__pycache__`,
`.pytest_cache`, `.tox`, `.nox`, `.mypy_cache`, `.ruff_cache`, `.ipynb_checkpoints`,
`node_modules`, `bower_components`, `.next`, `.nuxt`, `.svelte-kit`, `.turbo`, `.parcel-cache`,
`.angular`, `.pnpm-store`, `.nyc_output`, `.gradle`, `_build`, `.build`, `DerivedData`,
`.dart_tool`, `.terraform`, `.stack-work`, `dist-newstyle`, `zig-cache`, `.zig-cache`, `zig-out`,
`CMakeFiles`, `.ccls-cache`, `.cache`, `.direnv`. Each run creates its own; a file with one of
these names is copied. A project that commits one of these folders does not find it in a copied
workspace; with **Permanently modify source** the run works in the folder itself and keeps it.
These folders do not count toward the copy size limit, the symbolic links inside them do not stop
a run, and they are not shown in the Artifacts view, the workspace in Storage or the result ZIP.

To add tools, build your own image on top of rightcommit. In the rightcommit folder:

1. Create `Dockerfile.tools` with the Debian packages you need:

   ```dockerfile
   FROM ghcr.io/rightcommit/rightcommit:latest
   RUN apt-get update && apt-get install -y --no-install-recommends golang-go maven gradle \
       && rm -rf /var/lib/apt/lists/*
   ```

2. Create `compose.override.yaml`, which Docker Compose reads together with `compose.yaml`:

   ```yaml
   services:
     rightcommit:
       image: rightcommit:tools
       build:
         context: .
         dockerfile: Dockerfile.tools
   ```

3. Build the extended image on the newest rightcommit image, then start it:

   ```sh
   docker compose build --pull
   docker compose up -d
   ```

Repeat step 3 to update, instead of `docker compose pull`. If you build rightcommit yourself,
write `FROM rightcommit:local` in step 1, build it first with
`docker compose -f compose.yaml -f compose.build.yaml build`, and leave out `--pull` in step 3.
Debian packages can be older than the latest releases (Gradle is at 4.4): install a newer
version from its official site if your project needs it.

The same image is the place for the programs that a Source **setup command** runs, such as
`openspec` (see [ADD-A-FRAMEWORK.md](ADD-A-FRAMEWORK.md)).

Outside Docker, the test and setup commands use the tools installed on your machine.

## Troubleshooting

| Problem | What to do |
|---|---|
| The `localhost:8080` page doesn't open | Check that Docker is running, then look at the errors with `docker compose logs --tail=100`. |
| Error about port 8080 already in use | Another app is using the port. Stop it, or stop other copies of rightcommit. |
| Windows: Docker Desktop says WSL is missing or needs an update | Open PowerShell as administrator, run `wsl --update`, then restart Docker Desktop. |
| `docker compose` says `denied` or `not found` for `ghcr.io/rightcommit/rightcommit` | The image of this version is not published yet: GitHub builds it shortly after each release. Try again later, or [build the image yourself](#build-the-image-yourself). |
| Windows: the build fails with errors about `install-twg.sh` or `\r` | Your copy was downloaded before line endings were fixed. Download the project again. |
| Linux: `permission denied` from Docker | Log out and back in after `usermod`, or use `sudo docker compose …`. |

## Next steps

- Give the Pi agent extra tools: [ADD-PI-PACKAGES.md](ADD-PI-PACKAGES.md)
- Add a skill to all agents: [ADD_A_SKILL.md](ADD_A_SKILL.md)
- Use Spec Kit, OpenSpec or another framework with your project: [ADD-A-FRAMEWORK.md](ADD-A-FRAMEWORK.md)
