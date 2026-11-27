use crate::{
    App,
    model::*,
    store::{hash, private},
};
use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::Stdio,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::Command,
};

pub(crate) struct ProcessGroup(pub u32);
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        unsafe {
            libc::kill(-(self.0 as i32), libc::SIGKILL);
        }
    }
}

pub fn user_home(app: &App, uid: i64) -> Result<PathBuf> {
    let users = app.data.join("users");
    std::fs::create_dir_all(&users)?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&app.data, std::fs::Permissions::from_mode(0o711))?;
    std::fs::set_permissions(&users, std::fs::Permissions::from_mode(0o711))?;
    let home = users.join(uid.to_string());
    ensure!(!home.is_symlink(), "Home utente non valida");
    std::fs::create_dir_all(&home)?;
    private(&home, true)?;
    own(app, uid, &home)?;
    for d in [
        ".config",
        ".config/twg",
        ".aws",
        ".codex",
        ".claude",
        ".pi",
        ".pi/agent",
        "workspaces",
        "manager",
        "manager/mcp-servers",
        "manager/projects",
        "tmp",
        ".cargo",
    ] {
        let p = home.join(d);
        ensure!(!p.is_symlink(), "Directory runtime non valida");
        std::fs::create_dir_all(&p)?;
        own(app, uid, &p)?;
    }
    Ok(home)
}
pub fn own(app: &App, uid: i64, p: &Path) -> Result<()> {
    #[cfg(target_os = "linux")]
    if app.isolate {
        use std::os::unix::ffi::OsStrExt;
        let c = std::ffi::CString::new(p.as_os_str().as_bytes())?;
        ensure!(
            unsafe { libc::lchown(c.as_ptr(), (20000 + uid) as u32, (20000 + uid) as u32) } == 0,
            "chown fallito"
        );
    }
    let _ = (app, uid, p);
    Ok(())
}
pub fn own_tree(app: &App, uid: i64, p: &Path) -> Result<()> {
    // Native installs never change ownership. Avoid a recursive no-op stat walk.
    if !cfg!(target_os = "linux") || !app.isolate { return Ok(()); }
    own(app, uid, p)?;
    // The agent may own and change the tree while the server runs as root: every entry is
    // changed relative to a folder opened without following links, never by its path.
    let id = (20000 + uid) as libc::uid_t;
    walk_nofollow(p, &mut |dir, name| {
        if unsafe { libc::fchownat(dir, name.as_ptr(), id, id as libc::gid_t, libc::AT_SYMLINK_NOFOLLOW) } == 0 {
            return Ok(());
        }
        let error = std::io::Error::last_os_error();
        // Docker Desktop on macOS refuses even root a new owner for a read-only file of a mounted
        // host folder (every Git object of a repository made on the Mac): such an entry, looked at
        // relative to the same folder without following it, is skipped only when it grants no more.
        if matches!(error.raw_os_error(), Some(libc::EACCES | libc::EPERM)) {
            let mut st: libc::stat = unsafe { std::mem::zeroed() };
            if unsafe { libc::fstatat(dir, name.as_ptr(), &mut st, libc::AT_SYMLINK_NOFOLLOW) } == 0
                && owner_can_stay(st.st_mode as u32, st.st_uid, st.st_gid, id, id as libc::gid_t)
            {
                return Ok(());
            }
        }
        bail!("chown fallito: {error}")
    })
}
/// Whether an entry whose owner cannot be changed may stay as it is: it already has the wanted
/// owner, or it is a regular file without any write bit, which nobody was going to write and the
/// user can still read when its mode allows; leaving it gives the user less access, never more.
/// Folders, links and writable files always need the new owner.
pub(crate) fn owner_can_stay(mode: u32, uid: u32, gid: u32, wanted_uid: u32, wanted_gid: u32) -> bool {
    (uid == wanted_uid && gid == wanted_gid)
        || (mode & libc::S_IFMT as u32 == libc::S_IFREG as u32 && mode & 0o222 == 0)
}

