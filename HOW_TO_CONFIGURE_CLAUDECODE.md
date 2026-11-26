# Configuring Claude Code

rightcommit runs the official, unmodified Claude Code CLI. The documented configuration
uses API keys or authorized gateway/cloud credentials for individual or internal organizational
use, and the administrator's own Claude subscription for the administrator's own runs.
Read [Provider terms](PROVIDER_TERMS.md) and [Intended use](INTENDED_USE.md).

## What you need

An **Anthropic API key** from the Claude Console, with API usage billed by Anthropic, or an
authorized gateway or cloud provider (Amazon Bedrock, Google Vertex AI, Microsoft Foundry)
configured by the administrator.

**Claude Pro or Max subscription:** the administrator can use their own subscription for their
own runs, with a token they create using Claude Code and put in `.env`; see
[Your own Claude subscription](#4-your-own-claude-subscription-administrator-only). Every
other user needs an API key. The API key field in the settings takes only API keys: it
refuses Claude subscription tokens (`sk-ant-oat…`) with *Enter an Anthropic API key, not a
Claude subscription token.* rightcommit has no Claude account sign-in. Anthropic
[bills the Claude API separately](https://support.claude.com/en/articles/9876003) from a
Claude subscription. Pi with the Anthropic provider refuses subscription tokens and Claude
account logins, for every user: it needs an API key.

## Original vendor CLI

rightcommit uses Claude Code as supplied by Anthropic, installed from the official
`@anthropic-ai/claude-code` package at the version pinned by the distribution. rightcommit does
not patch its source code or executables, alter its internal operation, or replace its
native authentication mechanisms or model-access protocols. The CLI itself handles
provider authentication and model requests using its supported interfaces.

rightcommit orchestrates the original CLI through command-line options, environment
variables and supported configuration. The application-level rules below determine which
API key or provider configuration is supplied to each user's process; they do not modify
the CLI's built-in access mechanisms. Provider agreements, permissions and usage limits
continue to apply. This statement is not a certification of overall terms compliance.

## 1. API key in the app

Create an API key in the [Claude Console](https://console.anthropic.com/settings/keys).
Open **Personal settings → Agents → Claude Code**, enter the API key and save.
The full key is encrypted in the database and is not returned by the app.
Leave the field empty to keep a saved key; use the bin button to remove it.

With one administrator, that key is used by their Claude Code processes. With multiple
users, it is also the default for users without their own Claude Code key. A user's own
key takes precedence. The shared key is never displayed or copied into another user's settings.

## 2. Configuration through environment variables

This section applies only to administrator ID 1. Other users do not inherit provider
credentials, cloud/gateway configuration, endpoints or model settings from the server environment.
Their shared fallback is exclusively the administrator's API key saved in Personal settings.
The administrator's Project Manager also inherits the server's other environment variables,
but not these: they keep the rules below. See
[Agents and the server environment](README.md#agents-and-the-server-environment).

For Docker, edit `.env` beside the Compose file, protect it with `chmod 600 .env`, and run
`docker compose up -d`. For native installations, set the variables before starting the app.
Use credentials authorized for the organization and intended users.

| Method | Variables |
|---|---|
| API key | `ANTHROPIC_API_KEY` |
| Your own Claude subscription | `CLAUDE_CODE_OAUTH_TOKEN`, see [section 4](#4-your-own-claude-subscription-administrator-only) |
| Gateway or proxy | `ANTHROPIC_BASE_URL` and `ANTHROPIC_AUTH_TOKEN`, or an `Authorization`, `x-api-key` or `api-key` line in `ANTHROPIC_CUSTOM_HEADERS` |
| Amazon Bedrock | `CLAUDE_CODE_USE_BEDROCK=1`, `AWS_REGION` and `AWS_BEARER_TOKEN_BEDROCK` or `AWS_ACCESS_KEY_ID` + `AWS_SECRET_ACCESS_KEY` |
| Bedrock Mantle | `CLAUDE_CODE_USE_MANTLE=1` with AWS credentials as above |
| Claude Platform on AWS | `CLAUDE_CODE_USE_ANTHROPIC_AWS=1` and `ANTHROPIC_AWS_API_KEY` (plus `ANTHROPIC_AWS_WORKSPACE_ID` or `ANTHROPIC_AWS_BASE_URL` if your setup needs them) |
| Google Vertex AI | `CLAUDE_CODE_USE_VERTEX=1`, `CLOUD_ML_REGION`, `ANTHROPIC_VERTEX_PROJECT_ID`, `GOOGLE_APPLICATION_CREDENTIALS` |
| Microsoft Foundry | `CLAUDE_CODE_USE_FOUNDRY=1`, `ANTHROPIC_FOUNDRY_RESOURCE`, `ANTHROPIC_FOUNDRY_API_KEY` (or Azure client credentials) |


Both Compose files forward the declared CLI variables. File paths for cloud credentials
and certificates must exist and be readable inside the container. Additional supported
cloud and model variables are listed in `.env.example`.

## 3. Precedence

1. The user's API key in Personal settings.
2. The administrator's Claude Code API key in Personal settings.
3. For the administrator only, environment configuration, in this order: one enabled cloud
   provider, gateway authentication, `CLAUDE_CODE_OAUTH_TOKEN`, then `ANTHROPIC_API_KEY`.

An app API key selects direct Anthropic API billing and excludes global cloud/gateway
credentials and endpoint overrides. The administrator can remove the app key to use environment-based cloud or gateway
configuration. Model defaults remain configurable in the app and environment.
Provider permissions, usage policies and billing remain the credential owner's responsibility.

Changing `.env` requires recreating the container; `docker compose restart` does not reload it.

## 4. Your own Claude subscription (administrator only)

The administrator can run their own Claude Code processes on their own Claude Pro or Max
subscription instead of an API key. Only the first administrator (the first account created,
ID 1) gets it. Other users need their own API key, or use the administrator's API key saved in
Personal settings if there is one; the subscription token is never passed to them.

**Signing in to Claude Code on your computer is not enough.** rightcommit starts Claude Code
with a separate configuration folder for each account (`CLAUDE_CONFIG_DIR` set to
`users/<id>/.claude` in the app's data folder), not your own `~/.claude`. Claude Code keeps a
login with its configuration folder: on Linux in `.credentials.json` inside it, and on macOS in a Keychain
entry tied to that folder, so a session with a different `CLAUDE_CONFIG_DIR` reads a different
entry ([Credential management](https://code.claude.com/docs/en/authentication#credential-management)).
A login you made in a terminal on the same computer is therefore not used, with a native
install (Homebrew, crates.io, source) or with Docker or Podman. Use a token instead:

1. On a computer where Claude Code is installed and a browser is available, run
   [`claude setup-token`](https://code.claude.com/docs/en/authentication#generate-a-long-lived-token)
   in a terminal and approve access in the browser. It prints a token valid for one year and
   does not save it anywhere. Anthropic's documentation says it requires a Pro, Max, Ultra,
   Team or Enterprise plan.
2. Add the token to `.env`:

   ```dotenv
   CLAUDE_CODE_OAUTH_TOKEN=paste-the-token-here
   ```

   - **Docker or Podman:** the `.env` beside the Compose file, then `docker compose up -d`
     (or `podman compose up -d`).
   - **Native install:** the `.env` in the folder you start the app from (for example
     `~/rightcommit/.env`). Stop the app and start it again from that folder.

   Protect the file with `chmod 600 .env`.
3. In **Personal settings → Agents → Claude Code**, leave the API key field empty: a saved
   API key is used instead of the token (see [Precedence](#3-precedence)). A cloud provider,
   `ANTHROPIC_AUTH_TOKEN` or an authentication header in `ANTHROPIC_CUSTOM_HEADERS` in
   `.env` also take precedence over the token.
4. Check: the Claude Code section shows **Credential configured**, and in
   **Personal settings → Environment** the Claude Code card shows
   **External configuration**. **Test connection** sends one short request to the model.

rightcommit reads the token from the environment and passes it only to the administrator's
Claude Code processes. It does not save it in the database or show it, and it replaces it with
`[redacted]` in the agent output it shows. When the token expires, create a new one and replace
it in `.env`.

Anthropic's [Claude Code legal and compliance](https://code.claude.com/docs/en/legal-and-compliance)
page says that developers "may not collect, store, or intermediate Claude.ai credentials or
session tokens — sign-in to a Claude account must complete through Anthropic's own flow", and
that this does not prevent "an end user from signing in to the unmodified Claude Code binary
with their own Claude subscription, including where a platform hosts Claude Code". The same
page says: "Advertised usage limits for Pro, Max, and Ultra plans assume ordinary, individual
usage of Claude Code and the Agent SDK." Use this route only with your own
subscription, on your own installation, for your own runs, and do not share it with other
people: for other users, use API keys. Read that page and your plan's terms yourself; this
guide is not legal advice.
