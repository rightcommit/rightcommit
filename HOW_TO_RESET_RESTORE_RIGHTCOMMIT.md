# Resetting and restoring rightcommit

`rightcommit resetdb` returns an installation to the state of a first install: the next
person who opens the app creates the administrator account, as on day one. This guide
explains what a reset deletes and keeps, how to run it, where it leaves its backup, how to
bring that backup back, and how to delete old backups.

> ⚠️ **A reset deletes data.** Accounts, settings, saved credentials, workflows, runs and
> **every user's files** are removed. The backup it saves contains the database and its
> encryption key, **not the users' files**: agent sign-ins, run workspaces and Project Manager
> spaces cannot be brought back.

**You need:** a working rightcommit installation (Homebrew, crates.io, source, Docker or
Podman) and a terminal opened in the folder you start rightcommit from. A version that has the
`resetdb` command: `rightcommit --help` lists it.

## What a reset deletes and what it keeps

The paths below are those of a native install, where the data lives in `~/rightcommit/data`.
With Docker or Podman the same folders are under `/data` inside the `rightcommit-data` volume.

**Deleted:**

| What | Where |
|---|---|
| Accounts, settings and saved credentials (API keys, connection tokens), workflows, runs, schedules, Project Manager chats, artifacts and run history | the database `data/system/nodo.sqlite`, which is emptied |
| Each user's files: agent sign-ins, run workspaces, Project Manager spaces | `data/users` |
| The key that encrypts the saved credentials (a new one is created at the next start) | `data/system/master.key` |
| The workflow template catalog, if an administrator changed it (the built-in one comes back), and its earlier versions | `data/system/workflow-templates.json`, `data/system/template-history` |
| Earlier versions of the settings and of the A2A connections, copies of artifact files, RAG indexes, recorded ServiceNow operations, and any other file rightcommit keeps in `data/system` | other folders in `data/system` |

**Kept:**

- the agents and tools installed by `rightcommit setup` (`data/system/tools`): you do **not**
  need to run `rightcommit setup` again;
- the bundled resources and Pi extensions (`data/system/assets`) and the downloaded RAG models
  (`data/system/rag-models`);
- earlier backups (`data/system/backups`);
- your projects in `sources` and the `.env` file.

Before changing anything, `resetdb` prints this list for your installation, with the exact
data folder it is about to reset. It finds that folder the same way the app does: `data` in
the current folder, or `NODO_DATA`, also when it is set in the `.env` of the current folder.

### When to use it

Use a reset when you want a clean installation: after a test or a demo, before handing the
installation to someone else, or to wipe every saved secret at once.

For smaller changes you do not need it:

- delete a single workflow from the app;
- remove a saved API key with the bin button next to it in **Personal Settings → Agents**;
- suspend or remove an account in **Users** (administrator only; removing an account keeps
  the work it saved);
- updating rightcommit to a new version never needs a reset.

## Reset

The app must be **stopped** first. While it runs it holds the database, and `resetdb` refuses
with:

> `Il database è in uso: arresta il server rightcommit che usa questa cartella dati (Ctrl+C nel
> suo terminale, oppure docker compose stop) e riprova. Nessun file è stato modificato.`
>
> ("The database is in use: stop the rightcommit server that uses this data folder and try
> again. No file was changed.")

### Native install (Homebrew, crates.io)

1. Stop the app: press **Ctrl + C** in the terminal where it runs.
2. Run the reset from the folder you start the app from:

   ```sh
   cd ~/rightcommit
   rightcommit resetdb
   ```

3. Read the list. To go ahead, type `resetdb` and press **Return** when it asks
   **Scrivi resetdb per confermare** ("type resetdb to confirm"). Anything else, or just
   **Return**, cancels without changing anything.
4. When it prints **Azzeramento completato** ("reset complete"), the line below it,
   **Copia di sicurezza**, shows where the backup is.
5. Start the app again with `rightcommit`.

From source, run the same steps in the folder where you cloned rightcommit and use
`cargo +1.95.0 run --locked -- resetdb` instead of `rightcommit resetdb`.

### Docker

Run these commands in the rightcommit folder (the one with `compose.yaml`):

```sh
docker compose stop
docker compose run --rm rightcommit resetdb
docker compose up -d
```

The second command starts a one-off container on the same volume, shows the list and asks
you to type `resetdb`, like a native install. `docker compose exec rightcommit rightcommit
resetdb` does not work while the app is running: it is refused with the message above.

### Podman

The same commands, with `podman compose`:

```sh
podman compose stop
podman compose run --rm rightcommit resetdb
podman compose up -d
```

### Options

| Option | What it does |
|---|---|
| `--yes` | Skips the question. Needed in scripts and wherever there is no terminal to type in; without it `resetdb` refuses there. |
| `--no-backup` | Does not save the backup. Use it when the goal is to wipe secrets, for example before handing the installation over: a backup keeps the saved credentials together with the key that decrypts them. The reset then cannot be undone. |

You can combine them: `rightcommit resetdb --yes --no-backup`, or with Docker
`docker compose run --rm rightcommit resetdb --yes --no-backup`.

`--no-backup` only skips the new backup: **backups from earlier resets stay**, and so does
`.env`. To wipe every secret, also [delete the old backups](#delete-old-backups) and remove
any keys you put in `.env` yourself.

### After the reset

- Open the app: it asks you to create an account, and **the first account you create becomes
  the administrator**. The old accounts and passwords no longer work.
- Each user enters API keys and signs in to the agents again.
- The workflow template catalog is the built-in one.
- Your projects in `sources` are still there.

Running `resetdb` again on an installation that is already empty changes nothing and prints
**Niente da azzerare** ("nothing to reset").

## Where the backup is

Each reset (unless you used `--no-backup`) creates a new folder in `system/backups`, named
`resetdb-` followed by the date and time of the reset in UTC, for example
`resetdb-20261124T203316Z` (24 November 2026, 20:33:16 UTC).

| Install | Folder |
|---|---|
| Homebrew, crates.io, source | `~/rightcommit/data/system/backups/` (inside the folder you start the app from) |
| Docker, Podman | `/data/system/backups/` inside the `rightcommit-data` volume |

What the folder contains:

| File | What it is |
|---|---|
| `nodo.sqlite` | A complete copy of the database as it was before the reset: accounts with their passwords (hashed), settings with the saved credentials (encrypted), workflows, runs and their history, schedules, Project Manager chats. |
| `master.key` | The key that decrypts the saved credentials in that database. |
| `workflow-templates.json` | The workflow template catalog, if the installation had one in `system`. |
| `template-history` | Earlier versions of that catalog, only if an administrator edited it. |

A backup is created only when the database has something in it. Nothing else is backed up:
in particular, nothing from `users` (agent sign-ins, run workspaces, Project Manager spaces).

> ⚠️ **The backup holds the encrypted credentials together with the key that decrypts them.**
> Anyone who can read the folder can read every API key and token that was saved in the app.
> rightcommit creates it readable only by the account that runs the app. Do not copy it to a
> shared place, and delete it when you no longer need it.

## Restore a backup

A restore puts back the database, its key and the template catalog from one backup folder.

**What comes back:** the accounts and their passwords, each user's settings and saved
credentials (still readable, because the key comes back too), workflows, the list of runs with
their history, schedules, Project Manager chats and the template catalog.

**What does not come back:** each user's files. Agent sign-ins made inside the app, run
workspaces and Project Manager spaces were deleted by the reset and are not in the backup.
Also missing: earlier versions of the settings, copies of artifact files and RAG indexes.
In practice:

- users sign in to the agents again where they used a sign-in instead of an API key;
- old runs appear in the list with their status and history, but **their files are gone**:
  opening a run's files shows an error (`No such file or directory`). Do not retry, continue or
  revise an old run: its workspace is missing. Start a new run of the restored workflow;
- RAG indexes must be built again.

The database and `master.key` must be restored **together, from the same folder**. The saved
credentials in the database are encrypted with that key: with any other key, including the new
one created after the reset, the app cannot read the settings.

Restoring replaces whatever was created after the reset (new accounts, workflows, runs): step 2
below deletes it first.

### Native install (Homebrew, crates.io, source)

1. Stop the app (**Ctrl + C**).
2. Go to the app folder and clear whatever was created since the reset:

   ```sh
   cd ~/rightcommit
   rightcommit resetdb --no-backup
   ```

   Type `resetdb` to confirm. If the app was not started after the reset, it prints
   **Niente da azzerare** ("nothing to reset"): that is fine. `--no-backup` avoids adding a
   backup of the empty installation next to the one you want; your existing backups are kept.
3. List the backups and pick one:

   ```sh
   ls data/system/backups
   ```

4. Copy its files back, replacing `resetdb-20261124T203316Z` with the folder you picked:

   ```sh
   rm -f data/system/nodo.sqlite-wal data/system/nodo.sqlite-shm
   cp -Rp data/system/backups/resetdb-20261124T203316Z/. data/system/
   ```

   The first line removes database side files that would otherwise be mixed with the restored
   database; usually they do not exist. The second copies `nodo.sqlite`, `master.key` and the
   template files into `data/system`. The backup stays where it is.
5. Start the app with `rightcommit` and sign in with an old account.
6. Check that your workflows are back and that **Personal Settings → Agents** shows your saved
   API keys. When everything is in place, you can [delete the backup](#delete-old-backups).

From source, use `cargo +1.95.0 run --locked -- resetdb --no-backup` in step 2 and
`cargo +1.95.0 run --locked` in step 5, in the folder where you cloned rightcommit.

### Docker

The data is in the `rightcommit-data` volume, under `/data/system`. While the app is stopped,
each step runs in a one-off container on that volume. Run the commands in the rightcommit
folder:

1. Stop the app:

   ```sh
   docker compose stop
   ```

2. Clear whatever was created since the reset (type `resetdb` to confirm):

   ```sh
   docker compose run --rm rightcommit resetdb --no-backup
   ```

3. List the backups:

   ```sh
   docker compose run --rm --entrypoint ls rightcommit -l /data/system/backups
   ```

4. Copy the files of the backup you picked back into `/data/system`, replacing
   `resetdb-20261124T203316Z` with its name:

   ```sh
   docker compose run --rm --entrypoint rm rightcommit -f /data/system/nodo.sqlite-wal /data/system/nodo.sqlite-shm
   docker compose run --rm --entrypoint cp rightcommit -Rp /data/system/backups/resetdb-20261124T203316Z/. /data/system/
   ```

5. Start the app and sign in with an old account:

   ```sh
   docker compose up -d
   ```

Nothing else is needed: the files keep the owner and permissions they had in the volume, and
each user's private folder is created again the first time it is needed.

### Podman

The same steps with `podman compose`:

```sh
podman compose stop
podman compose run --rm rightcommit resetdb --no-backup
podman compose run --rm --entrypoint ls rightcommit -l /data/system/backups
podman compose run --rm --entrypoint rm rightcommit -f /data/system/nodo.sqlite-wal /data/system/nodo.sqlite-shm
podman compose run --rm --entrypoint cp rightcommit -Rp /data/system/backups/resetdb-20261124T203316Z/. /data/system/
podman compose up -d
```

## Delete old backups

Every reset adds a new backup and nothing removes them automatically. Each one holds a copy of
the saved credentials and the key that decrypts them, so delete those you no longer need.
Deleting a backup does not affect the running app.

### Native install

```sh
cd ~/rightcommit
ls -l data/system/backups
rm -rf data/system/backups/resetdb-20261124T203316Z
```

Replace `resetdb-20261124T203316Z` with the folder to delete. To delete all of them:
`rm -rf data/system/backups/resetdb-*`.

### Docker

With the app running:

```sh
docker compose exec rightcommit ls -l /data/system/backups
docker compose exec rightcommit rm -rf /data/system/backups/resetdb-20261124T203316Z
```

With the app stopped, use `docker compose run --rm --entrypoint ls rightcommit -l
/data/system/backups` to list them and
`docker compose run --rm --entrypoint rm rightcommit -rf /data/system/backups/resetdb-20261124T203316Z`
to delete one.

### Podman

The same commands, with `podman compose` in place of `docker compose`.

## Troubleshooting

`resetdb` prints its messages in Italian. Every error that ends with **Nessun file è stato
modificato** ("no file was changed") left your data as it was.

| Message | What it means and what to do |
|---|---|
| `Il database è in uso: arresta il server rightcommit …` | The app is still running on this data. Stop it (**Ctrl + C**, or `docker compose stop` / `podman compose stop`) and run `resetdb` again. With Docker or Podman, use `compose run --rm`, not `compose exec`. |
| `Cartella dati non trovata: data …` | No data folder where you ran the command. Go to the folder you start the app from (`cd ~/rightcommit`) or set `NODO_DATA` to the data folder, then run it again. |
| `… non è una cartella dati di rightcommit: manca system …` | The folder found is not a rightcommit data folder (it has no `system` folder). Check that you are in the right folder, or that `NODO_DATA` points to the right one. |
| `La cartella dei progetti … si trova dentro …: spostala prima di azzerare …` | Your projects folder (`NODO_SOURCES`) is inside a folder the reset would delete. Move the projects folder outside the data folder, update `NODO_SOURCES`, then run `resetdb` again. |
| `Database non leggibile …` | The database file cannot be opened as a database. Do not try again blindly: keep a copy of the whole data folder and look at the error printed under **Caused by**. |
| `Annullato …` | You did not type `resetdb` exactly at the question. Run the command again and type `resetdb`. |
| `Conferma mancante: per eseguire senza terminale aggiungi --yes …` | There was no terminal to answer the question (a script, a pipe, `docker compose run -T`). Add `--yes` if you really want to reset. |
| `Azzeramento non eseguito; i file sono stati rimessi al loro posto` | A file could not be moved (look at the error below it). Everything was put back. Fix the cause, often permissions on `data`, and run it again. |
| `Spazio non ancora liberato: elimina a mano …` | The reset is complete, but a temporary folder named `resetdb-….trash` in `data/system` could not be deleted. Delete that folder yourself. |
| `Argomento sconosciuto: …` | An option was misspelled. The only options are `--yes` and `--no-backup`. |
| After a restore, the settings page shows `Decifratura fallita` ("decryption failed") | The database was restored without the `master.key` from the same backup folder. Stop the app and repeat [the restore](#restore-a-backup) from step 2, copying the whole folder. |