/// Calls `visit(folder, name)` for every entry below `root`, at any depth. Each folder is opened
/// relative to its parent with `O_NOFOLLOW`, so a link is visited (and can be changed itself) but
/// never entered, even when a folder is replaced by a link while the walk runs. A `root` that is a
/// link or not a folder has nothing below it.
pub(crate) fn walk_nofollow(
    root: &Path,
    visit: &mut dyn FnMut(libc::c_int, &std::ffi::CStr) -> Result<()>,
) -> Result<()> {
    use std::os::unix::ffi::OsStrExt;
    const FLAGS: libc::c_int = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
    fn not_folder(error: &std::io::Error) -> bool {
        matches!(error.raw_os_error(), Some(libc::ENOTDIR | libc::ELOOP | libc::ENOENT))
    }
    fn clear_errno() {
        #[cfg(any(target_os = "linux", target_os = "android"))]
        unsafe { *libc::__errno_location() = 0 }
        #[cfg(any(target_os = "macos", target_os = "ios", target_os = "freebsd"))]
        unsafe { *libc::__error() = 0 }
    }
    struct Folder(*mut libc::DIR);
    impl Drop for Folder {
        fn drop(&mut self) {
            unsafe { libc::closedir(self.0) };
        }
    }
    fn walk(fd: libc::c_int, visit: &mut dyn FnMut(libc::c_int, &std::ffi::CStr) -> Result<()>) -> Result<()> {
        let stream = unsafe { libc::fdopendir(fd) };
        if stream.is_null() {
            let error = std::io::Error::last_os_error();
            unsafe { libc::close(fd) };
            return Err(error.into());
        }
        let folder = Folder(stream);
        loop {
            clear_errno();
            let entry = unsafe { libc::readdir(folder.0) };
            if entry.is_null() {
                let error = std::io::Error::last_os_error();
                ensure!(error.raw_os_error().unwrap_or(0) == 0, "Lettura cartella non riuscita: {error}");
                return Ok(());
            }
            let (name, kind) = unsafe { (std::ffi::CStr::from_ptr((*entry).d_name.as_ptr()).to_owned(), (*entry).d_type) };
            if name.as_bytes() == b"." || name.as_bytes() == b".." {
                continue;
            }
            visit(fd, &name)?;
            if kind == libc::DT_DIR || kind == libc::DT_UNKNOWN {
                let child = unsafe { libc::openat(fd, name.as_ptr(), FLAGS) };
                if child < 0 {
                    let error = std::io::Error::last_os_error();
                    if not_folder(&error) {
                        continue;
                    }
                    return Err(error.into());
                }
                walk(child, visit)?;
            }
        }
    }
    let path = std::ffi::CString::new(root.as_os_str().as_bytes())?;
    let fd = unsafe { libc::open(path.as_ptr(), FLAGS) };
    if fd < 0 {
        let error = std::io::Error::last_os_error();
        return if not_folder(&error) { Ok(()) } else { Err(error.into()) };
    }
    walk(fd, visit)
}
/// Resolve API-key inheritance at execution time; never persist or publish the admin key.
pub(crate) fn effective_agent_config(app: &App, uid: i64, runtime: &str, config: &AgentConfig) -> Result<AgentConfig> {
    let mut effective = config.clone();
    if uid == 1 || (!config.api_key.is_empty() && !(runtime == "pi" && config.multi_provider)) {return Ok(effective);}
    let admin = app.store.settings(1)?;
    if let Some(shared) = admin.agents.get(runtime).filter(|shared| !shared.api_key.is_empty()) {
        let endpoint = |cfg: &AgentConfig| {
            let url = if cfg.provider == "local" && cfg.base_url.trim().is_empty() {
                std::env::var("PI_LOCAL_BASE_URL").unwrap_or_default()
            } else {cfg.base_url.clone()};
            url.trim().trim_end_matches('/').to_string()
        };
        if runtime != "pi" || (!shared.multi_provider && shared.provider == config.provider
            && endpoint(shared) == endpoint(config)) {
            effective.api_key = shared.api_key.clone();
            if runtime == "pi" {effective.multi_provider = false;}
        }
    }
    Ok(effective)
}
pub(crate) fn redact_for_user(app: &App, uid: i64, text: &str, settings: &Settings) -> String {
    let text = redact(text, settings);
    if uid != 1 && let Ok(admin) = app.store.settings(1) {redact(&text, &admin)} else {text}
}
fn search_paths(app: &App) -> Vec<std::path::PathBuf> {
    let mut paths = vec![crate::distribution::tools(&app.data)];
    paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_else(|| "/usr/local/bin:/usr/bin:/bin".into())));
    paths
}
/// Isolation, base environment and process group shared by agents and project tools.
/// It adds no agent configuration and no provider credentials.
fn base_command(app: &App, uid: i64, bin: &str, cwd: &Path) -> Result<Command> {
    let home = user_home(app, uid)?;
    let search_path = std::env::join_paths(search_paths(app))?;
    let mut c = if app.isolate {
        let mut c = Command::new("/usr/bin/setpriv");
        c.args([
            "--reuid",
            &(20000 + uid).to_string(),
            "--regid",
            &(20000 + uid).to_string(),
            "--clear-groups",
            "--no-new-privs",
            bin,
        ]);
        c
    } else {
        Command::new(bin)
    };
    c.env_clear()
        .env(
            "PATH",
            search_path,
        )
        .env("HOME", &home)
        .env("USER", format!("nodo-{uid}"))
        .env("LANG", "C.UTF-8")
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("TMPDIR", home.join("tmp"))
        .env("CARGO_HOME", home.join(".cargo"))
        .env(
            "RUSTUP_HOME",
            std::env::var("RUSTUP_HOME").unwrap_or_else(|_| {
                format!("{}/.rustup", std::env::var("HOME").unwrap_or_default())
            }),
        )
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_AUTHOR_NAME", "rightcommit")
        .env("GIT_AUTHOR_EMAIL", "rightcommit@localhost")
        .env("GIT_COMMITTER_NAME", "rightcommit")
        .env("GIT_COMMITTER_EMAIL", "rightcommit@localhost")
        .current_dir(cwd)
        .kill_on_drop(true)
        .process_group(0);
    Ok(c)
}
pub fn command(app: &App, uid: i64, bin: &str, cwd: &Path, settings: &Settings) -> Result<Command> {
    let home = user_home(app, uid)?;
    let mut c = base_command(app, uid, bin, cwd)?;
    c.env("CODEX_HOME", home.join(".codex"))
        .env("CLAUDE_CONFIG_DIR", home.join(".claude"))
        .env("PI_CODING_AGENT_DIR", home.join(".pi/agent"))
        .env("SPECKIT_NON_INTERACTIVE", "1");
    let cfg = effective_agent_config(app, uid, bin, &settings.agents.get(bin).cloned().unwrap_or_default())?;
    if bin == "pi" {crate::pi_providers::validate_auth(&cfg, &home)?;}
    let provider_env = provider_environment_in_home(bin, &cfg, &home, uid);
    ensure!(
        cloud_provider_count(&provider_env) <= 1,
        "Seleziona un solo provider cloud per Claude: AWS, Vertex oppure Foundry"
    );
    if let Some(key) = provider_env.get("ANTHROPIC_API_KEY") {
        ensure!(!key.starts_with("sk-ant-oat"), "Il campo API key richiede una API key Anthropic valida.");
    }
    c.envs(provider_env);
    // Official Spec Kit 1.2.x supports explicit feature-directory resolution.
    // A single living spec is unambiguous even on an isolated nodo/<run> branch.
    let specs = cwd.join("specs");
    if !specs.is_symlink()
        && let Ok(entries) = std::fs::read_dir(&specs)
    {
        let features: Vec<_> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| !p.is_symlink() && p.join("spec.md").is_file())
            .collect();
        if features.len() == 1 {
            c.env("SPECIFY_FEATURE_DIRECTORY", &features[0]);
            if let Some(name) = features[0].file_name() {
                c.env("SPECIFY_FEATURE", name);
            }
        }
    }
    Ok(c)
}
/// Whether this account's Project Manager also sees the environment of the rightcommit server,
/// and its stdio MCP servers may take variables from it by name (`McpConfig::env_from_server`).
/// Server environment configuration belongs to the first administrator; extending the
/// inheritance to every account is a deliberate change of this one condition.
pub(crate) fn manager_inherits_server_environment(uid: i64) -> bool {
    uid == 1
}
/// The server's environment variables, as the administrator's Project Manager inherits them.
pub(crate) fn server_environment() -> Vec<(String, String)> {
    std::env::vars_os()
        .filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)))
        .collect()
}
/// Variables the runtime's own CLI reads to authenticate or to choose where requests go.
/// Only rightcommit's provider rules put them in the environment (see
/// `resolve_provider_environment_for_user`); the server environment never adds or replaces one.
/// Other providers' keys are not managed for Codex and Claude Code. Pi reaches every catalog
/// provider, so all of their variables are Pi's own.
fn provider_managed(runtime: &str, name: &str) -> bool {
    let listed = |lists: &[&[&str]]| lists.iter().any(|list| list.contains(&name));
    let prefixed = |prefixes: &[&str]| prefixes.iter().any(|prefix| name.starts_with(prefix));
    listed(&[NETWORK_ENV])
        || match runtime {
            "claude" => prefixed(&["ANTHROPIC_", "CLAUDE_"]) || listed(&[CLAUDE_MODEL_ENV, AWS_ENV, VERTEX_ENV, FOUNDRY_ENV]),
            // Codex native provider profiles can also use the AWS variables.
            "codex" => prefixed(&["CODEX_", "OPENAI_"]) || (name.starts_with("AWS_") && listed(&[AWS_ENV])),
            "pi" => {
                prefixed(&["PI_", "ANTHROPIC_", "OPENAI_"])
                    || listed(&[HOST_SECRETS])
                    || crate::pi_providers::catalog().iter().any(|provider| provider.env.iter().any(|env| env == name))
            }
            _ => true,
        }
}
/// The variables a command sets or removes, the names a server value must not replace.
fn command_variables(c: &Command) -> Vec<String> {
    c.as_std().get_envs().filter_map(|(name, _)| name.to_str().map(str::to_owned)).collect()
}
/// Whether rightcommit decides this variable for a command with these variables: one of them
/// or its family (GIT_*, XDG_*, CARGO_*, CODEX_*, CLAUDE_*, PI_* ...; LC_* override LANG), or
/// the server's own working directory.
fn owned_by_command(owned: &[String], name: &str) -> bool {
    ["PWD", "OLDPWD"].contains(&name)
        || name.starts_with("LC_")
        || owned.iter().any(|set| set == name || set.find('_').is_some_and(|end| name.starts_with(&set[..=end])))
}
/// Adds the server environment to a Project Manager process without replacing what
/// rightcommit decides: every variable the command already sets or removes and its family
/// (GIT_*, XDG_*, CARGO_*, CODEX_*, CLAUDE_*, PI_* ...; LC_* override LANG), the server's own
/// working directory, and the provider variables of the runtime.
pub(crate) fn inherit_server_environment(c: &mut Command, runtime: &str, server: &[(String, String)]) {
    let owned = command_variables(c);
    for (name, value) in server {
        if !name.is_empty() && !owned_by_command(&owned, name) && !provider_managed(runtime, name) {
            c.env(name, value);
        }
    }
}
/// Whether a stdio MCP server command may not take this variable from the server environment
/// (`McpConfig::env_from_server`). The same rule as `inherit_server_environment`: rightcommit
/// decides every variable the command sets and its family and, when the command is an agent CLI,
/// that runtime's provider variables. Call it before the configuration's own `env` is added.
pub(crate) fn server_variable_reserved(c: &Command, bin: &str, name: &str) -> bool {
    owned_by_command(&command_variables(c), name) || (["claude", "codex", "pi"].contains(&bin) && provider_managed(bin, name))
}
/// Whether a variable name looks like it holds a secret.
pub(crate) fn secret_name(name: &str) -> bool {
    let name = name.to_ascii_uppercase();
    ["KEY", "TOKEN", "SECRET", "PASSWORD", "PASSWD", "CREDENTIAL"].iter().any(|word| name.contains(word))
}
/// Hides values of server variables whose names look secret, in addition to `redact`.
/// Short values are kept: replacing a common short word everywhere would garble the output.
pub(crate) fn redact_server_secrets(text: &str, server: &[(String, String)]) -> String {
    redact_values(text, server.iter().filter(|(name, _)| secret_name(name)).map(|(_, value)| value.as_str()))
}
/// Replaces each value of 8 characters or more with `[redacted]`; shorter values are kept.
pub(crate) fn redact_values<'a>(text: &str, values: impl IntoIterator<Item = &'a str>) -> String {
    let mut values: Vec<&str> = values.into_iter().filter(|value| value.chars().count() >= 8).collect();
    // Longest first: a short value must not break up a longer one that contains it.
    values.sort_by_key(|value| std::cmp::Reverse(value.len()));
    let mut text = text.to_string();
    for value in values {
        text = text.replace(value, "[redacted]");
    }
    text
}
// Only provider variables reach agent processes. HOME/PATH and CLI state stay app-owned.
const CLAUDE_MODEL_ENV: &[&str] = &[
    "ANTHROPIC_MODEL",
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_CUSTOM_HEADERS",
    "ANTHROPIC_BETAS",
    "API_TIMEOUT_MS",
    "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC",
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    "ANTHROPIC_DEFAULT_OPUS_MODEL_NAME",
    "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME",
];
const AWS_ENV: &[&str] = &[
    "CLAUDE_CODE_USE_BEDROCK",
    "CLAUDE_CODE_USE_MANTLE",
    "CLAUDE_CODE_USE_ANTHROPIC_AWS",
    "AWS_REGION",
    "AWS_DEFAULT_REGION",
    "AWS_BEARER_TOKEN_BEDROCK",
    "AWS_ACCESS_KEY_ID",
    "AWS_SECRET_ACCESS_KEY",
    "AWS_SESSION_TOKEN",
    "AWS_PROFILE",
    "AWS_CONFIG_FILE",
    "AWS_SHARED_CREDENTIALS_FILE",
    "AWS_WEB_IDENTITY_TOKEN_FILE",
    "AWS_ROLE_ARN",
    "AWS_ROLE_SESSION_NAME",
    "ANTHROPIC_BEDROCK_BASE_URL",
    "ANTHROPIC_BEDROCK_MANTLE_BASE_URL",
    "ANTHROPIC_BEDROCK_REGION_PREFIX",
    "CLAUDE_CODE_SKIP_BEDROCK_AUTH",
    "ANTHROPIC_AWS_API_KEY",
    "ANTHROPIC_AWS_BASE_URL",
    "ANTHROPIC_AWS_WORKSPACE_ID",
];
const VERTEX_ENV: &[&str] = &[
    "CLAUDE_CODE_USE_VERTEX",
    "CLAUDE_CODE_SKIP_VERTEX_AUTH",
    "ANTHROPIC_VERTEX_PROJECT_ID",
    "ANTHROPIC_VERTEX_BASE_URL",
    "CLOUD_ML_REGION",
    "GOOGLE_APPLICATION_CREDENTIALS",
    "GOOGLE_CLOUD_PROJECT",
    "GCLOUD_PROJECT",
    "GOOGLE_OAUTH_ACCESS_TOKEN",
];
const FOUNDRY_ENV: &[&str] = &[
    "CLAUDE_CODE_USE_FOUNDRY",
    "CLAUDE_CODE_SKIP_FOUNDRY_AUTH",
    "ANTHROPIC_FOUNDRY_API_KEY",
    "ANTHROPIC_FOUNDRY_AUTH_TOKEN",
    "ANTHROPIC_FOUNDRY_RESOURCE",
    "ANTHROPIC_FOUNDRY_BASE_URL",
    "AZURE_CLIENT_ID",
    "AZURE_TENANT_ID",
    "AZURE_CLIENT_SECRET",
    "AZURE_CLIENT_CERTIFICATE_PATH",
    "AZURE_FEDERATED_TOKEN_FILE",
];
const NETWORK_ENV: &[&str] = &[
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "NO_PROXY",
    "NODE_EXTRA_CA_CERTS",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
    "CODEX_CA_CERTIFICATE",
];
const HOST_SECRETS: &[&str] = &[
    "CLAUDE_CODE_OAUTH_TOKEN",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_OAUTH_TOKEN",
    "ANTHROPIC_CUSTOM_HEADERS",
    "OPENAI_API_KEY",
    "CODEX_API_KEY",
    "CODEX_ACCESS_TOKEN",
    "OPENROUTER_API_KEY",
    "XAI_API_KEY",
    "DEEPSEEK_API_KEY",
    "PI_LOCAL_API_KEY",
    "ANTHROPIC_API_KEY",
    "AWS_BEARER_TOKEN_BEDROCK",
    "AWS_ACCESS_KEY_ID",
    "AWS_SECRET_ACCESS_KEY",
    "AWS_SESSION_TOKEN",
    "ANTHROPIC_AWS_API_KEY",
    "ANTHROPIC_FOUNDRY_API_KEY",
    "ANTHROPIC_FOUNDRY_AUTH_TOKEN",
    "AZURE_CLIENT_SECRET",
    "GOOGLE_OAUTH_ACCESS_TOKEN",
];
fn enabled(value: Option<&String>) -> bool {
    value.is_some_and(|v| ["1", "true", "yes", "on"].contains(&v.to_ascii_lowercase().as_str()))
}
pub(crate) fn uses_cloud_provider(env: &std::collections::BTreeMap<String, String>) -> bool {
    cloud_provider_count(env) > 0
}
fn cloud_provider_count(env: &std::collections::BTreeMap<String, String>) -> usize {
    [
        [
            "CLAUDE_CODE_USE_BEDROCK",
            "CLAUDE_CODE_USE_MANTLE",
            "CLAUDE_CODE_USE_ANTHROPIC_AWS",
        ]
        .iter()
        .any(|key| enabled(env.get(*key))),
        enabled(env.get("CLAUDE_CODE_USE_VERTEX")),
        enabled(env.get("CLAUDE_CODE_USE_FOUNDRY")),
    ]
    .into_iter()
    .filter(|active| *active)
    .count()
}
pub(crate) fn resolve_provider_environment_for_user(
    uid: i64,
    runtime: &str,
    cfg: &AgentConfig,
    lookup: impl Fn(&str) -> Option<String>,
) -> std::collections::BTreeMap<String, String> {
    let mut env = std::collections::BTreeMap::new();
    // Server provider configuration belongs to the administrator. Other users can
    // receive only an explicit saved API key (their own or the administrator's).
    let host = |name: &str| {
        if uid != 1 && ["claude", "codex"].contains(&runtime) {None}
        else {lookup(name).filter(|value| !value.is_empty())}
    };
    if ["claude", "codex", "pi"].contains(&runtime) {
        for &name in NETWORK_ENV {
            if let Some(value) = lookup(name).filter(|value| !value.is_empty()) {
                env.insert(name.into(), value);
            }
        }
    }
    if runtime == "pi" {
        env.extend(crate::pi_providers::environment(cfg,host));
        return env;
    }
    // A saved personal API key always selects direct API billing. Global gateways,
    // cloud credentials and subscription tokens must not override it.
    if !cfg.api_key.is_empty() && ["claude", "codex"].contains(&runtime) {
        env.insert(if runtime == "claude" {"ANTHROPIC_API_KEY"} else {"CODEX_API_KEY"}.into(), cfg.api_key.clone());
        if runtime == "claude" {
            for &name in CLAUDE_MODEL_ENV.iter().filter(|name| !["ANTHROPIC_BASE_URL", "ANTHROPIC_CUSTOM_HEADERS"].contains(name)) {
                if let Some(value) = host(name) {env.insert(name.into(), value);}
            }
        }
        return env;
    }
    if runtime == "claude" {
        for &name in CLAUDE_MODEL_ENV {
            if let Some(value) = host(name) {
                env.insert(name.into(), value);
            }
        }
        for (flags, names) in [
            (
                &[
                    "CLAUDE_CODE_USE_BEDROCK",
                    "CLAUDE_CODE_USE_MANTLE",
                    "CLAUDE_CODE_USE_ANTHROPIC_AWS",
                ][..],
                AWS_ENV,
            ),
            (&["CLAUDE_CODE_USE_VERTEX"][..], VERTEX_ENV),
            (&["CLAUDE_CODE_USE_FOUNDRY"][..], FOUNDRY_ENV),
        ] {
            if flags.iter().any(|key| enabled(host(key).as_ref())) {
                for &name in names {
                    if let Some(value) = host(name) {
                        env.insert(name.into(), value);
                    }
                }
            }
        }
        if uses_cloud_provider(&env) {
            // Cloud gateways can supply a bearer token while skipping native cloud auth.
            if [
                "CLAUDE_CODE_SKIP_BEDROCK_AUTH",
                "CLAUDE_CODE_SKIP_VERTEX_AUTH",
                "CLAUDE_CODE_SKIP_FOUNDRY_AUTH",
            ]
            .iter()
            .any(|key| enabled(env.get(*key)))
                && let Some(token) = host("ANTHROPIC_AUTH_TOKEN")
            {
                env.insert("ANTHROPIC_AUTH_TOKEN".into(), token);
            }
            return env;
        }
        // A global bearer/header without a gateway URL may be a personal subscription.
        // Forward it only to the first administrator, never to other users.
        let gateway = env.contains_key("ANTHROPIC_BASE_URL");
        if !gateway && uid != 1 {env.remove("ANTHROPIC_CUSTOM_HEADERS");}
        if env.get("ANTHROPIC_CUSTOM_HEADERS").is_some_and(|headers| has_auth_header(headers)) {return env;}
        if (gateway || uid == 1) && let Some(token) = host("ANTHROPIC_AUTH_TOKEN") {
            env.insert("ANTHROPIC_AUTH_TOKEN".into(), token);
            return env;
        }
        if uid == 1 && let Some(token) = host("CLAUDE_CODE_OAUTH_TOKEN") {
            env.insert("CLAUDE_CODE_OAUTH_TOKEN".into(), token);
            return env;
        }
    }
    if runtime == "codex"
        && let Some(url) = host("OPENAI_BASE_URL")
    {
        env.insert("OPENAI_BASE_URL".into(), url);
    }
    let key_name = match runtime {
        "codex" => "CODEX_API_KEY",
        "claude" => "ANTHROPIC_API_KEY",
        _ => return env,
    };
    let key = if !cfg.api_key.is_empty() {
        Some(cfg.api_key.clone())
    } else {
        host(key_name).or_else(|| {
            if runtime == "codex" {
                host("OPENAI_API_KEY")
            } else {
                None
            }
        })
    };
    if let Some(key) = key {
        env.insert(key_name.into(), key);
    } else if runtime == "codex" && uid == 1
        && let Some(token) = host("CODEX_ACCESS_TOKEN")
    {
        env.insert("CODEX_ACCESS_TOKEN".into(), token);
    }
    env
}
#[cfg(test)]
pub(crate) fn resolve_provider_environment(runtime: &str, cfg: &AgentConfig, lookup: impl Fn(&str) -> Option<String>) -> std::collections::BTreeMap<String, String> {
    resolve_provider_environment_for_user(1, runtime, cfg, lookup)
}
pub(crate) fn provider_environment_in_home(
    runtime: &str,
    cfg: &AgentConfig,
    home: &Path,
    uid: i64,
) -> std::collections::BTreeMap<String, String> {
    let mut env = resolve_provider_environment_for_user(uid, runtime, cfg, |name| std::env::var(name).ok());
    // Codex native provider profiles can use AWS independently of Claude's provider flags.
    if uid == 1 && runtime == "codex" && cfg.api_key.is_empty() && home.join(".codex/config.toml").is_file() {
        for &name in AWS_ENV.iter().filter(|name| name.starts_with("AWS_")) {
            if let Ok(value) = std::env::var(name)
                && !value.is_empty()
            {
                env.insert(name.into(), value);
            }
        }
    }
    env
}
fn has_aws_credentials(env: &std::collections::BTreeMap<String, String>) -> bool {
    env.contains_key("AWS_BEARER_TOKEN_BEDROCK")
        || (env.contains_key("AWS_ACCESS_KEY_ID") && env.contains_key("AWS_SECRET_ACCESS_KEY"))
        || env.contains_key("AWS_PROFILE")
        || env.contains_key("AWS_SHARED_CREDENTIALS_FILE")
        || (env.contains_key("AWS_WEB_IDENTITY_TOKEN_FILE") && env.contains_key("AWS_ROLE_ARN"))
}
pub(crate) fn has_auth_header(headers: &str) -> bool {
    headers.lines().any(|line| {
        line.split_once(':').is_some_and(|(name, value)| {
            ["authorization", "x-api-key", "api-key"]
                .contains(&name.trim().to_ascii_lowercase().as_str())
                && !value.trim().is_empty()
        })
    })
}
pub(crate) fn has_provider_credentials(env: &std::collections::BTreeMap<String, String>) -> bool {
    if cloud_provider_count(env) > 1 {
        return false;
    }
    if enabled(env.get("CLAUDE_CODE_USE_BEDROCK"))
        || enabled(env.get("CLAUDE_CODE_USE_MANTLE"))
        || enabled(env.get("CLAUDE_CODE_USE_ANTHROPIC_AWS"))
    {
        env.contains_key("AWS_BEARER_TOKEN_BEDROCK")
            || env.contains_key("ANTHROPIC_AWS_API_KEY")
            || env.contains_key("ANTHROPIC_AUTH_TOKEN")
            || (env.contains_key("AWS_ACCESS_KEY_ID") && env.contains_key("AWS_SECRET_ACCESS_KEY"))
            || env.contains_key("AWS_PROFILE")
            || env.contains_key("AWS_SHARED_CREDENTIALS_FILE")
            || (env.contains_key("AWS_WEB_IDENTITY_TOKEN_FILE") && env.contains_key("AWS_ROLE_ARN"))
    } else if enabled(env.get("CLAUDE_CODE_USE_VERTEX")) {
        env.contains_key("GOOGLE_APPLICATION_CREDENTIALS")
            || env.contains_key("GOOGLE_OAUTH_ACCESS_TOKEN")
            || env.contains_key("ANTHROPIC_AUTH_TOKEN")
    } else if enabled(env.get("CLAUDE_CODE_USE_FOUNDRY")) {
        env.contains_key("ANTHROPIC_FOUNDRY_API_KEY")
            || env.contains_key("ANTHROPIC_FOUNDRY_AUTH_TOKEN")
            || env.contains_key("ANTHROPIC_AUTH_TOKEN")
            || (env.contains_key("AZURE_CLIENT_ID")
                && env.contains_key("AZURE_TENANT_ID")
                && [
                    "AZURE_CLIENT_SECRET",
                    "AZURE_CLIENT_CERTIFICATE_PATH",
                    "AZURE_FEDERATED_TOKEN_FILE",
                ]
                .iter()
                .any(|key| env.contains_key(*key)))
    } else {
        if env
            .get("ANTHROPIC_CUSTOM_HEADERS")
            .is_some_and(|headers| has_auth_header(headers))
        {
            return true;
        }
        if has_aws_credentials(env) {
            return true;
        }
        [
            "CLAUDE_CODE_OAUTH_TOKEN",
            "ANTHROPIC_AUTH_TOKEN",
            "ANTHROPIC_OAUTH_TOKEN",
            "CODEX_ACCESS_TOKEN",
            "CODEX_API_KEY",
            "ANTHROPIC_API_KEY",
            "OPENAI_API_KEY",
            "OPENROUTER_API_KEY",
            "XAI_API_KEY",
            "DEEPSEEK_API_KEY",
            "PI_LOCAL_API_KEY",
        ]
        .iter()
        .any(|name| env.contains_key(*name))
    }
}
pub fn redact(s: &str, settings: &Settings) -> String {
    let mut t = s.to_string();
    for name in crate::pi_providers::catalog().iter().flat_map(|provider|provider.env.iter()).filter(|name|name.contains("KEY")||name.contains("TOKEN")||name.contains("SECRET")) {
        if let Ok(secret)=std::env::var(name) && !secret.is_empty(){t=t.replace(&secret,"[redacted]");}
    }
    for &name in HOST_SECRETS {
        if let Ok(secret) = std::env::var(name)
            && !secret.is_empty()
        {
            t = t.replace(&secret, "[redacted]");
        }
    }
    if let Ok(headers) = std::env::var("ANTHROPIC_CUSTOM_HEADERS") {
        for line in headers.lines() {
            if let Some((_, value)) = line.split_once(':') {
                let value = value.trim();
                if !value.is_empty() {
                    t = t.replace(value, "[redacted]");
                    if let Some((scheme, token)) = value.split_once(' ')
                        && (scheme.eq_ignore_ascii_case("bearer")
                            || scheme.eq_ignore_ascii_case("basic"))
                        && !token.trim().is_empty()
                    {
                        t = t.replace(token.trim(), "[redacted]");
                    }
                }
            }
        }
    }
    // Longest first: a short key must not break up a longer one that contains it.
    let mut keys: Vec<&String> = settings.agents.values().flat_map(|a| a.secrets()).collect();
    keys.sort_by_key(|key| std::cmp::Reverse(key.len()));
    for key in keys {
        t = t.replace(key.as_str(), "[redacted]");
    }
    for a in settings.connections.values() {
        for secret in [&a.token, &a.password, &a.client_secret] {
            if !secret.is_empty() { t = t.replace(secret, "[redacted]"); }
        }
        if !a.username.is_empty() && !a.token.is_empty() {
            use base64::Engine;
            let encoded = base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", a.username, a.token));
            t = t.replace(&encoded, "[redacted]");
        }
        if !a.username.is_empty() && !a.password.is_empty() {
            use base64::Engine;
            let encoded = base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", a.username, a.password));
            t = t.replace(&encoded, "[redacted]");
        }
    }
    for c in settings.mcp_servers.values() {
        for secret in [
            &c.auth.token,
            &c.auth.password,
            &c.auth.client_secret,
            &c.auth.access_token,
            &c.auth.refresh_token,
        ]
        .into_iter()
        .chain(c.env.values())
        .chain(c.headers.values())
        {
            if !secret.is_empty() {
                t = t.replace(secret, "[redacted]");
            }
        }
    }
    t
}
pub async fn tool(
    app: &App,
    uid: i64,
    cwd: &Path,
    bin: &str,
    args: &[&str],
    timeout: u64,
) -> Result<String> {
    let mut s = app.store.settings(uid)?;
    let config = effective_agent_config(app, uid, bin, &s.agents.get(bin).cloned().unwrap_or_default())?;
    s.agents.insert(bin.into(), config);
    let mut c = command(app, uid, bin, cwd, &s)?;
    c.args(args);
    let child = c.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    finish(app, uid, bin, child, timeout, &s).await
}
/// Git with raw arguments and raw standard output, for paths that are not valid UTF-8
/// or that the text cleanup of `tool` could alter. Errors carry the redacted stderr.
pub async fn git_bytes(app: &App, uid: i64, cwd: &Path, args: &[&std::ffi::OsStr], timeout: u64) -> Result<Vec<u8>> {
    program_bytes(app, uid, "git", cwd, args, timeout).await
}
/// Like `git_bytes` for another program from the search path, run as the user: for a file the
/// server must change in a folder the user owns without writing it as itself.
pub(crate) async fn program_bytes(app: &App, uid: i64, program: &str, cwd: &Path, args: &[&std::ffi::OsStr], timeout: u64) -> Result<Vec<u8>> {
    let s = app.store.settings(uid)?;
    let mut c = base_command(app, uid, program, cwd)?;
    c.args(args);
    let child = c.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    let _group = ProcessGroup(child.id().context("PID non disponibile")?);
    let out = tokio::time::timeout(std::time::Duration::from_secs(timeout), child.wait_with_output())
        .await
        .context("Tempo massimo comando superato")??;
    ensure!(
        out.status.success(),
        "{program}: {}",
        redact_for_user(app, uid, &String::from_utf8_lossy(&out.stderr), &s).chars().take(4000).collect::<String>()
    );
    Ok(out.stdout)
}
/// Like `git_bytes`, with extra environment and at most `cap` bytes of output: a preview
/// of a large diff stops reading and ends git instead of holding all of it in memory.
/// Returns the output and whether it was cut.
pub(crate) async fn git_with(app: &App, uid: i64, cwd: &Path, args: &[&std::ffi::OsStr], env: &[(&str, &std::ffi::OsStr)], timeout: u64, cap: usize) -> Result<(Vec<u8>, bool)> {
    use tokio::io::AsyncReadExt;
    let s = app.store.settings(uid)?;
    let mut c = base_command(app, uid, "git", cwd)?;
    c.args(args).envs(env.iter().copied());
    let mut child = c.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    let _group = ProcessGroup(child.id().context("PID non disponibile")?);
    let (mut stdout, mut stderr) = (child.stdout.take().context("Output git non disponibile")?, child.stderr.take().context("Output git non disponibile")?);
    let run = async {
        let errors = tokio::spawn(async move {
            let mut text = vec![];
            let _ = stderr.read_to_end(&mut text).await;
            text
        });
        let mut out = vec![];
        (&mut stdout).take(u64::try_from(cap).unwrap_or(u64::MAX).saturating_add(1)).read_to_end(&mut out).await?;
        let cut = out.len() > cap;
        if cut {
            out.truncate(cap);
            let _ = child.start_kill();
        }
        let status = child.wait().await?;
        let errors = errors.await.unwrap_or_default();
        ensure!(
            cut || status.success(),
            "git: {}",
            redact_for_user(app, uid, &String::from_utf8_lossy(&errors), &s).chars().take(4000).collect::<String>()
        );
        anyhow::Ok((out, cut))
    };
    tokio::time::timeout(std::time::Duration::from_secs(timeout), run).await.context("Tempo massimo comando superato")?
}
/// Run a project command, such as the local test step, as the run user. It gets only the
/// base environment: no agent configuration and no provider credentials.
pub async fn project_tool(app: &App, uid: i64, cwd: &Path, argv: &[String], timeout: u64) -> Result<String> {
    let (program, args) = argv.split_first().context("Comando vuoto")?;
    ensure!(
        program_available(program, cwd, &search_paths(app)),
        "{program} non è installato sul server: aggiungilo all'immagine o all'ambiente, \
         oppure imposta un altro comando nel nodo"
    );
    let s = app.store.settings(uid)?;
    let mut c = base_command(app, uid, program, cwd)?;
    c.args(args);
    let child = c.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            anyhow::anyhow!("{program} non è installato sul server")
        } else {
            anyhow::Error::from(e).context(format!("Avvio di {program} non riuscito"))
        }
    })?;
    finish(app, uid, program, child, timeout, &s).await
}
/// A program name is looked up in PATH like the process will; a path is resolved from cwd.
pub(crate) fn program_available(program: &str, cwd: &Path, paths: &[std::path::PathBuf]) -> bool {
    use std::os::unix::fs::PermissionsExt;
    let executable = |path: &Path| std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0);
    if program.contains('/') {
        executable(&cwd.join(program))
    } else {
        paths.iter().any(|dir| executable(&dir.join(program)))
    }
}
async fn finish(app: &App, uid: i64, label: &str, child: tokio::process::Child, timeout: u64, s: &Settings) -> Result<String> {
    let _group = ProcessGroup(child.id().context("PID non disponibile")?);
    let out = tokio::time::timeout(
        std::time::Duration::from_secs(timeout),
        child.wait_with_output(),
    )
    .await
    .context("Tempo massimo comando superato")??;
    let text = redact_for_user(app, uid,
        &format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
        s,
    );
    ensure!(
        out.status.success(),
        "{label}: {}",
        text.chars().take(4000).collect::<String>()
    );
    Ok(text)
}
/// Spec Kit commands, which Task nodes may name without the `speckit-` prefix.
const SPECKIT_COMMANDS: &[&str] = &[
    "analyze", "checklist", "clarify", "constitution", "converge", "implement", "plan", "specify",
    "tasks", "taskstoissues",
];

