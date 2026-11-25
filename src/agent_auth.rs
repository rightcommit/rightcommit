use crate::{
    App, agents,
    model::{AgentConfig, Settings, Workflow},
};
use serde_json::{Value, json};
use std::{collections::{BTreeMap, HashSet}, path::Path, process::Stdio};

type Env = BTreeMap<String, String>;
fn on(env: &Env, key: &str) -> bool {
    env.get(key)
        .is_some_and(|v| ["1", "true", "yes", "on"].contains(&v.to_ascii_lowercase().as_str()))
}
fn info(method: &str, source: &str, provider: &str) -> Value {
    json!({"method":method,"source":source,"provider":provider})
}
fn aws_source(env: &Env) -> &str {
    [
        "AWS_BEARER_TOKEN_BEDROCK",
        "ANTHROPIC_AWS_API_KEY",
        "AWS_ACCESS_KEY_ID",
        "AWS_WEB_IDENTITY_TOKEN_FILE",
        "AWS_PROFILE",
        "AWS_SHARED_CREDENTIALS_FILE",
    ]
    .into_iter()
    .find(|key| env.contains_key(*key))
    .unwrap_or("native_credentials")
}
fn claude(cfg: &AgentConfig, env: &Env, native_login: bool) -> Value {
    let aws = [
        "CLAUDE_CODE_USE_BEDROCK",
        "CLAUDE_CODE_USE_MANTLE",
        "CLAUDE_CODE_USE_ANTHROPIC_AWS",
    ]
    .iter()
    .any(|key| on(env, key));
    let vertex = on(env, "CLAUDE_CODE_USE_VERTEX");
    let foundry = on(env, "CLAUDE_CODE_USE_FOUNDRY");
    if [aws, vertex, foundry]
        .into_iter()
        .filter(|active| *active)
        .count()
        > 1
    {
        return info("configuration_error", "conflicting_providers", "unknown");
    }
    if aws {
        let provider = if on(env, "CLAUDE_CODE_USE_ANTHROPIC_AWS") {
            "anthropic_aws"
        } else if on(env, "CLAUDE_CODE_USE_MANTLE") {
            "bedrock_mantle"
        } else {
            "bedrock"
        };
        let source = if env.contains_key("ANTHROPIC_AUTH_TOKEN") {
            "ANTHROPIC_AUTH_TOKEN"
        } else {
            aws_source(env)
        };
        return info("aws_credentials", source, provider);
    }
    if vertex {
        let source = [
            "ANTHROPIC_AUTH_TOKEN",
            "GOOGLE_OAUTH_ACCESS_TOKEN",
            "GOOGLE_APPLICATION_CREDENTIALS",
        ]
        .into_iter()
        .find(|key| env.contains_key(*key))
        .unwrap_or("native_credentials");
        return info("google_credentials", source, "vertex");
    }
    if foundry {
        let source = [
            "ANTHROPIC_FOUNDRY_AUTH_TOKEN",
            "ANTHROPIC_FOUNDRY_API_KEY",
            "ANTHROPIC_AUTH_TOKEN",
            "AZURE_CLIENT_SECRET",
            "AZURE_FEDERATED_TOKEN_FILE",
        ]
        .into_iter()
        .find(|key| env.contains_key(*key))
        .unwrap_or("native_credentials");
        return info("azure_credentials", source, "foundry");
    }
    let provider = if env.contains_key("ANTHROPIC_BASE_URL") {
        "gateway"
    } else {
        "anthropic"
    };
    if env
        .get("ANTHROPIC_CUSTOM_HEADERS")
        .is_some_and(|headers| agents::has_auth_header(headers))
    {
        return info("auth_headers", "ANTHROPIC_CUSTOM_HEADERS", provider);
    }
    if env.contains_key("ANTHROPIC_AUTH_TOKEN") {
        return info("bearer_token", "ANTHROPIC_AUTH_TOKEN", provider);
    }
    if env.contains_key("CLAUDE_CODE_OAUTH_TOKEN") {
        return info("oauth_token", "CLAUDE_CODE_OAUTH_TOKEN", provider);
    }
    if env.contains_key("ANTHROPIC_API_KEY") {
        return info(
            "api_key",
            if cfg.api_key.is_empty() {
                "ANTHROPIC_API_KEY"
            } else {
                "personal_settings"
            },
            provider,
        );
    }
    if native_login {
        info("native_login", "native_credentials", provider)
    } else {
        info("not_configured", "none", provider)
    }
}
fn codex(
    cfg: &AgentConfig,
    env: &Env,
    native: &Value,
    login: &Value,
    host_codex_key: bool,
) -> Value {
    let provider = native["provider"].as_str().unwrap_or("openai");
    if native["config_error"] == true {
        return info("configuration_error", "native_config", "unknown");
    }
    // A native cloud/custom provider selects its own authentication, independently of OpenAI login.
    if native["aws"] == true {
        return info("aws_credentials", aws_source(env), provider);
    }
    if provider != "openai" {
        return info(
            "native_provider",
            native["env_key"].as_str().unwrap_or("native_config"),
            provider,
        );
    }
    if env.contains_key("CODEX_API_KEY") {
        let source = if !cfg.api_key.is_empty() {
            "personal_settings"
        } else if host_codex_key {
            "CODEX_API_KEY"
        } else {
            "OPENAI_API_KEY"
        };
        return info("api_key", source, provider);
    }
    if env.contains_key("CODEX_ACCESS_TOKEN") {
        return info("access_token", "CODEX_ACCESS_TOKEN", provider);
    }
    match login["kind"].as_str().unwrap_or("none") {
        "chatgpt" => info("chatgpt_login", "login_file", provider),
        "api_key" => info("api_key", "login_file", provider),
        "unknown" => info("native_login", "login_file", provider),
        _ => info("not_configured", "none", provider),
    }
}
fn login_file(home: &Path) -> Value {
    let path = home.join(".codex/auth.json");
    let exists = path.is_file();
    let data = std::fs::metadata(&path)
        .ok()
        .filter(|m| m.len() <= 1024 * 1024)
        .and_then(|_| std::fs::read(&path).ok())
        .and_then(|data| serde_json::from_slice::<Value>(&data).ok());
    let kind = match data {
        Some(v)
            if v["tokens"]["access_token"]
                .as_str()
                .is_some_and(|s| !s.is_empty())
                || v["tokens"]["id_token"]
                    .as_str()
                    .is_some_and(|s| !s.is_empty()) =>
        {
            "chatgpt"
        }
        Some(v) if v["OPENAI_API_KEY"].as_str().is_some_and(|s| !s.is_empty()) => "api_key",
        _ if exists => "unknown",
        _ => "none",
    };
    json!({"path":path,"exists":exists,"kind":kind})
}
async fn native_codex(app: &App, uid: i64, home: &Path, settings: &Settings) -> Value {
    if !home.join(".codex/config.toml").is_file() {
        return json!({"provider":"openai"});
    }
    // Parse native TOML with the Python runtime already included in the image. Only names/flags
    // leave this process; keys, headers, commands and credential contents are never returned.
    let script = r#"import json,pathlib,re,tomllib
try:
 p=pathlib.Path.home()/'.codex/config.toml'
 if p.stat().st_size>262144:raise ValueError()
 c=tomllib.loads(p.read_text())
 profile=c.get('profile')
 if isinstance(profile,str):c.update(c.get('profiles',{}).get(profile,{}))
 provider=c.get('model_provider','openai')
 if not isinstance(provider,str) or not re.fullmatch(r'[A-Za-z0-9._-]{1,100}',provider):raise ValueError()
 definition=c.get('model_providers',{}).get(provider,{})
 key=definition.get('env_key')
 if not isinstance(key,str) or not re.fullmatch(r'[A-Za-z_][A-Za-z0-9_]{0,100}',key):key=None
 print(json.dumps({'provider':provider,'aws':provider.startswith('amazon-bedrock') or bool(definition.get('aws')),'env_key':key}))
except Exception:print(json.dumps({'config_error':True}))
"#;
    let Ok(mut cmd) = agents::command(app, uid, "python3", home, settings) else {
        return json!({"config_error":true});
    };
    cmd.args(["-c", script])
        .stdin(Stdio::null())
        .stderr(Stdio::null());
    match tokio::time::timeout(std::time::Duration::from_secs(3), cmd.output()).await {
        Ok(Ok(output)) if output.status.success() => {
            serde_json::from_slice(&output.stdout).unwrap_or(json!({"config_error":true}))
        }
        _ => json!({"config_error":true}),
    }
}
pub async fn describe(app: &App, uid: i64, settings: &Settings) -> anyhow::Result<Value> {
    let home = agents::user_home(app, uid)?;
    let own_claude = settings.agents.get("claude").cloned().unwrap_or_default();
    let claude_cfg = agents::effective_agent_config(app, uid, "claude", &own_claude)?;
    let own_codex = settings.agents.get("codex").cloned().unwrap_or_default();
    let codex_cfg = agents::effective_agent_config(app, uid, "codex", &own_codex)?;
    let claude_env = agents::provider_environment_in_home("claude", &claude_cfg, &home, uid);
    let codex_env = agents::provider_environment_in_home("codex", &codex_cfg, &home, uid);
    let native = native_codex(app, uid, &home, settings).await;
    let mut login = login_file(&home);
    let mut codex = codex(
        &codex_cfg,
        &codex_env,
        &native,
        &login,
        std::env::var("CODEX_API_KEY").is_ok_and(|v| !v.is_empty()),
    );
    if own_codex.api_key.is_empty() && !codex_cfg.api_key.is_empty() && codex["method"] == "api_key" {codex["source"] = json!("administrator_api_key");}
    login["active"] = json!(codex["source"] == "login_file");
    // The internal classification is not a public credential field.
    login.as_object_mut().unwrap().remove("kind");
    codex["login_file"] = login;
    let mut claude = claude(&claude_cfg, &claude_env, home.join(".claude/.credentials.json").is_file());
    if own_claude.api_key.is_empty() && !claude_cfg.api_key.is_empty() {claude["source"] = json!("administrator_api_key");}
    for auth in [&mut claude, &mut codex] {
        auth.as_object_mut().unwrap().remove("login_file");
        if auth["source"] == "login_file" {auth["source"] = json!("external_configuration");}
        if ["native_login","chatgpt_login","oauth_token","access_token"].contains(&auth["method"].as_str().unwrap_or("")) {
            auth["method"] = json!("external_configuration");auth["source"] = json!("external_configuration");
        }
    }
    Ok(json!({"claude":claude,"codex":codex}))
}
/// The "configured" state of Personal Settings -> Agents: the credentials this user's processes for
/// `runtime` receive (own key, the administrator's key, the administrator's environment, native login).
pub(crate) fn has_credentials(app: &App, uid: i64, runtime: &str, settings: &Settings, home: &Path) -> bool {
    let cfg = settings.agents.get(runtime).cloned().unwrap_or_default();
    let inherited = agents::effective_agent_config(app, uid, runtime, &cfg).unwrap_or_else(|_| cfg.clone());
    if runtime == "pi" {
        return if cfg.multi_provider {crate::pi_providers::any_available_for_user(app, uid, settings, home)}
            else {crate::pi_providers::available(&inherited, home)};
    }
    let provider_env = agents::provider_environment_in_home(runtime, &inherited, home, uid);
    agents::has_provider_credentials(&provider_env) || (!agents::uses_cloud_provider(&provider_env) && match runtime {
        "codex" => home.join(".codex/auth.json").is_file() || (home.join(".codex/config.toml").is_file() && home.join(".aws/credentials").is_file()),
        "claude" => home.join(".claude/.credentials.json").is_file(),
        _ => false,
    })
}
fn runtime_name(runtime: &str) -> &str {
    match runtime {"claude" => "Claude Code", "codex" => "Codex", "pi" => "Pi", other => other}
}
/// The Agent nodes a run will call: those bound to a Task that is not skipped, a Gate or an agent
/// Review, also inside called workflows. An Agent with no runtime uses the user's default agent.
fn used_agents(app: &App, uid: i64, w: &Workflow, default: &str, prefix: &str, seen: &mut HashSet<String>, out: &mut Vec<(String, String)>) {
    if !seen.insert(w.id.clone()) {return;}
    for a in w.nodes.iter().filter(|a| a.kind == "Agent" && w.nodes.iter().any(|n| n.agent == a.id
        && (n.kind == "Task" && !n.skip || n.kind == "Gate" || n.kind == "Review" && n.mode != "human"))) {
        let runtime = if a.runtime.is_empty() {default} else {&a.runtime};
        out.push((runtime.to_string(), format!("{prefix}{}", if a.title.trim().is_empty() {&a.id} else {&a.title})));
    }
    for n in w.nodes.iter().filter(|n| n.kind == "Workflow") {
        if let Ok(child) = app.store.workflow(uid, &n.workflow_id) {
            used_agents(app, uid, &child, default, &format!("{prefix}{} / ", child.name), seen, out);
        }
    }
}
/// Refuse a run before it starts when an agent it calls has no credentials for this user, instead of
/// failing at the first agent step. It uses the same state as Personal Settings and also accepts a
/// configuration only the CLI can read (External configuration), so it never blocks a setup that works.
pub(crate) async fn ensure_agents_configured(app: &App, uid: i64, w: &Workflow) -> anyhow::Result<()> {
    let settings = app.store.settings(uid)?;
    let home = app.data.join("users").join(uid.to_string());
    let mut used = vec![];
    used_agents(app, uid, w, &settings.default_runtime, "", &mut HashSet::new(), &mut used);
    let mut external: Option<Value> = None;
    let mut missing = vec![];
    for (runtime, title) in used {
        if has_credentials(app, uid, &runtime, &settings, &home) {continue;}
        if ["claude", "codex"].contains(&runtime.as_str()) {
            if external.is_none() {external = Some(describe(app, uid, &settings).await?);}
            if external.as_ref().is_some_and(|e| e[&runtime]["method"] != "not_configured") {continue;}
        }
        missing.push(format!("{title} ({})", runtime_name(&runtime)));
    }
    anyhow::ensure!(missing.is_empty(), "Il workflow usa agenti senza credenziali: {}. Configurali in Configurazioni personali → Agenti, poi avvia di nuovo il workflow.", missing.join(", "));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn env(values: &[(&str, &str)]) -> Env {
        values
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }
    #[test]
    fn reports_effective_bearer_and_cloud_without_returning_secrets() {
        let cfg = AgentConfig {
            api_key: "saved-secret".into(),
            ..Default::default()
        };
        let result = claude(
            &cfg,
            &env(&[
                ("ANTHROPIC_AUTH_TOKEN", "host-secret"),
                ("ANTHROPIC_BASE_URL", "https://example.invalid"),
            ]),
            true,
        );
        assert_eq!(result["method"], "bearer_token");
        assert_eq!(result["provider"], "gateway");
        assert!(!result.to_string().contains("secret"));
        assert!(!result.to_string().contains("example.invalid"));
        let result = claude(
            &cfg,
            &env(&[
                ("CLAUDE_CODE_USE_BEDROCK", "1"),
                ("AWS_BEARER_TOKEN_BEDROCK", "secret"),
            ]),
            true,
        );
        assert_eq!(result["provider"], "bedrock");
        assert_eq!(result["source"], "AWS_BEARER_TOKEN_BEDROCK");
    }
    #[test]
    fn codex_login_file_is_bypassed_by_keys_and_native_aws_profiles() {
        let cfg = AgentConfig::default();
        let native = json!({"provider":"openai"});
        let login = json!({"kind":"chatgpt"});
        assert_eq!(
            codex(&cfg, &Env::new(), &native, &login, false)["method"],
            "chatgpt_login"
        );
        assert_eq!(
            codex(
                &cfg,
                &env(&[("CODEX_API_KEY", "secret")]),
                &native,
                &login,
                false
            )["source"],
            "OPENAI_API_KEY"
        );
        assert_eq!(
            codex(
                &cfg,
                &env(&[("CODEX_API_KEY", "secret")]),
                &native,
                &login,
                true
            )["source"],
            "CODEX_API_KEY"
        );
        let aws = json!({"provider":"amazon-bedrock-runtime","aws":true});
        assert_eq!(
            codex(&cfg, &Env::new(), &aws, &login, false)["method"],
            "aws_credentials"
        );
    }
    #[test]
    fn file_metadata_is_scoped_and_does_not_return_tokens() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        std::fs::create_dir(home.join(".codex")).unwrap();
        assert_eq!(login_file(home)["exists"], false);
        std::fs::write(
            home.join(".codex/auth.json"),
            r#"{"tokens":{"access_token":"never-return-me"}}"#,
        )
        .unwrap();
        let metadata = login_file(home);
        assert_eq!(metadata["kind"], "chatgpt");
        assert!(!metadata.to_string().contains("never-return-me"));
        assert_eq!(login_file(&home.join("other-user"))["exists"], false);
    }
    fn workflow(id: &str, nodes: Value) -> Workflow {
        serde_json::from_value(json!({"id":id,"name":id,"description":"","entry":"task","nodes":nodes,"edges":[]})).unwrap()
    }
    fn agent_task(runtime: &str, skip: bool) -> Workflow {
        workflow("w", json!([{"id":"source","kind":"Source"},{"id":"author","kind":"Agent","title":"Author","runtime":runtime},
            {"id":"task","kind":"Task","title":"Write","agent":"author","prompt":"p","skip":skip},{"id":"review","kind":"Review","agent":"author","mode":"human"}]))
    }
    // User 2 receives no provider credentials from the server environment, so these checks do not
    // depend on the keys of the machine running the tests.
    #[tokio::test]
    async fn run_without_agent_credentials_is_refused_before_it_starts() {
        let dir = tempfile::tempdir().unwrap();
        let app = crate::tests::app(dir.path());
        let w = crate::presets::all().remove(0);
        app.store.save_workflow(2, &w).unwrap();
        let start: crate::api::StartRun = serde_json::from_value(json!({"workflow_id":w.id})).unwrap();
        let error = crate::api::prepare_run(&app, 2, start, None).await.unwrap_err().to_string();
        assert_eq!(error, "Il workflow usa agenti senza credenziali: SDD author (Codex). Configurali in Configurazioni personali → Agenti, poi avvia di nuovo il workflow.");
        assert!(app.store.runs(2).unwrap().is_empty());
        assert!(std::fs::read_dir(dir.path().join("users/2/workspaces")).map_or(true, |mut d| d.next().is_none()));
        // A called workflow's Agents are checked too, named after their workflow.
        let mut child = agent_task("claude", false);
        child.id = "child".into();
        child.name = "Child".into();
        app.store.save_workflow(2, &child).unwrap();
        let parent = workflow("parent", json!([{"id":"source","kind":"Source"},{"id":"call","kind":"Workflow","workflow_id":"child"}]));
        assert!(ensure_agents_configured(&app, 2, &parent).await.unwrap_err().to_string().contains(": Child / Author (Claude Code)."));
    }
    // An invalid workflow reports what to fix in the graph first; the credentials refusal is only for
    // a workflow that could otherwise start.
    #[tokio::test]
    async fn validation_errors_come_before_the_credentials_refusal() {
        let dir = tempfile::tempdir().unwrap();
        let app = crate::tests::app(dir.path());
        // A built-in workflow that starts with a Codex Agent, which user 2 has no credentials for.
        let valid = crate::presets::all().remove(0);
        let start = || -> crate::api::StartRun {serde_json::from_value(json!({"workflow_id":valid.id})).unwrap()};
        let mut no_entry = valid.clone();
        no_entry.entry.clear();
        app.store.save_workflow(2, &no_entry).unwrap();
        let error = crate::api::prepare_run(&app, 2, start(), None).await.unwrap_err().to_string();
        assert!(error.starts_with("Il workflow non ha un nodo di ingresso"), "{error}");
        let mut no_source = valid.clone();
        let sources: Vec<_> = no_source.nodes.iter().filter(|n| n.kind == "Source").map(|n| n.id.clone()).collect();
        no_source.nodes.retain(|n| n.kind != "Source");
        no_source.edges.retain(|e| !sources.contains(&e.from) && !sources.contains(&e.to));
        app.store.save_workflow(2, &no_source).unwrap();
        let error = crate::api::prepare_run(&app, 2, start(), None).await.unwrap_err().to_string();
        assert_eq!(error, "Aggiungi almeno un Source");
        app.store.save_workflow(2, &valid).unwrap();
        let error = crate::api::prepare_run(&app, 2, start(), None).await.unwrap_err().to_string();
        assert!(error.starts_with("Il workflow usa agenti senza credenziali"), "{error}");
        assert!(app.store.runs(2).unwrap().is_empty());
    }
    #[tokio::test]
    async fn runs_that_call_no_agent_or_a_configured_one_are_allowed() {
        let dir = tempfile::tempdir().unwrap();
        let app = crate::tests::app(dir.path());
        let local_ci: Workflow = serde_json::from_str(include_str!("../examples/local-ci/workflow.json")).unwrap();
        ensure_agents_configured(&app, 2, &local_ci).await.unwrap();
        // A skipped Task and a human Review never call their Agent.
        ensure_agents_configured(&app, 2, &agent_task("codex", true)).await.unwrap();
        assert!(ensure_agents_configured(&app, 2, &agent_task("codex", false)).await.is_err());
        // The administrator's saved key is inherited by users without their own.
        let mut admin = Settings::default();
        admin.agents.insert("codex".into(), AgentConfig {api_key: "admin-codex-key".into(), ..Default::default()});
        app.store.save_settings(1, &admin).unwrap();
        ensure_agents_configured(&app, 2, &agent_task("codex", false)).await.unwrap();
        ensure_agents_configured(&app, 2, &agent_task("", false)).await.unwrap();
        assert!(ensure_agents_configured(&app, 2, &agent_task("claude", false)).await.is_err());
        // A native login is an External configuration.
        let home = crate::agents::user_home(&app, 2).unwrap();
        std::fs::create_dir_all(home.join(".claude")).unwrap();
        std::fs::write(home.join(".claude/.credentials.json"), "{}").unwrap();
        ensure_agents_configured(&app, 2, &agent_task("claude", false)).await.unwrap();
        // A local Pi model needs no key; the default runtime applies to an Agent without one.
        let mut own = Settings {default_runtime: "pi".into(), ..Default::default()};
        own.agents.insert("pi".into(), AgentConfig {provider: "local".into(), base_url: "http://127.0.0.1:11434/v1".into(), model: "local-model".into(), ..Default::default()});
        app.store.save_settings(2, &own).unwrap();
        ensure_agents_configured(&app, 2, &agent_task("", false)).await.unwrap();
    }
    #[tokio::test]
    async fn administrator_environment_configures_the_agent() {
        if std::env::var("RIGHTCOMMIT_AGENT_ENV_CHILD").is_err() {
            // The environment belongs to the whole process: check it in a child run of this test only.
            let out = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "agent_auth::tests::administrator_environment_configures_the_agent", "--test-threads=1"])
                .env("RIGHTCOMMIT_AGENT_ENV_CHILD", "1")
                .env("ANTHROPIC_API_KEY", "test-only-environment-key")
                .output()
                .unwrap();
            let report = String::from_utf8_lossy(&out.stdout);
            assert!(out.status.success() && report.contains("1 passed"), "{report}");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let app = crate::tests::app(dir.path());
        let w = agent_task("claude", false);
        ensure_agents_configured(&app, 1, &w).await.unwrap();
        // Other users never receive the administrator's environment.
        assert!(ensure_agents_configured(&app, 2, &w).await.is_err());
    }
    #[tokio::test]
    async fn native_profiles_are_reported_without_disclosing_config_credentials() {
        let dir = tempfile::tempdir().unwrap();
        let app = crate::tests::app(dir.path());
        let home = agents::user_home(&app, 1).unwrap();
        let config = home.join(".codex/config.toml");
        std::fs::write(&config, "model_provider = \"amazon-bedrock-runtime\"\n[model_providers.amazon-bedrock-runtime.aws]\nregion = \"eu-central-1\"\n").unwrap();
        std::fs::write(
            home.join(".codex/auth.json"),
            r#"{"tokens":{"access_token":"private-native-secret"}}"#,
        )
        .unwrap();
        let result = describe(&app, 1, &Settings::default()).await.unwrap();
        assert_eq!(result["codex"]["method"], "aws_credentials");
        assert_eq!(result["codex"]["provider"], "amazon-bedrock-runtime");
        assert!(result["codex"].get("login_file").is_none());
        assert!(!result.to_string().contains("private-native-secret"));
        std::fs::write(config, "invalid = \"private-config-secret").unwrap();
        let result = describe(&app, 1, &Settings::default()).await.unwrap();
        assert_eq!(result["codex"]["method"], "configuration_error");
        assert!(!result.to_string().contains("private-config-secret"));
    }
}
