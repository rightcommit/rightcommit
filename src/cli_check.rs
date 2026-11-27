//! Connection check for Claude Code and Codex: one short request to the model with the
//! values in the settings form. Nothing is saved and no tool can run.
use crate::{
    App, agents,
    auth::{ApiResult, bad},
    model::{AgentConfig, User},
};
use axum::{Json, extract::State};
use serde_json::{Value, json};
use std::{process::Stdio, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const PROMPT: &str = "Connection check. Reply with OK only.";
const START_FAILED: &str = "Impossibile avviare l’agente. Verifica che sia installato nell’app.";
const FAILED: &str = "Verifica fallita. Controlla modello, credenziali e disponibilità della rete.";

/// An isolated working directory; remove it after the probe process exits.
struct ProbeWorkspace(std::path::PathBuf);
impl Drop for ProbeWorkspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The saved configuration with the form's model, effort and key. An empty key keeps the
/// saved one, as saving the form would.
fn draft_config(draft: AgentConfig, saved: &AgentConfig) -> anyhow::Result<AgentConfig> {
    anyhow::ensure!(
        draft.model.len() <= 200 && draft.effort.len() <= 20 && draft.api_key.len() <= 4096,
        "Configurazione dell’agente non valida."
    );
    let mut config = saved.clone();
    config.model = draft.model.trim().into();
    config.effort = draft.effort;
    if draft.api_key == "__CLEAR__" {
        config.api_key.clear();
    } else if !draft.api_key.is_empty() {
        config.api_key = draft.api_key;
    }
    Ok(config)
}

fn failure_message(detail: &str) -> &'static str {
    let detail = detail.to_lowercase();
    if ["401", "403", "unauthorized", "invalid api key", "no api key", "authentication", "not logged in", "/login"]
        .iter()
        .any(|sign| detail.contains(sign))
    {
        "Autenticazione fallita. Verifica la chiave API o l’accesso dell’agente."
    } else if ["429", "quota", "credit", "rate limit", "usage limit"].iter().any(|sign| detail.contains(sign)) {
        "Il provider ha raggiunto un limite o non dispone di credito. Riprova più tardi o verifica il tuo account."
    } else if detail.contains("model")
        && ["not found", "unknown", "not available", "not support", "does not exist", "invalid"]
            .iter()
            .any(|sign| detail.contains(sign))
    {
        "Il modello non è disponibile. Verifica il nome del modello e i permessi del tuo account."
    } else {
        FAILED
    }
}

/// Reads the CLI's JSON events. Only the error events and stderr are used to explain a
/// failure, and never returned: native output can include credentials.
fn check_output(stdout: &[u8], stderr: &[u8], success: bool) -> Result<(), &'static str> {
    let mut replied = false;
    let mut failed = !success;
    // Codex writes `error` also for a stream retry that can still succeed: it counts unless a
    // `turn.completed` follows; a fatal one ends with `turn.failed` and a non-zero exit.
    let mut codex_error = false;
    let mut detail = String::from_utf8_lossy(stderr).into_owned();
    for line in String::from_utf8_lossy(stdout).lines() {
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let text = |value: &Value| value.as_str().is_some_and(|text| !text.trim().is_empty());
        match event["type"].as_str().unwrap_or("") {
            // Claude Code
            "result" if event["is_error"] == true => {
                failed = true;
                detail.push_str(&event["result"].to_string());
            }
            "result" => replied |= text(&event["result"]),
            // Codex
            "item.completed" if event["item"]["type"] == "agent_message" => {
                replied |= text(&event["item"]["text"]);
            }
            "turn.failed" => {
                failed = true;
                detail.push_str(&event.to_string());
            }
            "error" => {
                codex_error = true;
                detail.push_str(&event.to_string());
            }
            "turn.completed" => codex_error = false,
            _ => {}
        }
    }
    if failed || codex_error || !replied {
        return Err(failure_message(&detail));
    }
    Ok(())
}