/// The invocation of the skill or command named in a Task, for the Agent's runtime.
/// Any skill installed in the workspace works (Spec Kit, OpenSpec or the project's own);
/// Spec Kit commands also accept their short names, such as `plan` or `/speckit.plan`.
pub fn resolve_command(command: &str, runtime: &str, workspace: &Path) -> Result<String> {
    let command = command.trim();
    if command.is_empty() {
        return Ok(String::new());
    }
    let name = command.trim_start_matches(['/', '$']);
    let name = name.strip_prefix("skill:").unwrap_or(name);
    // `:` separates the folders of a Claude Code command, as in `/opsx:propose`.
    ensure!(
        name.len() <= 128
            && name.split(':').all(|part| {
                part.starts_with(|c: char| c.is_ascii_alphanumeric())
                    && part.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
            }),
        "Nome skill non valido: {name}. Usa lettere, numeri, -, _, . e :"
    );
    let speckit = name.strip_prefix("speckit.").or_else(|| name.strip_prefix("speckit-"));
    // Spec Kit short names keep meaning Spec Kit when the project also has a skill with that name.
    let names = match speckit {
        Some(short) => vec![format!("speckit-{short}")],
        None if SPECKIT_COMMANDS.contains(&name) => vec![format!("speckit-{name}"), name.to_string()],
        None => vec![name.to_string(), format!("speckit-{name}")],
    };
    let label = match runtime {
        "claude" => "Claude Code",
        "codex" => "Codex",
        "pi" => "Pi",
        _ => bail!("Runtime non supportato"),
    };
    let mut searched = vec![];
    for name in &names {
        let prompt = match name.strip_prefix("speckit-") {
            Some(short) => format!("speckit.{short}"),
            None => name.clone(),
        };
        let candidates = match runtime {
            "claude" => vec![
                (format!(".claude/skills/{name}/SKILL.md"), format!("/{name}")),
                (format!(".claude/commands/{}.md", name.replace(':', "/")), format!("/{name}")),
            ],
            "codex" => vec![
                (format!(".agents/skills/{name}/SKILL.md"), format!("${name}")),
                (format!(".codex/skills/{name}/SKILL.md"), format!("${name}")),
            ],
            _ => vec![
                (format!(".pi/prompts/{prompt}.md"), format!("/{prompt}")),
                (format!(".pi/skills/{name}/SKILL.md"), format!("/skill:{name}")),
                (format!(".agents/skills/{name}/SKILL.md"), format!("/skill:{name}")),
            ],
        };
        for (file, invocation) in candidates {
            if workspace.join(&file).is_file() {
                return Ok(invocation);
            }
            searched.push(file);
        }
    }
    let short = speckit.unwrap_or(name);
    if speckit.is_some() || SPECKIT_COMMANDS.contains(&short) {
        bail!(
            "Skill Spec Kit {short} assente per {label} nel workspace. Attiva «Aggiungi Spec Kit se manca» nel nodo Source oppure aggiungi Spec Kit al repository."
        );
    }
    bail!(
        "Skill {name} assente per {label} nel workspace (cercata in {}). Aggiungi la skill al repository oppure imposta un comando di preparazione nel nodo Source.",
        searched.join(", ")
    )
}

