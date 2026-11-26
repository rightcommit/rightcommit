# Configuring Codex

rightcommit runs the official, unmodified Codex CLI. Supported configuration uses API keys
or authorized provider configuration for individual or internal organizational use.
Read [Provider terms](PROVIDER_TERMS.md) and [Intended use](INTENDED_USE.md).

## What you need

An **OpenAI API key** from the OpenAI platform, with API usage billed by OpenAI, or a
compatible provider configured by the administrator.

**ChatGPT subscription:** it cannot be entered in the app. rightcommit has no ChatGPT
sign-in, and the API key field expects an OpenAI platform API key. With a native install, a
person with a terminal on the computer can sign in to Codex themselves, in the folder
rightcommit uses for their account, and with Docker or Podman inside the app's container; see
[Your own Codex sign-in](#4-your-own-codex-sign-in). OpenAI recommends API key
authentication for programmatic Codex CLI workflows; with an API key, Codex uses standard API
pricing instead of the credits included in a ChatGPT plan. See the
[official authentication documentation](https://learn.chatgpt.com/docs/auth?surface=cli).

## Original vendor CLI

rightcommit uses Codex CLI as supplied by OpenAI, installed from the official
`@openai/codex` package at the version pinned by the distribution. rightcommit does
not patch its source code or executables, alter its internal operation, or replace its
native authentication mechanisms or model-access protocols. The CLI itself handles
provider authentication and model requests using its supported interfaces.

rightcommit orchestrates the original CLI through command-line options, environment
variables and supported configuration. The application-level rules below determine which
API key or provider configuration is supplied to each user's process; they do not modify
the CLI's built-in access mechanisms. Provider agreements, permissions and usage limits
continue to apply. This statement is not a certification of overall terms compliance.

## 1. API key in the app

Create a key on the [OpenAI platform](https://platform.openai.com/api-keys).
Open **Personal settings → Agents → Codex**, enter the API key and save.
The full key is encrypted in the database and is not returned by the app.
Leave the field empty to retain it; use the bin button to remove it.

The administrator's saved Codex API key is also the default for other users who have no
personal Codex key. Each user's own key overrides it. Shared keys are never returned to
other users or copied into their settings.

## 2. Configuration through environment variables

This section applies only to administrator ID 1. Other users do not inherit provider
credentials, cloud/gateway configuration, endpoints or model settings from the server environment.
Their shared fallback is exclusively the administrator's API key saved in Personal settings.
The administrator's Project Manager also inherits the server's other environment variables,
but not these: they keep the rules below. See
[Agents and the server environment](README.md#agents-and-the-server-environment).

Edit `.env` for Docker and recreate the service with `docker compose up -d`. Protect the
file with `chmod 600 .env`. For native installations, export variables before startup.

| Variable | Purpose |
|---|---|
| `CODEX_API_KEY` | Organizational API key for Codex |
| `OPENAI_API_KEY` | Used when `CODEX_API_KEY` is empty |
| `OPENAI_BASE_URL` | Authorized OpenAI-compatible endpoint |

Both Compose files forward the declared CLI variables. Custom providers can be selected
through the user's private `.codex/config.toml`; their own provider configuration and
agreement apply. AWS variables are forwarded for native provider configuration when no
app API key is selected. Credential files must be available inside the container.

## 3. Precedence

For the default OpenAI provider:

1. The user's Codex API key in Personal settings.
2. The administrator's Codex API key in Personal settings.
3. For the administrator only, `CODEX_API_KEY`, then `OPENAI_API_KEY` from the environment.
4. A sign-in made with `codex login` in the account's own Codex folder; see
   [Your own Codex sign-in](#4-your-own-codex-sign-in).

An app API key excludes global endpoint overrides. A custom provider explicitly selected
in the user's native configuration uses that provider's authentication; remove that selection
to use the default OpenAI route. See the [official authentication documentation](https://learn.chatgpt.com/docs/auth?surface=cli).

Reasoning is passed as `model_reasoning_effort`. Proxy and certificate variables are listed
in `.env.example`. Recreate the container after environment changes; a plain restart does
not reload `.env`.

## 4. Your own Codex sign-in

With a native install (Homebrew, crates.io or source), or with Docker or Podman, you can sign
in to Codex with `codex login`, for example with your ChatGPT account, and rightcommit uses
that sign-in for your own runs. The sign-in is made by you with the original Codex CLI, in a
terminal on the computer that runs rightcommit (with Docker or Podman, inside the app's
container); rightcommit has no sign-in page and never sees your password.

**Signing in to Codex in your own terminal is not enough.** rightcommit starts Codex with a
clean environment and a separate folder for each account: `CODEX_HOME` is set to
`users/<id>/.codex` in the app's data folder, not your own `~/.codex`. A `CODEX_HOME` you
export before starting rightcommit is ignored. Sign in inside that folder instead.

### Native install

The steps below are for the first administrator (the first account created, ID 1) and the
default data folder, `data` in the folder you start rightcommit from, for example
`~/rightcommit`. If you set `NODO_DATA`, use that folder in place of `data`.

1. Check that your account's folder exists:

   ```bash
   cd ~/rightcommit
   ls data/users/1
   ```

   If it does not exist, start `rightcommit` once, create the administrator in the browser
   and come back. The folder is created the first time the account is used.

2. Sign in to Codex in that folder, with the Codex that `rightcommit setup` installed, which
   is the one rightcommit runs:

   ```bash
   mkdir -p data/users/1/.codex
   CODEX_HOME="$PWD/data/users/1/.codex" data/system/tools/bin/codex login
   ```

   Give `CODEX_HOME` as an absolute path, as `$PWD` does here. If `data/system/tools/bin/codex` does not exist,
   run `rightcommit setup` from this folder first.

3. Start `rightcommit` and open **Personal settings → Agents → Codex**. Leave the API key
   field empty: a saved API key, yours or the administrator's, and `CODEX_API_KEY` or
   `OPENAI_API_KEY` in the environment are used instead of the sign-in (see
   [Precedence](#3-precedence)). **Test connection** sends one short request to the model.

What to know:

- The sign-in belongs to one account of one data folder. Another rightcommit account has its
  own folder, `data/users/<id>/.codex`, and needs its own sign-in or an API key. Starting
  rightcommit from another folder uses another data folder.
- Do not copy `auth.json` from `~/.codex`. Codex renews the tokens in that file, and two
  copies can invalidate each other. Sign in again in the app's folder instead.
- `rightcommit resetdb` removes `data/users`, so sign in again after a reset.
- To sign out, run the same command with `logout` in place of `login`.
- **FinOps** shows runs made with a ChatGPT sign-in with the estimated cost at API prices,
  like any other Codex run: it is an estimate, not a charge on the ChatGPT plan.
- This was verified with a native install on macOS.

### Docker or Podman

With Docker or Podman, Codex is already in the image, and the account's folder is
`/data/users/1/.codex` inside the app's data volume (`rightcommit-data`). Sign in inside the
running container with `docker exec`. You do not need Codex on your computer, and you should
not mount your own `~/.codex` into the container.

The container has no browser, and the usual `codex login` waits for the browser's answer on
port 1455 inside the container, which a browser on your computer cannot reach. Use the
**device code** sign-in instead: Codex prints a link and a one-time code, and you enter the
code in a browser on any computer. OpenAI's
[authentication documentation](https://learn.chatgpt.com/docs/auth?surface=cli) describes it
as a beta and says it must be enabled first: in ChatGPT's security settings for a personal
account, or in the workspace permissions by a workspace administrator.

The steps below are for the first administrator (ID 1) and the container name in
`compose.yaml`, `rightcommit`.

1. Start the app (`docker compose up -d` in the folder with `compose.yaml`), create the
   administrator in the browser if you have not yet, and check that your account's folder
   exists:

   ```bash
   docker exec rightcommit ls -ln /data/users/1
   ```

   The list includes `.codex`, owned by user `20001`. If `/data/users/1` does not exist,
   create the administrator first: the folder is created with the account.

2. Sign in as your account's user inside the container:

   ```bash
   docker exec -it -u 20001:20001 -e HOME=/data/users/1 -e CODEX_HOME=/data/users/1/.codex rightcommit codex login --device-auth
   ```

   Codex prints the link `https://auth.openai.com/codex/device` and a one-time code that
   expires in 15 minutes. Open the link in a browser, sign in to ChatGPT and enter the code.
   The command ends when the sign-in is complete; **Ctrl+C** cancels it.

   Why these options: rightcommit runs each account's agents inside the container as a
   separate system user, number 20000 plus the account ID (`20001` for ID 1), with `HOME` set
   to the account's folder and `CODEX_HOME` to its `.codex` folder. With `-u 20001:20001` the
   sign-in file belongs to that user, so Codex can read it during runs and save it again when
   it renews the tokens. Without `-e CODEX_HOME`, Codex uses another folder and rightcommit
   does not see the sign-in.

3. Check the sign-in:

   ```bash
   docker exec -u 20001:20001 -e HOME=/data/users/1 -e CODEX_HOME=/data/users/1/.codex rightcommit codex login status
   ```

   It prints `Logged in using ChatGPT`. Then open **Personal settings → Agents → Codex** and
   leave the API key field empty: a saved API key, yours or the administrator's, and
   `CODEX_API_KEY` or `OPENAI_API_KEY` in `.env` are used instead of the sign-in (see
   [Precedence](#3-precedence)). The Codex section shows **Credential configured**, and in
   **Personal settings → Environment** the Codex card shows **External configuration**.
   **Test connection** sends one short request to the model.

With Podman, use the same commands with `podman exec` in place of `docker exec`.

What to know:

- If you signed in without `-u 20001:20001`, the files belong to the container's
  administrator: rightcommit shows **Credential configured**, but Codex cannot read the
  sign-in during runs. Give the folder back to your account's user:
  `docker exec rightcommit chown -R 20001:20001 /data/users/1/.codex`.
- The sign-in is kept in the data volume, so it stays after `docker compose pull` and
  `docker compose up -d`. A reset (`docker compose run --rm rightcommit resetdb`, see
  [Resetting and restoring rightcommit](HOW_TO_RESET_RESTORE_RIGHTCOMMIT.md)) and deleting the volume
  (`docker compose down -v`) remove it.
- To sign out, run the command of step 2 with `logout` in place of `login --device-auth`.
- Another rightcommit account uses its own folder and user: `/data/users/<id>/.codex` and
  20000 plus its ID, for example `-u 20002:20002` for ID 2. Each person signs in with their
  own ChatGPT account.
- Do not mount or copy `~/.codex` from your computer: Codex renews the tokens in
  `auth.json`, and two copies can invalidate each other.
- Tested with Docker and the 1.10.2 image (Codex 0.166.0) up to the code prompt: completing
  the sign-in needs a real ChatGPT account and was not part of the test. How rightcommit
  detects the sign-in file, its ownership, keeping it after recreating the container, sign
  out and reset were checked with a placeholder file. Not tested with Podman.

A ChatGPT plan is for the person who holds it. Use your own sign-in for your own runs, do
not share it with other people, and follow OpenAI's terms and usage limits; see
[Provider terms](PROVIDER_TERMS.md). OpenAI recommends API keys for programmatic Codex
workflows such as unattended or scheduled runs.