pub async fn claude(State(app): State<App>, user: User, Json(draft): Json<AgentConfig>) -> ApiResult<Json<Value>> {
    check(app, user, "claude", draft).await
}
pub async fn codex(State(app): State<App>, user: User, Json(draft): Json<AgentConfig>) -> ApiResult<Json<Value>> {
    check(app, user, "codex", draft).await
}
async fn check(app: App, user: User, runtime: &str, draft: AgentConfig) -> ApiResult<Json<Value>> {
    let mut settings = app.store.settings(user.id)?;
    let saved = settings.agents.get(runtime).cloned().unwrap_or_default();
    let config = draft_config(draft, &saved)?;
    if settings
        .connections
        .get("servicenow")
        .is_some_and(|sn| [&sn.password, &sn.client_secret, &sn.token].into_iter().any(|secret| !secret.is_empty() && secret == &config.api_key))
    {
        return Err(bad(
            "Una credenziale ServiceNow è finita nei campi di un agente. Salvataggio bloccato: controlla l’autocompilazione del browser o del gestore password.",
        ));
    }
    settings.agents.insert(runtime.into(), config.clone());
    let home = agents::user_home(&app, user.id)?;
    let path = home
        .join("tmp")
        .join(format!("{runtime}-check-{}", crate::model::id()));
    std::fs::create_dir(&path)?;
    let workspace = ProbeWorkspace(path);
    crate::store::private(&workspace.0, true)?;
    agents::own(&app, user.id, &workspace.0)?;
    let mut command = agents::command(&app, user.id, runtime, &workspace.0, &settings)?;
    // The same flags as a run, read-only and without tools or MCP servers.
    if runtime == "claude" {
        command.args([
            "--print",
            "--verbose",
            "--output-format",
            "stream-json",
            "--strict-mcp-config",
            "--setting-sources",
            "project",
            "--tools",
            "",
        ]);
        if !config.model.is_empty() {
            command.args(["--model", &config.model]);
        }
        if ["low", "medium", "high", "xhigh", "max"].contains(&config.effort.as_str()) {
            command.args(["--effort", &config.effort]);
        }
    } else {
        command.args([
            "--no-daemon",
            "-a",
            "never",
            "--sandbox",
            "read-only",
            "exec",
            "--json",
            "--skip-git-repo-check",
        ]);
        if !config.model.is_empty() {
            command.args(["--model", &config.model]);
        }
        if ["minimal", "low", "medium", "high", "xhigh"].contains(&config.effort.as_str()) {
            command.args(["-c", &format!("model_reasoning_effort={:?}", config.effort)]);
        }
        command.arg("-");
    }
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|_| bad(START_FAILED))?;
    let _group = agents::ProcessGroup(child.id().ok_or_else(|| bad(START_FAILED))?);
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap().take(1_048_577);
    let mut stderr = child.stderr.take().unwrap().take(65537);
    let result = tokio::time::timeout(Duration::from_secs(90), async {
        // A CLI that exits before reading the prompt still reports its own error.
        let _ = stdin.write_all(PROMPT.as_bytes()).await;
        drop(stdin);
        let mut out = Vec::new();
        let mut err = Vec::new();
        let (status, read_out, read_err) = tokio::join!(child.wait(), stdout.read_to_end(&mut out), stderr.read_to_end(&mut err));
        read_out?;
        read_err?;
        Ok::<_, std::io::Error>((status?, out, err))
    })
    .await
    .map_err(|_| bad("La verifica ha superato il tempo massimo. Riprova tra poco."))?
    .map_err(|_| bad(FAILED))?;
    if result.1.len() > 1_048_576 || result.2.len() > 65536 {
        return Err(bad("Risposta di verifica non valida."));
    }
    check_output(&result.1, &result.2, result.0.success()).map_err(bad)?;
    Ok(Json(
        json!({"connected":true,"message":"Connessione riuscita. Il modello ha risposto correttamente."}),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn user() -> User {
        User { id: 1, email: "check@test.local".into() }
    }
    #[tokio::test]
    async fn checks_use_the_form_values_without_saving_and_never_expose_output() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let app = crate::tests::app(dir.path());
        app.store
            .update_settings(1, |settings| {
                for runtime in ["claude", "codex"] {
                    settings.agents.get_mut(runtime).unwrap().api_key = "saved-key".into();
                }
                Ok(())
            })
            .unwrap();
        let original = serde_json::to_value(app.store.settings(1).unwrap()).unwrap();
        let tools = crate::distribution::tools(dir.path());
        std::fs::create_dir_all(&tools).unwrap();
        let install = |name: &str, script: &str| {
            let binary = tools.join(name);
            std::fs::write(&binary, script).unwrap();
            std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        };
        // Each fixture checks its key, model, read-only flags and the prompt on stdin.
        install("claude", r##"#!/bin/sh
read -r EXPECTED_KEY < "${0%/*}/expected-key"
[ "$ANTHROPIC_API_KEY" = "$EXPECTED_KEY" ] || { echo '{"type":"result","is_error":true,"result":"Invalid API key"}'; exit 1; }
case " $* " in *" --print "*" --tools  --model draft-model --effort high "*) ;; *) exit 10 ;; esac
read -r PROMPT
[ "$PROMPT" = "Connection check. Reply with OK only." ] || exit 11
printf '%s\n' '{"type":"system","session_id":"401-429"}' '{"type":"result","is_error":false,"result":"OK"}'
"##);
        install("codex", r##"#!/bin/sh
read -r EXPECTED_KEY < "${0%/*}/expected-key"
[ "$CODEX_API_KEY" = "$EXPECTED_KEY" ] || { echo '{"type":"error","message":"401 Unauthorized secret-header"}'; exit 1; }
case " $* " in *" --sandbox read-only exec --json --skip-git-repo-check --model draft-model "*" - "*) ;; *) exit 10 ;; esac
read -r PROMPT
[ "$PROMPT" = "Connection check. Reply with OK only." ] || exit 11
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"OK"}}'
"##);
        let draft = |api_key: &str| AgentConfig {
            model: " draft-model ".into(),
            effort: "high".into(),
            api_key: api_key.into(),
            ..Default::default()
        };
        type Check = fn(State<App>, User, Json<AgentConfig>) -> std::pin::Pin<Box<dyn Future<Output = ApiResult<Json<Value>>> + Send>>;
        let checks: [(&str, Check); 2] = [
            ("claude", |a, u, d| Box::pin(claude(a, u, d))),
            ("codex", |a, u, d| Box::pin(codex(a, u, d))),
        ];
        for (name, run) in checks {
            let expect = |key: &str| std::fs::write(tools.join("expected-key"), format!("{key}\n")).unwrap();
            expect("draft-key");
            assert_eq!(run(State(app.clone()), user(), Json(draft("draft-key"))).await.unwrap().0["connected"], true, "{name}");
            // An empty field checks the saved key; a wrong one fails without exposing output.
            let error = run(State(app.clone()), user(), Json(draft(""))).await.unwrap_err();
            assert!(error.1.contains("Autenticazione fallita"), "{name}: {}", error.1);
            assert!(!error.1.contains("secret-header") && !error.1.contains("Invalid API key"));
            expect("saved-key");
            assert_eq!(run(State(app.clone()), user(), Json(draft(""))).await.unwrap().0["connected"], true, "{name}");
            assert_eq!(serde_json::to_value(app.store.settings(1).unwrap()).unwrap(), original);
        }
        // Every probe folder is removed.
        let leftovers = std::fs::read_dir(agents::user_home(&app, 1).unwrap().join("tmp")).unwrap().count();
        assert_eq!(leftovers, 0);
    }
    #[test]
    fn output_needs_a_reply_and_explains_failures_without_quoting_them() {
        let claude_ok = br#"{"type":"result","is_error":false,"result":"OK"}"#;
        let codex_ok = br#"{"type":"item.completed","item":{"type":"agent_message","text":"OK"}}"#;
        assert!(check_output(claude_ok, b"", true).is_ok());
        assert!(check_output(codex_ok, b"", true).is_ok());
        assert_eq!(check_output(claude_ok, b"", false), Err(FAILED));
        assert_eq!(check_output(b"{}", b"", true), Err(FAILED));
        // Numbers in ordinary events, such as identifiers, are not read as error codes.
        assert_eq!(check_output(br#"{"type":"system","session_id":"401-429"}"#, b"", true), Err(FAILED));
        // A Codex retry notice followed by a completed turn is a working connection; an error with
        // no completion after it, or followed by turn.failed, is not.
        let retry = r#"{"type":"error","message":"Reconnecting... 1/5 (stream disconnected before completion)"}"#;
        let completed = r#"{"type":"turn.completed","usage":{"input_tokens":1,"output_tokens":1}}"#;
        let codex_ok = std::str::from_utf8(codex_ok).unwrap();
        assert!(check_output(format!("{retry}\n{codex_ok}\n{completed}").as_bytes(), b"", true).is_ok());
        assert_eq!(check_output(format!("{retry}\n{codex_ok}").as_bytes(), b"", true), Err(FAILED));
        let failed = r#"{"type":"turn.failed","error":{"message":"exceeded retry limit, last status: 401 Unauthorized"}}"#;
        assert!(check_output(format!("{retry}\n{failed}").as_bytes(), b"", false).unwrap_err().contains("Autenticazione"));
        assert_eq!(check_output(format!("{retry}\n{codex_ok}\n{completed}").as_bytes(), b"", false), Err(FAILED));
        let limit = check_output(br#"{"type":"turn.failed","error":{"message":"You hit your usage limit sk-secret"}}"#, b"", true).unwrap_err();
        assert!(limit.contains("limite") && !limit.contains("sk-secret"));
        let model = check_output(br#"{"type":"result","is_error":true,"result":"The model nope does not exist"}"#, b"", false).unwrap_err();
        assert!(model.contains("modello non è disponibile"));
        assert!(check_output(b"", b"Not logged in. Please run /login", false).unwrap_err().contains("Autenticazione"));
    }
    #[test]
    fn drafts_keep_the_saved_key_unless_replaced_or_cleared() {
        let saved = AgentConfig { api_key: "saved-key".into(), model: "saved-model".into(), ..Default::default() };
        let draft = |api_key: &str| AgentConfig { model: " other ".into(), effort: "low".into(), api_key: api_key.into(), ..Default::default() };
        let config = draft_config(draft(""), &saved).unwrap();
        assert_eq!((config.model.as_str(), config.effort.as_str(), config.api_key.as_str()), ("other", "low", "saved-key"));
        assert_eq!(draft_config(draft("typed"), &saved).unwrap().api_key, "typed");
        assert!(draft_config(draft("__CLEAR__"), &saved).unwrap().api_key.is_empty());
        assert!(draft_config(draft(&"k".repeat(4097)), &saved).is_err());
    }
}