pub(crate) const CONTRACT: &str = r#"
rightcommit orchestration contract (highest priority for the final answer format only):
Work in the repository. When the task names an installed skill or command (Spec Kit, OpenSpec or another framework), actually read and execute its instructions; never claim an action or test without doing it. Keep output concise. This is a small library, keep the implementation small.
All turns have read, write, edit and shell tools in this execution workspace, including Analyze, Review and Gate. Restrictions stated in earlier turns no longer apply. Perform the required actions with the tools available now; do not report a read-only limitation from memory.
If a skill requires an unanswered human decision, do not invent a choice or wait on terminal stdin. End this turn with status needs_input and explicit questions. The human response will arrive in the SAME native conversation.
At the END of your final response emit exactly one JSON object between <nodo_result> and </nodo_result>:
<nodo_result>{"status":"completed|needs_input|blocked","summary":"the result itself in short form","details":"the long form, or empty","questions":[],"artifacts":["relative/path"],"decision":""}</nodo_result>
summary is what people see first and what the next steps receive first, so it must contain the result itself in short form: the answer, the decision and its reason, the key points of the plan, the changes made and the tests actually run. Never describe what you produced instead of giving it: "Plan provided", "One-line plan provided" or "See the answer above" are wrong. details holds the long form (the full answer, report or evidence) and may be left empty when summary says everything; text before <nodo_result> is used as details when details is empty. An instruction such as "only the output", "nothing else" or "no explanations" limits the content of summary and details; it never removes the <nodo_result> block.
Do not include hidden reasoning. Native session continuity is managed externally; never launch a nested coding agent. Do not read credential files. Do not push, merge, deploy, or contact a ticket system: Connection nodes perform those actions separately.
"#;

/// Native web search needs the provider's own search tool; OpenRouter has none, so Pi gets no web_search there.
fn pi_web_search(cfg: &AgentConfig) -> bool {
    ["", "anthropic", "openai"].contains(&cfg.provider.as_str())
}

/// Pi packages shipped for every user: the extensions folder itself and each dependency in its
/// package.json that declares Pi resources. Pi loads a package directory like `pi install` does,
/// so extensions, skills and prompt templates all come from the package's own manifest.
fn pi_packages(extensions: &Path, cfg: &AgentConfig) -> Vec<PathBuf> {
    let manifest = |dir: &Path| {
        std::fs::read_to_string(dir.join("package.json"))
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
    };
    let is_pi_package = |m: &Value| {
        m.get("pi").is_some()
            || m["keywords"]
                .as_array()
                .is_some_and(|k| k.iter().any(|k| k == "pi-package"))
    };
    let Some(root) = manifest(extensions) else {
        return vec![];
    };
    let mut packages = vec![];
    if is_pi_package(&root) {
        packages.push(extensions.to_path_buf());
    }
    let dependencies = root["dependencies"].as_object().into_iter().flatten();
    for name in dependencies.map(|(name, _)| name) {
        if name == "pi-web-search" && !pi_web_search(cfg) {
            continue;
        }
        let dir = extensions.join("node_modules").join(name);
        match manifest(&dir) {
            Some(m) if is_pi_package(&m) => packages.push(dir),
            Some(_) => {} // A library used by an extension, not a Pi package.
            None => {
                tracing::warn!(package = %name, "Pi package not installed: run npm ci in pi-extensions")
            }
        }
    }
    packages
}

/// Public failure messages never include provider payloads, partial keys or request URLs.
pub(crate) fn public_agent_failure(detail: &str) -> &'static str {
    let text = detail.to_ascii_lowercase();
    if detail == LOCAL_TURN_TIMEOUT {
        "Il turno ha raggiunto il limite di tempo configurato in rightcommit."
    } else if detail == MISSING_STRUCTURED_RESULT {
        MISSING_STRUCTURED_RESULT
    } else if text.contains("safeguards flagged") || text.contains("usage policy") || text.contains("cyber verification") || text.contains("refusal") {
        "Il modello ha rifiutato la richiesta per le regole di sicurezza del provider, non per un errore di configurazione. Succede per esempio con strumenti di penetration test o exploit. Riformula la richiesta o scegli un altro modello."
    } else if text.contains("401") || text.contains("incorrect api key") || text.contains("invalid api key") || text.contains("unauthorized") || text.contains("authentication") {
        "Il provider ha rifiutato le credenziali. Verifica la chiave API e il provider in Configurazioni personali → Agenti, oppure le credenziali in .env."
    } else if text.contains("insufficient_quota") || text.contains("billing") || text.contains("credit balance") || text.contains("quota exceeded") {
        "Il credito o la quota del provider è esaurito. Verifica il piano e i limiti del tuo account prima di riprovare."
    } else if text.contains("429") || text.contains("rate limit") || text.contains("too many requests") {
        "Il provider sta limitando le richieste. Attendi qualche istante e riprova."
    } else if text.contains("403") || text.contains("forbidden") || text.contains("permission denied") {
        "Il provider non consente questa richiesta. Verifica i permessi dell’account e l’accesso al modello selezionato."
    } else if text.contains("404") || text.contains("model_not_found") || text.contains("model not found") || text.contains("unknown model") {
        "Il modello o l’endpoint non è disponibile. Verifica provider, modello e URL in Configurazioni personali → Agenti."
    } else if is_timeout(&text) {
        "Una richiesta dell’agente non ha ricevuto risposta in tempo."
    } else if text.contains("connection") || text.contains("connect") || text.contains("dns") || text.contains("network") || text.contains("fetch failed") {
        "Impossibile raggiungere il provider. Verifica la connessione e l’URL configurato, poi riprova."
    } else if text.contains("500") || text.contains("502") || text.contains("503") || text.contains("overloaded") {
        "Il provider è temporaneamente non disponibile. Riprova tra qualche istante."
    } else if text.contains("conflicts with") {
        "Un pacchetto Pi registra uno strumento già presente. Rimuovi il pacchetto in conflitto e riprova."
    } else {
        "L’agente non ha completato la risposta. Verifica la configurazione in Configurazioni personali → Agenti e riprova."
    }
}

const LOCAL_TURN_TIMEOUT: &str = "Turno interrotto o scaduto; la sessione salvata resta disponibile per un riesame manuale";
const MISSING_STRUCTURED_RESULT: &str = "Risposta agente priva del risultato strutturato rightcommit. Riprendi nella stessa sessione con Retry.";
/// The one follow-up turn asked when an answer ends without its structured result.
pub(crate) const STRUCTURED_RESULT_FOLLOW_UP: &str = "Your previous answer in this conversation ended without the required <nodo_result> block. Do not do any new work, do not use tools and do not change or repeat that answer: rightcommit keeps the text you already wrote. Reply now with only the <nodo_result> block for the answer you just gave, following the format below, with every structured field that answer needs (status, decision, questions, artifacts and, where the format asks for it, details). Its summary states the result itself in short form, not a description of it.";

/// An answer that ended without a readable structured result. Shown as `MISSING_STRUCTURED_RESULT`;
/// it carries the redacted text of the answer for the single follow-up turn.
#[derive(Debug)]
struct MissingStructuredResult(String);

impl std::fmt::Display for MissingStructuredResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(MISSING_STRUCTURED_RESULT)
    }
}

impl std::error::Error for MissingStructuredResult {}

fn is_timeout(text: &str) -> bool {
    text.contains("timeout") || text.contains("timed out")
}

#[derive(Debug)]
struct RetryablePiTimeout;

impl std::fmt::Display for RetryablePiTimeout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Pi request timed out")
    }
}

impl std::error::Error for RetryablePiTimeout {}

#[derive(Default)]
struct PiTurnStatus {
    assistant_error: Option<String>,
    timed_out: bool,
    used_tools: bool,
    retried: bool,
}

impl PiTurnStatus {
    fn observe(&mut self, kind: &str, event: &Value) {
        match kind {
            "message_end" if event["message"]["role"] == "assistant" => {
                let message = &event["message"];
                let stop = message["stopReason"].as_str().unwrap_or_default();
                let error = message["errorMessage"].as_str().unwrap_or_default();
                self.assistant_error = matches!(stop, "error" | "aborted").then(|| {
                    if error.is_empty() { message.to_string() } else { error.to_owned() }
                });
                self.timed_out = stop == "error" && is_timeout(&error.to_ascii_lowercase());
                self.used_tools |= stop == "toolUse";
            }
            "tool_execution_start" | "tool_execution_update" | "tool_execution_end" => {
                self.used_tools = true;
            }
            "auto_retry_start" | "auto_retry_end" => self.retried = true,
            _ => {}
        }
    }

    fn can_retry(&self) -> bool {
        self.timed_out && !self.used_tools && !self.retried
    }
}

/// `codex exec --json` writes `error` both for a stream retry ("Reconnecting... 1/5") and for a
/// fatal error, and does not say which: a fatal error also ends the turn with `turn.failed` and a
/// non-zero exit, a retry that works is followed by `turn.completed` and exit 0. So an `error`
/// fails the turn unless a `turn.completed` follows it and Codex exits successfully.
#[derive(Default)]
struct CodexTurnStatus {
    error: Option<String>,
    completed_after_error: bool,
}

impl CodexTurnStatus {
    fn observe(&mut self, kind: &str, event: &Value) {
        match kind {
            "error" => {
                self.error = Some(event.to_string());
                self.completed_after_error = false;
            }
            "turn.completed" => self.completed_after_error = true,
            _ => {}
        }
    }

    /// The error that fails the turn, if any; `turn.failed` is reported on its own.
    fn failure(self, exited_successfully: bool) -> Option<String> {
        self.error.filter(|_| !(self.completed_after_error && exited_successfully))
    }
}

