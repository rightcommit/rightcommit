# Installing rightcommit with Podman

rightcommit supports one or more users with separate settings and agent homes. Pi, Codex
and Claude Code are available to every user. Before connecting an agent, read
[Provider terms and permitted use](PROVIDER_TERMS.md).

Podman is a free alternative to Docker. It runs rightcommit with everything included, on
**macOS, Windows or Linux**. To use Docker instead, see [INSTALL.md](INSTALL.md).

**You need:** a 64-bit computer (Intel, AMD or ARM) and an Internet connection. The first
start downloads the ready-made rightcommit image, about 1 GB.

## Step 1 — Install Podman

### macOS

With [Homebrew](https://brew.sh), in Terminal:

```sh
brew install podman podman-compose
podman machine init --memory 6144
podman machine start
```

The `podman machine` commands create the small Linux machine Podman runs in; 6 GB of memory
is enough to run the app and to build it from source. You only run `init` once.

### Windows

You need 64-bit Windows 10 or 11.

1. Download and install [Podman Desktop](https://podman-desktop.io/).
2. Open it and follow the first-start setup: let it install **Podman** and **Compose**,
   and create the Podman machine. If it asks to enable or update WSL, accept and restart.

### Linux

On Ubuntu or Debian:

```sh
sudo apt update
sudo apt install -y podman podman-compose
```

On Fedora:

```sh
sudo dnf install -y podman podman-compose
```

On Linux there is no Podman machine to create.

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
podman compose up -d
```

The first time, Podman downloads the rightcommit image of the version you downloaded: it takes
a few minutes, depending on your connection. When the cursor comes back the app is running, and
you can close the terminal.

To build the image from the source code instead (10–30 minutes the first time), run
`podman compose -f compose.yaml -f compose.build.yaml up -d --build`, and use the same command
to update it. See [Build the image yourself](INSTALL.md#build-the-image-yourself).

## Step 4 — Open the app

1. Open your browser at **[http://localhost:8080](http://localhost:8080)**.
2. Create the first account with an email and a password: it becomes the **administrator**.
3. Open **Agents and connections**, paste the API key of the agent you want to use
   (Codex, Claude Code or Pi) and save.

Next: [your first run](INSTALL.md#your-first-run). The steps are the same as with Docker, and
the image includes Python 3 and Rust for the two bundled examples.

## Everyday use

Run these commands from the rightcommit folder. On macOS and Windows, the Podman machine
must be running: if it is off, run `podman machine start` (or start it from Podman Desktop).

- **Stop the app:** `podman compose down`
- **Start it again:** `podman compose up -d`
- **Update it** after downloading a new version: `podman compose pull`, then `podman compose up -d`
- **See errors:** `podman compose logs --tail=100`
- **Start over with empty data**, as on a first install, without removing the volume:

  ```sh
  podman compose stop
  podman compose run --rm rightcommit resetdb
  podman compose up -d
  ```

  `resetdb` lists what it will delete and asks you to type `resetdb` to confirm. Accounts,
  settings and saved credentials, workflows, runs and each user's files are deleted; the
  folders in `sources` are kept. A copy of the database stays in the volume under
  `/data/system/backups` (add `--no-backup` to skip it). Every reset adds a new copy there (the
  database and its encryption key, in a folder named `resetdb-` with the date and time); delete
  old copies yourself when you no longer need them. Stop the app first: while it runs it holds
  the database, so `podman compose exec` is refused. At the next start, the first account you
  create becomes the administrator.
  To restore the backup a reset leaves, or to delete old backups, see
  [Reset and restore](HOW_TO_RESET_RESTORE_RIGHTCOMMIT.md).

> ⚠️ Never use `podman compose down -v`: it **deletes all the app's data**
> (the `rightcommit-data` volume).

## Your own project folders

To work on folders from your computer, put them in the `sources` folder inside the
rightcommit folder, then use a **Source** node of type **Folder in /sources**. See
[Your own project folders](INSTALL.md#your-own-project-folders) for the steps.

## Troubleshooting

| Problem | What to do |
|---|---|
| `Cannot connect to Podman` (macOS or Windows) | The Podman machine is off. Run `podman machine start`, or start it from Podman Desktop. |
| `podman compose` says no compose provider is installed | macOS and Linux: install `podman-compose` (step 1). Windows: install Compose from Podman Desktop. |
| `podman compose` says `denied` or `not found` for `ghcr.io/rightcommit/rightcommit` | The image of this version is not published yet: GitHub builds it shortly after each release. Try again later, or build it from source (step 3). |
| A build from source stops with an out-of-memory error (macOS) | Give the machine more memory: `podman machine stop`, `podman machine set --memory 6144`, `podman machine start`. |
| The `localhost:8080` page doesn't open | Check that the app is running, then look at the errors with `podman compose logs --tail=100`. |
| Error about port 8080 already in use | Another app, or Docker running rightcommit, is using the port. Stop it. |
| Windows: the build fails with errors about `install-twg.sh` or `\r` | Your copy was downloaded before line endings were fixed. Download the project again. |

## Next steps

- Give the Pi agent extra tools: [ADD-PI-PACKAGES.md](ADD-PI-PACKAGES.md)
- Add a skill to all agents: [ADD_A_SKILL.md](ADD_A_SKILL.md)
- Use Spec Kit, OpenSpec or another framework with your project: [ADD-A-FRAMEWORK.md](ADD-A-FRAMEWORK.md)