/// Pi refuses to start when two packages register the same tool, which fails every turn.
pub(crate) fn explain_agent_failure(runtime: &str, detail: String) -> String {
    if runtime == "pi" && detail.contains("conflicts with") {
        format!(
            "Un pacchetto Pi installato registra uno strumento già fornito da rightcommit, quindi Pi non si avvia. Rimuovilo con `pi remove` (vedi ADD-PI-PACKAGES.md).\n\n{detail}"
        )
    } else {
        detail
    }
}

/// rightcommit's own instructions are English; the answer follows the people's language.
pub(crate) const LANGUAGE: &str = "Language: write your answer, summary, details and questions in the language of the human-written content of this turn (the user's message, or the step's own instructions and the run request), not in the language of rightcommit's instructions around it, which are always English. When those human-written parts use different languages, follow the step's own instructions. Keep machine-read values exactly as specified: status, decision and outcome names, JSON keys, IDs, tags and paths.";

pub(crate) fn pi_arguments(
    session: &NativeSession,
    cfg: &AgentConfig,
    session_dir: &Path,
    extensions: &Path,
) -> Vec<String> {
    let mut tools = vec![
        "read",
        "bash",
        "edit",
        "write",
        "grep",
        "find",
        "ls",
        "web_fetch",
        "todo",
    ];
    if pi_web_search(cfg) {
        tools.push("web_search");
    }
    if cfg.codemode {
        tools.push("codemode");
    }
    // Pi restores active tools from its session. `+name` entries force the app's tools on,
    // including on resume, while keeping Pi's default selection and the tools of extensions
    // the user installed. Plain names, as for supervisors, replace the whole selection.
    let tools = tools
        .iter()
        .map(|t| format!("+{t}"))
        .collect::<Vec<_>>()
        .join(",");
    let mut args = vec![
        "--mode".into(),
        "json".into(),
        "--print".into(),
        "--session-id".into(),
        session.native_id.clone(),
        "--session-dir".into(),
        session_dir.to_string_lossy().into_owned(),
        "--approve".into(),
        "--no-mcp".into(),
        "--tools".into(),
        tools,
    ];
    for path in pi_packages(extensions, cfg) {
        args.extend(["--extension".into(), path.to_string_lossy().into_owned()]);
    }
    if !session.model.is_empty() {
        args.extend(["--model".into(), session.model.clone()]);
    }
    if crate::pi_providers::known(&cfg.provider) {
        args.extend(["--provider".into(), cfg.provider.clone()]);
    }
    if ["off", "minimal", "low", "medium", "high", "xhigh", "max"].contains(&cfg.effort.as_str()) {
        args.extend(["--thinking".into(), cfg.effort.clone()]);
    }
    args
}

#[derive(Clone, Copy)]
pub struct Turn<'a> {
    pub binding: &'a str,
    pub runtime: &'a str,
    pub model_override: &'a str,
    pub provider_override: &'a str,
    pub base_url_override: &'a str,
    pub prompt: &'a str,
}
/// A login file can be created empty by Pi; require credentials for the selected provider.
pub(crate) fn pi_has_login(home: &Path, provider: &str) -> bool {
    let path=home.join(".pi/agent/auth.json");
    if path.is_symlink() { return false; }
    let Some(data)=std::fs::read(path).ok().and_then(|bytes|serde_json::from_slice::<Value>(&bytes).ok()) else { return false; };
    let value=&data[provider];
    let nonempty=|field: &str|value[field].as_str().is_some_and(|v|!v.trim().is_empty());
    match value["type"].as_str() {
        Some("api_key") => nonempty("key") || value["env"].as_object().is_some_and(|env|env.values().any(|v|v.as_str().is_some_and(|s|!s.trim().is_empty()))),
        Some("oauth") => nonempty("access") && nonempty("refresh") && value["expires"].is_number(),
        _ => false,
    }
}
pub(crate) fn validate_pi_model(runtime: &str, model: &str) -> Result<()> {
    ensure!(runtime != "pi" || !model.trim().is_empty(), "Pi richiede un modello esplicito. Configuralo in Configurazioni personali → Agenti → Pi Agent, oppure nel nodo Agent del workflow.");
    Ok(())
}
pub async fn turn(app: &App, uid: i64, run: &mut Run, request: Turn<'_>) -> Result<AgentResult> {
    let mut result = turn_once(app, uid, run, request).await;
    if result.as_ref().is_err_and(|error| error.is::<RetryablePiTimeout>()) {
        result = turn_once(app, uid, run, request).await;
    }
    // An answer without its structured result gets ONE follow-up turn in the same native session,
    // asking only for the block. The follow-up is never followed up: if it fails too, the first
    // error stands. Attempts and rework counts are the caller's and do not change.
    let answer = match &result {
        Err(error) => match error.downcast_ref::<MissingStructuredResult>() {
            Some(missing) => missing.0.split("<nodo_result>").next().unwrap_or_default().trim().to_owned(),
            None => return result,
        },
        Ok(_) => return result,
    };
    emit(app, uid, &run.id, "activity", &run.current, &serde_json::json!({"runtime":request.runtime,"phase":"result_follow_up"}).to_string());
    let Ok(mut out) = turn_once(app, uid, run, Turn { prompt: STRUCTURED_RESULT_FOLLOW_UP, ..request }).await else {
        return result;
    };
    Ok(if answer.is_empty() {
        out
    } else if request.binding == crate::manager::BINDING {
        // The manager's reply is its summary: show the text the user asked for.
        out.summary = answer;
        out
    } else {
        if out.details.trim().is_empty() {
            let mut chars = answer.chars();
            out.details = chars.by_ref().take(16_000).collect();
            out.details_truncated = chars.next().is_some();
        }
        out
    })
}

/// `-c` overrides that give Codex the run's MCP bridge. Turns run with approval policy `never`, so
/// the bridge's tools, which carry no annotations, must be approved by the server entry itself:
/// otherwise a sandboxed turn refuses every call ("MCP tool call requires approval, but approval
/// policy is never"); only danger-full-access approves them. Codex knows the key since 0.122.0 and
/// ignores unknown server keys, so it is always passed.
fn codex_mcp_overrides(exe: &str) -> Vec<String> {
    [
        ("command", serde_json::Value::from(exe).to_string()),
        ("args", "[\"mcp-bridge\"]".into()),
        ("env_vars", "[\"NODO_MCP_TOKEN\",\"NODO_MCP_ENDPOINT\"]".into()),
        ("startup_timeout_sec", "30".into()),
        ("tool_timeout_sec", "330".into()),
        ("default_tools_approval_mode", "\"approve\"".into()),
    ]
    .into_iter()
    .flat_map(|(key, value)| ["-c".to_string(), format!("mcp_servers.nodo.{key}={value}")])
    .collect()
}

async fn turn_once(app: &App, uid: i64, run: &mut Run, request: Turn<'_>) -> Result<AgentResult> {
    crate::auth::ensure_active(app, uid)?;
    let Turn {
        binding,
        runtime,
        model_override,
        provider_override,
        base_url_override,
        prompt,
    } = request;
    let mut settings = app.store.settings(uid)?;
    let cfg = if runtime=="pi" {
        let model=run.sessions.get(binding).map(|session|session.model.as_str()).unwrap_or(model_override);
        let config=crate::pi_providers::effective(&settings,provider_override,model,base_url_override)?;
        if config.multi_provider {crate::pi_providers::preflight(app,uid,&settings,provider_override,model,base_url_override)?;}
        settings.agents.insert("pi".into(),config.clone());config
    } else {settings.agents.get(runtime).cloned().unwrap_or_default()};
    let cfg = effective_agent_config(app, uid, runtime, &cfg)?;
    let mut session = run.sessions.get(binding).cloned().unwrap_or(NativeSession {
        provider: String::new(), base_url: String::new(),
runtime: runtime.into(),
        native_id: String::new(),
        turns: 0,
        model: if model_override.is_empty() {
            cfg.model.clone()
        } else {
            model_override.into()
        },
    });
    ensure!(
        session.runtime == runtime,
        "Il runtime del binding è cambiato: usa un nodo Memory e un nuovo binding"
    );
    if runtime=="pi" {
        ensure!(session.provider.is_empty() || session.provider==cfg.provider,"Il provider del binding Pi è cambiato: usa un nuovo binding Agent.");
        ensure!(session.base_url.is_empty() || session.base_url==cfg.base_url,"L’endpoint del binding Pi è cambiato: usa un nuovo binding Agent.");
        session.provider=cfg.provider.clone();session.base_url=cfg.base_url.clone();
    }
    validate_pi_model(runtime, &session.model)?;
    let workspace = PathBuf::from(&run.workspace);
    let fresh = session.native_id.is_empty();
    let managing = binding == crate::manager::BINDING;
    let template_assistant = binding == crate::templates::BINDING;
    let supervising =
        binding == crate::supervision::BINDING || binding == crate::templates::BINDING;
    let mcp = if supervising || managing {
        None
    } else {
        crate::mcp_client::prepare(app, uid, run).await?
    };
    let mut settings = app.store.settings(uid)?; // OAuth may have refreshed during MCP preparation.
    settings.agents.insert(runtime.into(),cfg.clone());
    let read_only = supervising || (managing && !settings.manager_write_enabled);
    // The administrator's Project Manager also sees the server environment; workflow runs never do.
    let inherited = if managing && manager_inherits_server_environment(uid) {server_environment()} else {Vec::new()};
    let redact_turn = |value: &str| {
        let text = redact_server_secrets(&redact_for_user(app, uid, value, &settings), &inherited);
        if let Some(mcp) = &mcp {
            // Server variables passed to the run's MCP servers never reach run evidence.
            redact_values(&text, mcp.server_values()).replace(mcp.token(), "[redacted]")
        } else {
            text
        }
    };
    let mut cmd = command(app, uid, runtime, &workspace, &settings)?;
    if let Some(mcp) = &mcp {
        cmd.env("NODO_MCP_TOKEN", mcp.token())
            .env("NODO_MCP_ENDPOINT", &mcp.endpoint);
    }
    crate::compaction::configure(app, uid, runtime, &settings, &mut cmd)?;
    if runtime == "pi" {crate::pi_local::configure(app,uid,&settings,&session.model,&mut cmd)?;}
    inherit_server_environment(&mut cmd, runtime, &inherited);
    match runtime {
        "codex" => {
            cmd.arg("--no-daemon");
            cmd.args([
                "-a",
                "never",
                "--sandbox",
                if read_only {
                    "read-only"
                } else if app.isolate {
                    "danger-full-access"
                } else {
                    "workspace-write"
                },
                "exec",
            ]);
            if !fresh {
                cmd.args(["resume", &session.native_id]);
            }
            cmd.args(["--json", "--skip-git-repo-check"]);
            if managing && !read_only && !app.isolate {
                let home = app.data.join("users").join(uid.to_string()).join("manager");
                cmd.args(["-c", &format!(
                    "sandbox_workspace_write.writable_roots={}",
                    serde_json::to_string(&[home.join("mcp-servers"), home.join("projects")])?
                )]);
            }
            if !session.model.is_empty() {
                cmd.args(["--model", &session.model]);
            }
            if ["minimal", "low", "medium", "high", "xhigh"].contains(&cfg.effort.as_str()) {
                cmd.args(["-c", &format!("model_reasoning_effort={:?}", cfg.effort)]);
            }
            if mcp.is_some() {
                cmd.args(codex_mcp_overrides(&std::env::current_exe()?.to_string_lossy()));
            }
            cmd.arg("-");
        }
        "claude" => {
            if read_only {
                cmd.args([
                    "--tools",
                    if template_assistant {
                        ""
                    } else {
                        "Read,Glob,Grep"
                    },
                ]);
            }
            cmd.args([
                "--print",
                "--verbose",
                "--output-format",
                "stream-json",
                "--strict-mcp-config",
                "--setting-sources",
                "project",
                "--permission-mode",
                "acceptEdits",
                "--allowedTools",
                if read_only { "Read,Glob,Grep" } else { "Read,Write,Edit,Glob,Grep,Bash,NotebookEdit,WebFetch,WebSearch" },
            ]);
            if let Some(mcp) = &mcp {
                let config = serde_json::json!({"mcpServers":{"nodo":{"command":std::env::current_exe()?,"args":["mcp-bridge"],"env":{"NODO_MCP_TOKEN":mcp.token(),"NODO_MCP_ENDPOINT":mcp.endpoint}}}});
                let path = mcp.dir.join("claude-mcp.json");
                std::fs::write(&path, serde_json::to_vec(&config)?)?;
                private(&path, false)?;
                own(app, uid, &path)?;
                cmd.arg("--mcp-config").arg(path).args([
                    "--allowedTools",
                    "Read,Write,Edit,Glob,Grep,Bash,NotebookEdit,WebFetch,WebSearch,mcp__nodo__*",
                ]);
            }
            if fresh {
                session.native_id = id();
                cmd.args(["--session-id", &session.native_id]);
            } else {
                cmd.args(["--resume", &session.native_id]);
            }
            if !session.model.is_empty() {
                cmd.args(["--model", &session.model]);
            }
            if ["low", "medium", "high", "xhigh", "max"].contains(&cfg.effort.as_str()) {
                cmd.args(["--effort", &cfg.effort]);
            }
        }
        "pi" => {
            if fresh {
                session.native_id = id();
            }
            let mut args = pi_arguments(
                &session,
                &cfg,
                &user_home(app, uid)?.join(".pi/agent/sessions"),
                &app.assets.join("pi-extensions"),
            );
            if read_only && let Some(i) = args.iter().position(|a| a == "--tools") {
                args[i + 1] = if template_assistant {
                    ""
                } else {
                    "read,grep,find,ls"
                }
                .into();
            }
            if let Some(mcp) = &mcp {
                let path = mcp.dir.join("pi-mcp.mjs");
                let definitions = serde_json::to_string(&mcp.tools)?;
                let extension = format!(
                    r#"export default function(pi) {{
                    for (const t of {definitions}) pi.registerTool({{
                        name:t.name,label:t.title||t.name,description:t.description||t.name,
                        parameters:t.inputSchema,executionMode:'sequential',
                        async execute(id,params,signal) {{
                            const res=await fetch(process.env.NODO_MCP_ENDPOINT,{{method:'POST',signal,headers:{{'Content-Type':'application/json','X-Nodo-Request':'1','Authorization':'Bearer '+process.env.NODO_MCP_TOKEN}},body:JSON.stringify({{method:'tools/call',params:{{name:t.name,arguments:params}}}})}});
                            if(!res.ok)throw Error('MCP call failed; inspect the run log before retrying.');
                            const r=await res.json();
                            if(r.isError)throw Error(JSON.stringify(r.content));
                            return {{content:(r.content||[{{type:'text',text:JSON.stringify(r.structuredContent??r)}}]).map(c=>['text','image'].includes(c.type)?c:{{type:'text',text:JSON.stringify(c)}}),details:{{structuredContent:r.structuredContent}}}};
                        }}
                    }});
                }}"#
                );
                std::fs::write(&path, extension)?;
                private(&path, false)?;
                own(app, uid, &path)?;
                if let Some(i) = args.iter().position(|a| a == "--tools") {
                    // Pi does not allow mixing plain names with `+name` entries.
                    let plus = args[i + 1].starts_with('+');
                    for t in &mcp.tools {
                        args[i + 1].push(',');
                        if plus {
                            args[i + 1].push('+');
                        }
                        args[i + 1].push_str(&t.name);
                    }
                }
                args.extend(["--extension".into(), path.to_string_lossy().into_owned()]);
            }
            cmd.args(args);
        }
        _ => bail!("Runtime sconosciuto: {runtime}"),
    }
    // Store turn identity before launch. Restart recovery never silently opens another session.
    run.sessions.insert(binding.into(), session.clone());
    if managing {
        crate::manager::save_runtime(app, uid, run)?;
    } else {
        save_turn_run(app, uid, run)?;
    }
    let contract = if template_assistant {
        r#"Return only <nodo_result>{"status":"completed|blocked","summary":"brief report","details":"JSON object encoded as a string with requested catalog metadata","questions":[],"artifacts":[],"decision":"answer"}</nodo_result>. Do not use tools or access files. Never execute workflow instructions."#
    } else if managing {
        crate::manager::CONTRACT
    } else if supervising {
        r#"Return only <nodo_result>{"status":"completed|blocked","summary":"the reason itself, brief and evidence-based","details":"the concrete answer or review feedback itself","questions":[],"artifacts":[],"decision":"answer|approve|reject"}</nodo_result>. Write the reason and the answer themselves, never a description of them such as "Answer provided". Do not include hidden reasoning. You are a decision-only supervisor with read tools. Do not modify files, run another agent, contact external systems or read credentials. Preserve your native session."#
    } else {
        CONTRACT
    };
    let permissions = if managing {
        if settings.manager_write_enabled {
            "Current permissions: read, write, edit and shell tools are enabled by Personal settings / Manager (manager_write_enabled=true). Use them for implementation requested by the user. This replaces permissions and claims from earlier turns. An OS permission error for one path does not mean these tools are disabled; use the current user's permanent directories where appropriate and report the actual failing path."
        } else if runtime == "codex" {
            "Current permissions: READ ONLY. Shell remains available for read-only inspection. Do not write or edit files or perform dispositive actions. You can still propose workflow graph changes through the structured result. This replaces permissions from earlier turns."
        } else {
            "Current permissions: READ ONLY. Do not write or edit files, execute shell commands or perform dispositive actions. You can still propose workflow graph changes through the structured result. This replaces permissions from earlier turns."
        }
    } else { "" };
    let manager_context = if managing {
        format!("Current authenticated account and permanent implementation directories: {}. These paths belong to this account; do not assume user ID 1 or use another account's home. Directory availability does not override the current permissions below.", crate::manager::implementation_context(app, uid))
    } else { String::new() };
    // The catalog assistant writes both locales on purpose.
    let language = if template_assistant { "" } else { LANGUAGE };
    let full = format!("{prompt}\n\n{contract}\n\n{language}\n\n{manager_context}\n\n{permissions}");
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let claude_api_baseline = if runtime == "claude" {
        crate::claude_time::baseline(
            &app.data.join(format!("users/{uid}")),
            &session.native_id,
            &run.workspace,
        )
    } else {
        None
    };
    // Read before launch, like the Claude baseline. Without a readable log the resumed turn keeps
    // the thread totals Codex reports.
    let codex_baseline = (runtime == "codex" && !fresh)
        .then(|| crate::codex_cost::thread_total(&app.data.join(format!("users/{uid}")), &session.native_id))
        .flatten();
    let mut child = cmd
        .spawn()
        .with_context(|| format!("CLI {runtime} non disponibile"))?;
    let _group = ProcessGroup(child.id().context("PID non disponibile")?);
    let mut stdin = child.stdin.take().unwrap();
    let full_copy = full.clone();
    tokio::spawn(async move {
        let _ = stdin.write_all(full_copy.as_bytes()).await;
        let _ = stdin.shutdown().await;
    });
    let mut err = child.stderr.take().unwrap();
    let err_task = tokio::spawn(async move {
        let mut b = Vec::new();
        let _ = (&mut err).take(2 * 1024 * 1024).read_to_end(&mut b).await;
        b
    });
    let stdout = child.stdout.take().unwrap();
    let mut lines = BufReader::new(stdout).lines();
    let node = run.current.clone();
    let rid = run.id.clone();
    let mut text = String::new();
    let mut failed = None;
    let mut pi_status = PiTurnStatus::default();
    let mut codex_status = CodexTurnStatus::default();
    let mut seen_session = None;
    let mut count = 0usize;
    let started = std::time::Instant::now();
    let mut telemetry = crate::finops::Tracker::new(
        runtime,
        &node,
        binding,
        &session.model,
        *run.attempts.get(&node).unwrap_or(&1),
    );
    telemetry.claude_api_baseline = claude_api_baseline;
    telemetry.codex_baseline = codex_baseline;
    let mut activity = crate::agent_activity::Tracker::default();
    emit(
        app,
        uid,
        &rid,
        "activity",
        &node,
        &serde_json::json!({"runtime":runtime,"phase":"agent_start"}).to_string(),
    );
    let read = async {
        while let Some(line) = lines.next_line().await? {
            count += line.len();
            ensure!(count < 32 * 1024 * 1024, "Output agente oltre il limite");
            let Ok(v) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if let Some(event) = crate::compaction::observe(runtime, &v) {
                emit(
                    app,
                    uid,
                    &rid,
                    "compaction",
                    &node,
                    &redact_turn(&event.to_string()),
                );
            }
            for event in activity.observe(runtime, &v) {
                emit(
                    app,
                    uid,
                    &rid,
                    "activity",
                    &node,
                    &redact_turn(&event.to_string()),
                );
            }
            if let Some(usage) = telemetry.observe(&v, &session.native_id) {
                emit(
                    app,
                    uid,
                    &rid,
                    "usage",
                    &node,
                    &serde_json::to_string(&usage)?,
                );
            }
            let ty = v["type"].as_str().unwrap_or("");
            if runtime == "pi" {
                pi_status.observe(ty, &v);
            }
            if runtime == "codex" {
                codex_status.observe(ty, &v);
                if ty == "error" {
                    // Retry notices stay visible as an ordinary line; the error of a failed turn
                    // is reported with the turn.
                    let message = v["message"].as_str().map_or_else(|| v.to_string(), str::to_owned);
                    let notice = serde_json::json!({"runtime":runtime,"phase":"notice","detail":message});
                    emit(app, uid, &rid, "activity", &node, &redact_turn(&notice.to_string()));
                }
            }
            if ty == "thread.started" {
                seen_session = v["thread_id"].as_str().map(str::to_owned);
            }
            if ty == "system" && v["subtype"] == "init" {
                seen_session = v["session_id"].as_str().map(str::to_owned);
            }
            if ty == "session" {
                seen_session = v["id"].as_str().map(str::to_owned);
            }
            if let Some(ref sid) = seen_session
                && session.native_id.is_empty()
            {
                session.native_id = sid.clone();
                run.sessions.insert(binding.into(), session.clone());
                if managing {
                    crate::manager::save_runtime(app, uid, run)?;
                } else {
                    save_turn_run(app, uid, run)?;
                }
            }
            let mut message = None;
            if ty == "item.completed" && v["item"]["type"] == "agent_message" {
                message = v["item"]["text"].as_str().map(str::to_owned);
            }
            if ty == "result" {
                if v["is_error"] == true {
                    failed = Some(v["result"].to_string());
                }
                message = v["result"].as_str().map(str::to_owned);
            }
            if ty == "message_end" && v["message"]["role"] == "assistant" {
                message = Some(
                    v["message"]["content"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|c| c["text"].as_str())
                                .collect::<Vec<_>>()
                                .join("\n")
                        })
                        .unwrap_or_default(),
                );
                if runtime != "pi" && ["error", "aborted"].contains(&v["message"]["stopReason"].as_str().unwrap_or(""))
                {
                    failed = Some(v["message"].to_string());
                }
            }
            if ty == "turn.failed" || ty == "error" && runtime != "codex" {
                failed = Some(v.to_string());
            }
            if let Some(m) = message {
                if runtime == "pi" || !m.is_empty() {
                    text.clone_from(&m);
                }
                if !m.is_empty() {
                    emit(app, uid, &rid, "output", &node, &redact_turn(&m));
                }
            }
        }
        Ok::<(), anyhow::Error>(())
    };
    let read_result = tokio::time::timeout(
        std::time::Duration::from_secs(settings.task_timeout_secs.max(1)),
        read,
    )
    .await;
    if !matches!(read_result, Ok(Ok(()))) {
        let _ = child.kill().await;
        err_task.abort();
        emit(
            app,
            uid,
            &rid,
            "agent_turn",
            &node,
            &telemetry
                .finish(started.elapsed().as_millis() as u64, "interrupted")
                .to_string(),
        );
        match read_result {
            Err(_) => bail!(LOCAL_TURN_TIMEOUT),
            Ok(result) => result.context("Lettura dell’output dell’agente interrotta")?,
        }
    }
    let status = child.wait().await?;
    let stderr = err_task.await.unwrap_or_default();
    let failed = failed.or(codex_status.failure(status.success()));
    let retry_pi = managing && runtime == "pi" && failed.is_none() && pi_status.can_retry();
    let failed = failed.or(pi_status.assistant_error);
    let compact_error =
        failed.as_deref().unwrap_or("").to_owned() + &String::from_utf8_lossy(&stderr);
    if compact_error.to_lowercase().contains("compact") && (failed.is_some() || !status.success()) {
        emit(app, uid, &rid, "compaction", &node, &redact_turn(&serde_json::json!({"runtime":runtime,"phase":"error","mode":"automatic","message":compact_error}).to_string()));
    }
    emit(
        app,
        uid,
        &rid,
        "agent_turn",
        &node,
        &telemetry
            .finish(
                started.elapsed().as_millis() as u64,
                if status.success() && failed.is_none() {
                    "cli_success"
                } else {
                    "cli_error"
                },
            )
            .to_string(),
    );
    if let Some(sid) = seen_session {
        ensure!(
            session.native_id.is_empty() || session.native_id == sid,
            "La CLI ha cambiato la sessione nativa durante resume"
        );
        session.native_id = sid;
    }
    ensure!(
        !session.native_id.is_empty(),
        "La CLI non ha restituito una sessione nativa persistente"
    );
    run.sessions.insert(binding.into(), session.clone());
    if managing {
        crate::manager::save_runtime(app, uid, run)?;
    } else {
        save_turn_run(app, uid, run)?;
    }
    if retry_pi {
        return Err(RetryablePiTimeout.into());
    }
    ensure!(
        status.success() && failed.is_none(),
        "{}",
        redact_turn(&explain_agent_failure(
            runtime,
            failed.unwrap_or_else(|| {
                String::from_utf8_lossy(&stderr)
                    .chars()
                    .take(3000)
                    .collect()
            })
        ))
    );
    let text = redact_turn(&text);
    let result = parse_result(&text).map_err(|error| {
        if error.to_string() == MISSING_STRUCTURED_RESULT { MissingStructuredResult(text.clone()).into() } else { error }
    })?;
    session.turns += 1;
    run.sessions.insert(binding.into(), session);
    if managing {
        crate::manager::save_runtime(app, uid, run)?;
    } else {
        save_turn_run(app, uid, run)?;
    }
    Ok(result)
}
pub fn emit(app: &App, uid: i64, run: &str, kind: &str, node: &str, text: &str) {
    if let Ok(e) = app.store.event(uid, run, kind, node, text) {
        let _ = app.bus.send((uid, e));
    }
}
pub fn parse_result(text: &str) -> Result<AgentResult> {
    let raw = text
        .rsplit_once("<nodo_result>")
        .and_then(|(_, s)| s.split_once("</nodo_result>").map(|(j, _)| j))
        .unwrap_or(text);
    let mut result: AgentResult = serde_json::from_str(raw.trim()).context(MISSING_STRUCTURED_RESULT)?;
    // Only the parser can attest that it truncated the public report.
    result.details_truncated = false;
    if result.details.is_empty()
        && let Some((report, _)) = text.rsplit_once("<nodo_result>")
    {
        let mut chars = report.trim().chars();
        result.details = chars.by_ref().take(16_000).collect();
        result.details_truncated = chars.next().is_some();
    }
    ensure!(
        ["completed", "needs_input", "blocked"].contains(&result.status.as_str()),
        "Stato agente non valido"
    );
    ensure!(
        result.status != "needs_input" || !result.questions.is_empty(),
        "L'agente richiede input senza domande"
    );
    Ok(result)
}

// The copy leaves out `.git`, `target` and the dependency folders: see `project_tree::omitted`.
// Only /sources folder copies have a per-file limit. Snapshots and examples
// must not discard large files (including files originating in remote repos).
pub fn copy_source_tree(src: &Path, dst: &Path, limit: u64) -> Result<()> {
    source_copy_size(src, limit)?;
    copy_tree_inner(src, dst, Some(limit))
}

pub fn source_copy_size(src: &Path, limit: u64) -> Result<u64> {
    source_tree_size(src, Some(limit))
}
pub fn source_tree_size(src: &Path, limit: Option<u64>) -> Result<u64> {
    let (bytes, error) = source_tree_info(src, limit)?;
    if let Some(error) = error { anyhow::bail!(error); }
    Ok(bytes)
}

/// Inspect size and copy eligibility in one metadata pass, without reading contents.
pub fn source_tree_info(src: &Path, limit: Option<u64>) -> Result<(u64, Option<String>)> {
    ensure!(!src.is_symlink(), "Symlink Source non ammesso");
    let mut total = 0u64;
    let mut copy_error = None;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        if crate::project_tree::omitted(&entry.file_name(), entry.file_type()?.is_file()) {
            continue;
        }
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path)?;
        ensure!(!metadata.is_symlink(), "Symlink non ammesso nel Source: {}", path.display());
        if metadata.is_dir() {
            let (bytes, error) = source_tree_info(&path, limit)?;
            total = total.saturating_add(bytes);
            copy_error = copy_error.or(error);
        } else {
            ensure!(metadata.is_file(), "File Source non supportato: {}", path.display());
            if let Some(limit) = limit
                && metadata.len() > limit && copy_error.is_none() {
                copy_error = Some(format!(
                    "Il file {} supera il limite di {} MiB: usa la sorgente direttamente oppure aumenta il limite in Configurazioni",
                    path.display(), limit / (1024 * 1024)));
            }
            total = total.saturating_add(metadata.len());
        }
    }
    Ok((total, copy_error))
}

pub async fn copy_tree_async(src: &Path, dst: &Path) -> Result<()> {
    let (src, dst) = (src.to_path_buf(), dst.to_path_buf());
    crate::artifacts::blocking(move || copy_tree(&src, &dst)).await
}

pub async fn own_tree_async(app: &App, uid: i64, path: &Path) -> Result<()> {
    if !cfg!(target_os = "linux") || !app.isolate { return Ok(()); }
    let (app, path) = (app.clone(), path.to_path_buf());
    crate::artifacts::blocking(move || own_tree(&app, uid, &path)).await
}

pub fn copy_tree(src: &Path, dst: &Path) -> Result<()> {
    copy_tree_inner(src, dst, None)
}

fn copy_tree_inner(src: &Path, dst: &Path, max_file_bytes: Option<u64>) -> Result<()> {
    ensure!(!src.is_symlink(), "Symlink Source non ammesso");
    let canonical_source = std::fs::canonicalize(src)?;
    ensure!(
        !std::path::absolute(dst)?.starts_with(&canonical_source),
        "La destinazione della copia è dentro la sorgente"
    );
    ensure!(!dst.is_symlink(), "Destinazione copia non valida");
    std::fs::create_dir_all(dst)?;
    for e in std::fs::read_dir(src)? {
        let e = e?;
        let name = e.file_name();
        if crate::project_tree::omitted(&name, e.file_type()?.is_file()) {
            continue;
        }
        let p = e.path();
        let metadata = std::fs::symlink_metadata(&p)?;
        ensure!(
            !metadata.is_symlink(),
            "Symlink non ammesso nel Source: {}",
            p.display()
        );
        if metadata.is_dir() {
            copy_tree_inner(&p, &dst.join(name), max_file_bytes)?;
        } else {
            ensure!(
                metadata.is_file(),
                "File Source non supportato: {}",
                p.display()
            );
            if let Some(limit) = max_file_bytes {
                ensure!(
                    metadata.len() <= limit,
                    "Il file {} supera il limite di {} MiB",
                    p.display(),
                    limit / (1024 * 1024)
                );
            }
            copy_file_nofollow(&p, &dst.join(name))?;
        }
    }
    Ok(())
}
/// Copies a regular file without following a link that replaced it after its type was checked
/// (an earlier workspace belongs to the agent), keeping its permission bits like `std::fs::copy`.
fn copy_file_nofollow(src: &Path, dst: &Path) -> Result<()> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let mut input = std::fs::OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW).open(src)?;
    let metadata = input.metadata()?;
    ensure!(metadata.is_file(), "File Source non supportato: {}", src.display());
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(dst)?;
    std::io::copy(&mut input, &mut output)?;
    output.set_permissions(std::fs::Permissions::from_mode(metadata.permissions().mode() & 0o777))?;
    Ok(())
}
/// Content proof for CI: include project files and executable modes, not build caches.
pub(crate) fn workspace_fingerprint(root: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    use std::{io::Read, os::unix::fs::PermissionsExt};
    fn walk(root: &Path, dir: &Path, records: &mut Vec<(String, String, u32)>) -> Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let name = entry.file_name();
            let meta = std::fs::symlink_metadata(&path)?;
            // What the copy leaves out, and rightcommit's own folders (whatever their type).
            if crate::project_tree::omitted(&name, meta.is_file())
                || crate::source_access::internal_dir(&name.to_string_lossy())
            {
                continue;
            }
            let relative = path.strip_prefix(root)?.to_string_lossy().into_owned();
            if meta.file_type().is_symlink() {
                records.push((
                    relative,
                    format!("symlink:{}", std::fs::read_link(&path)?.to_string_lossy()),
                    0,
                ));
            } else if meta.is_dir() {
                walk(root, &path, records)?;
            } else if meta.is_file() {
                let mut file = std::fs::File::open(&path)?;
                let mut hasher = Sha256::new();
                let mut buffer = [0u8; 16384];
                loop {
                    let size = file.read(&mut buffer)?;
                    if size == 0 {
                        break;
                    }
                    hasher.update(&buffer[..size]);
                }
                records.push((
                    relative,
                    hex::encode(hasher.finalize()),
                    meta.permissions().mode() & 0o777,
                ));
            }
            ensure!(
                records.len() < 20000,
                "Troppi file per verificare il workspace"
            );
        }
        Ok(())
    }
    let mut records = Vec::new();
    walk(root, root, &mut records)?;
    records.sort();
    Ok(hash(serde_json::to_vec(&records)?))
}

pub fn snapshot(
    app: &App,
    uid: i64,
    r: &Run,
    node: &str,
    binding: &str,
    summary: &str,
) -> Result<()> {
    crate::artifacts::snapshot(app, uid, r, node, binding, summary)
}

// Parallel turns only publish their own session; the coordinator owns run state.
fn save_turn_run(app: &App, uid: i64, run: &Run) -> Result<()> {
    if run.context.contains_key("template-assistant") {
        return Ok(());
    }
    if run.context.contains_key("parallel-turn") {
        let mut live = app.store.run(uid, &run.id)?;
        let agent = &run.workflow.node(&run.current)?.agent;
        if let Some(session) = run.sessions.get(agent) {
            live.sessions.insert(agent.clone(), session.clone());
        }
        app.store.save_run(uid, &live)
    } else {
        app.store.save_run(uid, run)
    }
}

#[cfg(test)]
mod pi_readiness_tests {
    use super::*;
    #[test]
    fn empty_or_unrelated_auth_file_does_not_mark_pi_configured() {
        let root=tempfile::tempdir().unwrap();let auth=root.path().join(".pi/agent/auth.json");std::fs::create_dir_all(auth.parent().unwrap()).unwrap();
        assert!(!pi_has_login(root.path(),"anthropic"));
        for content in ["{}","broken",r#"{"anthropic":{}}"#,r#"{"anthropic":{"type":"api_key","key":""}}"#,r#"{"openai":{"type":"api_key","key":"test-secret"}}"#] {
            std::fs::write(&auth,content).unwrap();assert!(!pi_has_login(root.path(),"anthropic"));
        }
        std::fs::write(&auth,r#"{"anthropic":{"type":"api_key","key":"test-secret"}}"#).unwrap();assert!(pi_has_login(root.path(),"anthropic"));assert!(!pi_has_login(root.path(),"openai"));
        std::fs::write(&auth,r#"{"anthropic":{"type":"oauth","access":"test-access","refresh":"test-refresh","expires":123}}"#).unwrap();assert!(pi_has_login(root.path(),"anthropic"));
    }
    #[test]
    fn model_is_required_before_pi_launch() {
        assert!(validate_pi_model("pi","").is_err());assert!(validate_pi_model("pi","  ").is_err());assert!(validate_pi_model("pi","configured-model").is_ok());assert!(validate_pi_model("codex","").is_ok());
    }
    #[test]
    fn codex_mcp_bridge_is_approved_by_its_server_entry() {
        let args = codex_mcp_overrides("/opt/right \"commit\"/rightcommit");
        assert!(args.chunks(2).all(|pair| pair[0] == "-c" && pair[1].starts_with("mcp_servers.nodo.")));
        let values: Vec<&str> = args.iter().skip(1).step_by(2).map(String::as_str).collect();
        assert_eq!(values, [
            r#"mcp_servers.nodo.command="/opt/right \"commit\"/rightcommit""#,
            r#"mcp_servers.nodo.args=["mcp-bridge"]"#,
            r#"mcp_servers.nodo.env_vars=["NODO_MCP_TOKEN","NODO_MCP_ENDPOINT"]"#,
            "mcp_servers.nodo.startup_timeout_sec=30",
            "mcp_servers.nodo.tool_timeout_sec=330",
            // Without it a sandboxed turn with approval policy `never` refuses every bridge call.
            r#"mcp_servers.nodo.default_tools_approval_mode="approve""#,
        ]);
    }
}

#[cfg(test)]
mod server_environment_tests {
    use super::*;
    use std::collections::BTreeMap;

    fn server(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(name, value)| (name.to_string(), value.to_string())).collect()
    }
    fn environment(c: &Command) -> BTreeMap<String, Option<String>> {
        c.as_std()
            .get_envs()
            .map(|(name, value)| (name.to_string_lossy().into_owned(), value.map(|v| v.to_string_lossy().into_owned())))
            .collect()
    }
    fn manager(app: &App, runtime: &str, config: AgentConfig, inherited: &[(String, String)]) -> BTreeMap<String, Option<String>> {
        let mut settings = Settings::default();
        settings.agents.insert(runtime.into(), config);
        let home = user_home(app, 1).unwrap();
        let mut c = command(app, 1, runtime, &home, &settings).unwrap();
        inherit_server_environment(&mut c, runtime, inherited);
        environment(&c)
    }
    fn value<'a>(env: &'a BTreeMap<String, Option<String>>, name: &str) -> Option<&'a str> {
        env.get(name).and_then(|value| value.as_deref())
    }

    /// Every name the provider rules read must stay out of the inherited environment, so a
    /// new name in those rules fails here until it is managed.
    #[test]
    fn every_name_the_provider_rules_read_is_managed() {
        let recorded = std::cell::RefCell::new(std::collections::BTreeSet::<String>::new());
        for runtime in ["claude", "codex", "pi"] {
            recorded.borrow_mut().clear();
            let mut configs = vec![AgentConfig::default(), AgentConfig { api_key: "test-personal-key".into(), ..Default::default() }];
            if runtime == "pi" {
                configs = crate::pi_providers::catalog()
                    .iter()
                    .flat_map(|provider| [false, true].map(|multi_provider| AgentConfig { provider: provider.id.clone(), api_key: "test-personal-key".into(), multi_provider, ..Default::default() }))
                    .collect();
            }
            for cfg in &configs {
                for present in [false, true] {
                    resolve_provider_environment_for_user(1, runtime, cfg, |name| {
                        recorded.borrow_mut().insert(name.to_string());
                        present.then(|| "1".to_string())
                    });
                }
            }
            if runtime == "codex" {
                // provider_environment_in_home adds these for native Codex provider profiles.
                recorded.borrow_mut().extend(AWS_ENV.iter().filter(|name| name.starts_with("AWS_")).map(|name| name.to_string()));
            }
            let names = recorded.borrow().clone();
            assert!(names.len() > 3, "{runtime}: the probe must reach the provider rules");
            let leaked: Vec<_> = names.iter().filter(|name| !provider_managed(runtime, name)).collect();
            assert!(leaked.is_empty(), "{runtime} would inherit managed variables: {leaked:?}");

            // The same names through a real command: today's outcome, whatever the server holds.
            let dir = tempfile::tempdir().unwrap();
            let app = crate::tests::app(dir.path());
            let config = AgentConfig { provider: "anthropic".into(), model: "test-model".into(), api_key: "test-personal-key".into(), ..Default::default() };
            let mut settings = Settings::default();
            settings.agents.insert(runtime.into(), config);
            let home = user_home(&app, 1).unwrap();
            let before = environment(&command(&app, 1, runtime, &home, &settings).unwrap());
            let fake: Vec<_> = names.iter().map(|name| (name.clone(), "test-only-server-value".to_string())).collect();
            let mut c = command(&app, 1, runtime, &home, &settings).unwrap();
            inherit_server_environment(&mut c, runtime, &fake);
            assert!(environment(&c) == before, "{runtime}: a managed variable changed");
        }
    }

    #[test]
    fn codex_manager_sees_other_keys_and_custom_variables_but_keeps_its_own_rules() {
        let dir = tempfile::tempdir().unwrap();
        let app = crate::tests::app(dir.path());
        let home = user_home(&app, 1).unwrap();
        let inherited = server(&[
            ("GEMINI_API_KEY", "test-only-gemini-value"),
            ("MY_SERVICE_URL", "https://service.test"),
            ("NODO_CONNECTOR_HOSTS", "example.test"),
            ("OPENAI_BASE_URL", "https://gateway.test/v1"),
            ("CODEX_ACCESS_TOKEN", "test-only-codex-access"),
            ("OPENAI_API_KEY", "test-only-openai-value"),
            ("AWS_ACCESS_KEY_ID", "test-only-aws-id"),
            ("HOME", "/elsewhere"),
            ("CODEX_HOME", "/elsewhere/.codex"),
            ("PATH", "/elsewhere/bin"),
            ("GIT_DIR", "/elsewhere/.git"),
            ("XDG_CACHE_HOME", "/elsewhere/.cache"),
            ("LC_ALL", "C"),
            ("PWD", "/elsewhere"),
        ]);
        let env = manager(&app, "codex", AgentConfig { api_key: "test-personal-codex".into(), ..Default::default() }, &inherited);
        assert_eq!(value(&env, "GEMINI_API_KEY"), Some("test-only-gemini-value"));
        assert_eq!(value(&env, "MY_SERVICE_URL"), Some("https://service.test"));
        assert_eq!(value(&env, "NODO_CONNECTOR_HOSTS"), Some("example.test"));
        assert_eq!(value(&env, "CODEX_API_KEY"), Some("test-personal-codex"));
        for name in ["OPENAI_BASE_URL", "CODEX_ACCESS_TOKEN", "OPENAI_API_KEY", "AWS_ACCESS_KEY_ID", "GIT_DIR", "XDG_CACHE_HOME", "LC_ALL", "PWD"] {
            assert_eq!(value(&env, name), None, "{name}");
        }
        assert_eq!(value(&env, "HOME"), Some(home.to_str().unwrap()));
        assert_eq!(value(&env, "CODEX_HOME"), Some(home.join(".codex").to_str().unwrap()));
        assert_ne!(value(&env, "PATH"), Some("/elsewhere/bin"));
    }

    #[test]
    fn claude_manager_with_a_personal_key_gets_no_gateway_or_subscription() {
        let dir = tempfile::tempdir().unwrap();
        let app = crate::tests::app(dir.path());
        let inherited = server(&[
            ("ANTHROPIC_BASE_URL", "https://gateway.test"),
            ("ANTHROPIC_CUSTOM_HEADERS", "Authorization: Bearer test-only-header"),
            ("CLAUDE_CODE_OAUTH_TOKEN", "test-only-subscription"),
            ("ANTHROPIC_AUTH_TOKEN", "test-only-bearer"),
            ("CLAUDE_CODE_USE_BEDROCK", "1"),
            ("AWS_ACCESS_KEY_ID", "test-only-aws-id"),
            ("GOOGLE_APPLICATION_CREDENTIALS", "/elsewhere/key.json"),
            ("GEMINI_API_KEY", "test-only-gemini-value"),
            ("OPENAI_API_KEY", "test-only-openai-value"),
            ("CLAUDE_CONFIG_DIR", "/elsewhere/.claude"),
        ]);
        let env = manager(&app, "claude", AgentConfig { api_key: "test-personal-claude".into(), ..Default::default() }, &inherited);
        assert_eq!(value(&env, "ANTHROPIC_API_KEY"), Some("test-personal-claude"));
        for name in ["ANTHROPIC_BASE_URL", "ANTHROPIC_CUSTOM_HEADERS", "CLAUDE_CODE_OAUTH_TOKEN", "ANTHROPIC_AUTH_TOKEN", "CLAUDE_CODE_USE_BEDROCK", "AWS_ACCESS_KEY_ID", "GOOGLE_APPLICATION_CREDENTIALS"] {
            assert_eq!(value(&env, name), None, "{name}");
        }
        assert_eq!(value(&env, "GEMINI_API_KEY"), Some("test-only-gemini-value"));
        assert_eq!(value(&env, "OPENAI_API_KEY"), Some("test-only-openai-value"));
        assert_eq!(value(&env, "CLAUDE_CONFIG_DIR"), Some(user_home(&app, 1).unwrap().join(".claude").to_str().unwrap()));
    }

    #[test]
    fn pi_manager_on_anthropic_gets_no_subscription_token_or_other_provider_keys() {
        let dir = tempfile::tempdir().unwrap();
        let app = crate::tests::app(dir.path());
        let inherited = server(&[
            ("ANTHROPIC_AUTH_TOKEN", "test-only-bearer"),
            ("ANTHROPIC_OAUTH_TOKEN", "test-only-oauth"),
            ("CLAUDE_CODE_OAUTH_TOKEN", "test-only-subscription"),
            ("GEMINI_API_KEY", "test-only-gemini-value"),
            ("PI_LOCAL_BASE_URL", "http://127.0.0.1:1/v1"),
            ("MY_SERVICE_URL", "https://service.test"),
        ]);
        let config = AgentConfig { provider: "anthropic".into(), model: "test-model".into(), api_key: "test-personal-pi".into(), ..Default::default() };
        let env = manager(&app, "pi", config, &inherited);
        assert_eq!(value(&env, "ANTHROPIC_API_KEY"), Some("test-personal-pi"));
        for name in ["ANTHROPIC_AUTH_TOKEN", "ANTHROPIC_OAUTH_TOKEN", "CLAUDE_CODE_OAUTH_TOKEN", "GEMINI_API_KEY", "PI_LOCAL_BASE_URL"] {
            assert_eq!(value(&env, name), None, "{name}");
        }
        assert_eq!(value(&env, "MY_SERVICE_URL"), Some("https://service.test"));
    }

    #[test]
    fn only_the_first_administrator_manager_inherits() {
        assert!(manager_inherits_server_environment(1));
        assert!(!manager_inherits_server_environment(2));
    }

    #[test]
    fn secret_looking_server_values_are_redacted() {
        let inherited = server(&[
            ("MY_SERVICE_TOKEN", "test-only-service-token"),
            ("db_Password", "test-only-db-pass"),
            ("Deploy_Credentials", "test-only-credential"),
            ("SHORT_KEY", "abc123"),
            ("PUBLIC_URL", "https://service.test/public"),
        ]);
        let text = "token=test-only-service-token pass=test-only-db-pass cred=test-only-credential short=abc123 url=https://service.test/public";
        assert_eq!(
            redact_server_secrets(text, &inherited),
            "token=[redacted] pass=[redacted] cred=[redacted] short=abc123 url=https://service.test/public"
        );
        assert_eq!(redact_server_secrets(text, &[]), text);
    }
}

#[cfg(test)]
mod codex_retry_tests {
    use super::*;
    use serde_json::json;

    fn status(lines: &[Value], exited_successfully: bool) -> Option<String> {
        let mut status = CodexTurnStatus::default();
        for line in lines {
            status.observe(line["type"].as_str().unwrap_or(""), line);
        }
        status.failure(exited_successfully)
    }
    #[test]
    fn a_retry_notice_followed_by_a_completed_turn_is_not_a_failure() {
        let retry = json!({"type":"error","message":"Reconnecting... 1/5 (stream disconnected before completion)"});
        let completed = json!({"type":"turn.completed","usage":{"input_tokens":1,"output_tokens":1}});
        assert_eq!(status(&[retry.clone(), completed.clone()], true), None);
        assert_eq!(status(&[completed.clone()], true), None);
        // No completion after the error, a later error, or a non-zero exit: the last error stands.
        assert_eq!(status(&[retry.clone()], true), Some(retry.to_string()));
        let fatal = json!({"type":"error","message":"unexpected status 401 Unauthorized"});
        assert_eq!(status(&[retry.clone(), completed.clone(), fatal.clone()], true), Some(fatal.to_string()));
        assert_eq!(status(&[retry.clone(), completed], false), Some(retry.to_string()));
    }
    /// A stand-in codex writes what `codex exec --json` 0.172.0 writes for each case.
    #[tokio::test]
    async fn a_codex_turn_that_recovers_after_reconnecting_completes() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let app = crate::tests::app(dir.path());
        app.store.update_settings(1, |s| {
            s.agents.insert("codex".into(), AgentConfig { api_key: "test-codex".into(), ..Default::default() });
            Ok(())
        }).unwrap();
        let tools = crate::distribution::tools(dir.path());
        std::fs::create_dir_all(&tools).unwrap();
        std::fs::write(tools.join("codex"), r#"#!/bin/sh
cat > /dev/null
result='{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"<nodo_result>{\"status\":\"completed\",\"summary\":\"Recovered\"}</nodo_result>"}}'
completed='{"type":"turn.completed","usage":{"input_tokens":1,"output_tokens":1}}'
printf '%s\n' '{"type":"thread.started","thread_id":"retry-session"}' '{"type":"turn.started"}'
printf '%s\n' '{"type":"error","message":"Reconnecting... 1/5 (stream disconnected before completion: idle timeout)"}'
case "$(cat "$HOME/mode")" in
  completed) printf '%s\n' "$result" "$completed" ;;
  failed) printf '%s\n' '{"type":"error","message":"Reconnecting... 5/5"}' '{"type":"turn.failed","error":{"message":"exceeded retry limit, last status: 503 Service Unavailable"}}'; exit 1 ;;
  unanswered) printf '%s\n' "$result" ;;
  exit) printf '%s\n' "$result" "$completed"; exit 1 ;;
esac
"#).unwrap();
        std::fs::set_permissions(tools.join("codex"), std::fs::Permissions::from_mode(0o700)).unwrap();
        let home = user_home(&app, 1).unwrap();
        let w: Workflow = serde_json::from_value(json!({
            "id":"retry", "name":"Retry", "description":"", "entry":"step",
            "nodes":[{"id":"source","kind":"Source","source_type":"example"},
                {"id":"author","kind":"Agent","runtime":"codex"},
                {"id":"step","kind":"Task","agent":"author","prompt":"Work"}],
            "edges":[{"from":"source","to":"author","kind":"context"},{"from":"author","to":"step","kind":"context"}]
        })).unwrap();
        for mode in ["completed", "failed", "unanswered", "exit"] {
            std::fs::write(home.join("mode"), mode).unwrap();
            let r = crate::engine::create(&app, 1, w.clone(), "Request".into(), None).await.unwrap();
            let result = crate::engine::drive(&app, 1, &r.id).await;
            let saved = app.store.run(1, &r.id).unwrap();
            let events = app.store.events(1, &r.id, 0).unwrap();
            // The retry notice stays in the Activity log as an ordinary line, not as an error.
            let notice = events.iter().find(|e| e.kind == "activity" && e.text.contains("Reconnecting... 1/5")).expect(mode);
            assert_eq!(serde_json::from_str::<Value>(&notice.text).unwrap()["phase"], "notice", "{mode}");
            let turn: Value = serde_json::from_str(&events.iter().find(|e| e.kind == "agent_turn").unwrap().text).unwrap();
            if mode == "completed" {
                result.unwrap();
                assert_eq!(saved.status, "completed");
                assert_eq!(saved.outputs["step"].summary, "Recovered");
                assert_eq!(turn["status"], "cli_success");
                assert!(!events.iter().any(|e| e.kind == "error"));
                continue;
            }
            let error = result.unwrap_err().to_string();
            assert_ne!(saved.status, "completed", "{mode}");
            assert_eq!(turn["status"], "cli_error", "{mode}");
            // The error is the one Codex reported last, as before.
            match mode {
                "failed" => assert!(error.contains("turn.failed") && error.contains("exceeded retry limit"), "{error}"),
                _ => assert!(error.contains("Reconnecting... 1/5"), "{mode}: {error}"),
            }
        }
    }
}
