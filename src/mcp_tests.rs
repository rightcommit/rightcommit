use super::*;
use crate::model::{Edge, McpAuth, McpConfig};
use axum::routing::{get, post};
use std::sync::atomic::{AtomicUsize, Ordering};

const STDIO: &str = r#"
import json,sys,os
for line in sys.stdin:
 try:
  q=json.loads(line); method=q.get('method'); result={}
  if 'id' not in q:continue
  if method=='initialize':result={'protocolVersion':q['params']['protocolVersion'],'capabilities':{'tools':{},'resources':{},'prompts':{}},'serverInfo':{'name':'fixture','version':'1'}}
  elif method=='tools/list':result={'tools':[{'name':'echo','description':'Echo the provided value','inputSchema':{'type':'object','properties':{'value':{'type':'string'}}}},{'name':'forbidden','inputSchema':{'type':'object','properties':{}}}]}
  elif method=='tools/call':result={'content':[{'type':'text','text':q['params'].get('arguments',{}).get('value','')+'|'+os.getcwd()+'|'+os.environ.get('RIGHTCOMMIT_TEST_MCP_TOKEN','absent')}]}
  elif method=='resources/list':result={'resources':[{'uri':'fixture://document','name':'Document'}]}
  elif method=='resources/templates/list':result={'resourceTemplates':[]}
  elif method=='resources/read':result={'contents':[{'uri':q['params']['uri'],'text':'Resource content'}]}
  elif method=='prompts/list':result={'prompts':[{'name':'review','description':'Review a document'}]}
  elif method=='prompts/get':result={'messages':[{'role':'user','content':{'type':'text','text':'Review this document'}}]}
  print(json.dumps({'jsonrpc':'2.0','id':q['id'],'result':result}),flush=True)
 except Exception:sys.exit(2)
"#;
pub(crate) async fn fixture_app(dir: &std::path::Path) -> (App, Run) {
    let app = crate::tests::app(&std::fs::canonicalize(dir).unwrap());
    let script = dir.join("mcp.py");
    std::fs::write(&script, STDIO).unwrap();
    let c = McpConfig {
        name: "Fixture".into(),
        transport: "stdio".into(),
        command: "python3".into(),
        args: vec!["-u".into(), script.to_string_lossy().into()],
        ..Default::default()
    };
    let mut settings = Settings::default();
    settings.mcp_servers.insert("test".into(), c);
    app.store.save_settings(1, &settings).unwrap();
    let mut w = crate::presets::all().remove(0);
    let a = w.node(&w.entry).unwrap().agent.clone();
    w.nodes.push(Node {
        id: "mcp".into(),
        kind: "MCP".into(),
        title: "MCP".into(),
        mcp_id: "test".into(),
        mcp_tools: vec!["echo".into()],
        ..Default::default()
    });
    w.edges.push(Edge {
        from: "mcp".into(),
        to: a,
        kind: "context".into(),
        outcome: String::new(),
    });
    let r = crate::engine::create(&app, 1, w, String::new(), None)
        .await
        .unwrap();
    (app, r)
}
#[tokio::test]
async fn stdio_client_tool_scopes_sessions_resources_and_revocation() {
    let dir = tempfile::tempdir().unwrap();
    let (app, mut run) = fixture_app(dir.path()).await;
    let prepared = prepare(&app, 1, &run).await.unwrap().unwrap();
    assert_eq!(prepared.tools.len(), 1);
    assert!(prepared.tools[0].name.starts_with("nodo_"));
    let again = prepare(&app, 1, &run).await.unwrap().unwrap();
    let (g, g2) = {
        let grants = app.mcp.grants.lock().unwrap();
        (
            grants[&crate::store::hash(prepared.token())].clone(),
            grants[&crate::store::hash(again.token())].clone(),
        )
    };
    assert!(Arc::ptr_eq(&g.servers[0].session, &g2.servers[0].session));
    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        format!("Bearer {}", prepared.token()).parse().unwrap(),
    );
    assert!(grant(&app, &headers).is_err());
    app.busy.lock().unwrap().insert(run.id.clone());
    assert!(grant(&app, &headers).is_ok());
    let result = dispatch(
        &app,
        &g,
        Request {
            method: "tools/call".into(),
            params: json!({"name":prepared.tools[0].name,"arguments":{"value":"actual result"}}),
        },
    )
    .await
    .unwrap();
    assert!(
        result["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains(&run.workspace)
    );
    assert!(
        dispatch(
            &app,
            &g,
            Request {
                method: "tools/call".into(),
                params: json!({"name":alias("mcp","forbidden")})
            }
        )
        .await
        .is_err()
    );
    let events = app.store.events(1, &run.id, 0).unwrap();
    assert_eq!(events.iter().filter(|e| e.kind == "mcp_tool").count(), 2);
    assert!(events.iter().all(|e| !e.text.contains("actual result")));
    let server = &g.servers[0].session;
    assert!(
        metadata(server, "@resources/list", json!({}))
            .await
            .unwrap()
            .to_string()
            .contains("fixture://document")
    );
    assert!(
        metadata(
            server,
            "@resources/read",
            json!({"uri":"fixture://document"})
        )
        .await
        .unwrap()
        .to_string()
        .contains("Resource content")
    );
    assert!(
        metadata(server, "@prompts/get", json!({"name":"review"}))
            .await
            .unwrap()
            .to_string()
            .contains("Review this document")
    );
    drop(prepared);
    assert!(grant(&app, &headers).is_err());
    run.workflow.node("mcp").unwrap();
    assert!(prepare(&app, 2, &run).await.is_err());
    run.workflow
        .nodes
        .iter_mut()
        .find(|n| n.id == "mcp")
        .unwrap()
        .mcp_tools
        .clear();
    let all = prepare(&app, 1, &run).await.unwrap().unwrap();
    assert_eq!(all.tools.len(), 6);
    assert!(crate::engine::validate(&run.workflow).is_ok());
    run.workflow.entry = "mcp".into();
    assert!(crate::engine::validate(&run.workflow).is_err());
}
#[tokio::test]
async fn errors_handed_to_an_agent_are_english() {
    let dir = tempfile::tempdir().unwrap();
    let (app, run) = fixture_app(dir.path()).await;
    let prepared = prepare(&app, 1, &run).await.unwrap().unwrap();
    let bearer = |token: &str| {
        let mut h = HeaderMap::new();
        h.insert("authorization", format!("Bearer {token}").parse().unwrap());
        h
    };
    let call = |name: &str| Request {
        method: "tools/call".into(),
        params: json!({"name":name}),
    };
    // Why a grant is refused stays on the server; the agent gets one English answer.
    let error = |r: Result<Grant>| r.err().unwrap().to_string();
    assert_eq!(error(grant(&app, &HeaderMap::new())), "MCP credential missing");
    assert_eq!(error(grant(&app, &bearer(&"x".repeat(200)))), "Invalid MCP credential");
    assert_eq!(error(grant(&app, &bearer("unknown"))), "MCP credential revoked");
    assert_eq!(error(grant(&app, &bearer(prepared.token()))), "MCP agent turn not active");
    let Err(ApiError(status, message)) =
        agent(State(app.clone()), bearer(prepared.token()), Json(call("anything"))).await
    else {
        panic!("an inactive turn must be refused")
    };
    assert_eq!((status, message.as_str()), (StatusCode::UNAUTHORIZED, MCP_ACCESS_ERROR));
    assert_eq!(MCP_ACCESS_ERROR, "MCP access not valid or expired");
    app.busy.lock().unwrap().insert(run.id.clone());
    let g = grant(&app, &bearer(prepared.token())).unwrap();
    let Err(ApiError(status, message)) =
        agent(State(app.clone()), bearer(prepared.token()), Json(call(&alias("mcp", "forbidden")))).await
    else {
        panic!("a tool outside the grant must be refused")
    };
    assert_eq!((status, message.as_str()), (StatusCode::BAD_REQUEST, MCP_REQUEST_ERROR));
    assert_eq!(
        MCP_REQUEST_ERROR,
        "MCP request failed. Check the connection, the allowed tools and the timeout; no automatic retry."
    );
    let error = |r: Result<Value>| r.unwrap_err().to_string();
    assert_eq!(
        error(dispatch(&app, &g, call(&alias("mcp", "forbidden"))).await),
        "MCP tool not assigned to the agent"
    );
    assert_eq!(
        error(dispatch(&app, &g, Request { method: "tools/call".into(), params: json!({}) }).await),
        "Tool name missing"
    );
    assert_eq!(
        error(dispatch(&app, &g, Request { method: "resources/list".into(), params: json!({}) }).await),
        "Unsupported MCP method"
    );
    assert_eq!(
        error(metadata(&g.servers[0].session, "@unknown", json!({})).await),
        "Invalid MCP metadata method"
    );
}
#[test]
fn server_variables_are_named_checked_against_the_command_and_resolved_only_for_the_administrator() {
    let dir = tempfile::tempdir().unwrap();
    let app = crate::tests::app(dir.path());
    let settings = Settings::default();
    let home = agents::user_home(&app, 1).unwrap();
    let stdio = |names: &[&str]| McpConfig {
        name: "Service".into(),
        transport: "stdio".into(),
        command: "python3".into(),
        env_from_server: names.iter().map(|n| n.to_string()).collect(),
        ..Default::default()
    };
    let cfg = stdio(&["MY_SERVICE_TOKEN", "MY_SERVICE_URL"]);
    validate(&cfg).unwrap();
    let command = agents::command(&app, 1, &cfg.command, &home, &settings).unwrap();
    let lookup = |name: &str| match name {
        "MY_SERVICE_TOKEN" => Some("test-only-service-token-value".to_string()),
        "MY_SERVICE_URL" => Some("https://service.test".to_string()),
        "EMPTY_VALUE" => Some(String::new()),
        _ => None,
    };
    assert_eq!(
        resolve_server_variables(1, &cfg, &command, lookup).unwrap(),
        vec![("MY_SERVICE_TOKEN".to_string(), "test-only-service-token-value".to_string()), ("MY_SERVICE_URL".to_string(), "https://service.test".to_string())]
    );
    // A missing or empty variable stops the launch with its name, never with another value.
    for name in ["NOT_SET_ANYWHERE", "EMPTY_VALUE"] {
        let error = resolve_server_variables(1, &stdio(&["MY_SERVICE_TOKEN", name]), &command, lookup).unwrap_err().to_string();
        assert!(error.contains(name) && !error.contains("test-only-service-token-value"), "{error}");
    }
    // Only the first administrator, whatever the lookup holds.
    let other = agents::command(&app, 2, &cfg.command, &agents::user_home(&app, 2).unwrap(), &settings).unwrap();
    assert!(resolve_server_variables(2, &cfg, &other, lookup).unwrap_err().to_string().contains("ID 1"));
    assert!(check_server_variables(2, &stdio(&[]), &other).is_ok());
    // Names rightcommit sets for the command, and their families, are refused.
    for name in ["HOME", "PATH", "USER", "LANG", "TMPDIR", "PWD", "LC_ALL", "XDG_DATA_HOME", "GIT_DIR", "CARGO_TARGET_DIR", "RUSTUP_TOOLCHAIN", "CODEX_HOME", "CLAUDE_CODE_OAUTH_TOKEN", "PI_LOCAL_API_KEY", "SPECKIT_FLAG"] {
        let c = stdio(&[name]);
        validate(&c).unwrap();
        let error = check_server_variables(1, &c, &command).unwrap_err().to_string();
        assert!(error.contains(name), "{name}: {error}");
    }
    // An agent CLI started as an MCP server keeps its own provider rules.
    let mut codex = stdio(&["OPENAI_API_KEY"]);
    check_server_variables(1, &codex, &command).unwrap();
    codex.command = "codex".into();
    let codex_command = agents::command(&app, 1, "codex", &home, &settings).unwrap();
    assert!(check_server_variables(1, &codex, &codex_command).is_err());
    // Names are shell identifiers, unique, not also literal, capped, and stdio only.
    for bad in [&["1TOKEN"][..], &["MY-TOKEN"], &[""], &["LD_PRELOAD"], &["DYLD_INSERT_LIBRARIES"], &["SAME", "SAME"]] {
        assert!(validate(&stdio(bad)).is_err(), "{bad:?}");
    }
    let mut both = stdio(&["MY_SERVICE_URL"]);
    both.env.insert("MY_SERVICE_URL".into(), "https://literal.test".into());
    assert!(validate(&both).unwrap_err().to_string().contains("MY_SERVICE_URL"));
    let many: Vec<String> = (0..=MAX_SERVER_VARIABLES).map(|i| format!("VARIABLE_{i}")).collect();
    assert!(validate(&stdio(&many.iter().map(String::as_str).collect::<Vec<_>>())).is_err());
    let mut http = stdio(&["MY_SERVICE_URL"]);
    http.transport = "http".into();
    http.url = "https://mcp.example/mcp".into();
    assert!(validate(&http).is_err());
    // Settings saved before the field existed keep loading.
    let legacy: McpConfig = serde_json::from_value(json!({"name":"Old","transport":"stdio","command":"echo"})).unwrap();
    assert!(legacy.env_from_server.is_empty());
    // The public configuration lists the names only.
    assert_eq!(public_config(&cfg)["env_from_server"], json!(["MY_SERVICE_TOKEN", "MY_SERVICE_URL"]));
}
/// Launch (a run's MCP servers and Test connection) reads the named variables from the
/// process environment, so this test checks it in a child run of itself only.
#[tokio::test]
async fn stdio_server_receives_named_server_variables_at_launch_and_values_stay_hidden() {
    const NAME: &str = "mcp_client::tests::stdio_server_receives_named_server_variables_at_launch_and_values_stay_hidden";
    const VALUE: &str = "test-only-mcp-server-variable";
    if std::env::var("RIGHTCOMMIT_MCP_ENV_CHILD").is_err() {
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--test-threads=1"])
            .env("RIGHTCOMMIT_MCP_ENV_CHILD", "1")
            .env("RIGHTCOMMIT_TEST_MCP_TOKEN", VALUE)
            .env("RIGHTCOMMIT_TEST_MCP_EMPTY", "")
            .env_remove("RIGHTCOMMIT_TEST_MCP_MISSING")
            .output()
            .unwrap();
        let report = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success() && report.contains("1 passed"), "child test failed: {report}");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let (app, run) = fixture_app(dir.path()).await;
    let set = |uid: i64, names: &[&str]| {
        let mut settings = app.store.settings(1).unwrap();
        settings.mcp_servers.get_mut("test").unwrap().env_from_server = names.iter().map(|n| n.to_string()).collect();
        // Written straight to the store: launch must enforce the rules on its own.
        app.store.save_settings(uid, &settings).unwrap();
    };
    // Without a reference the server never sees the variable.
    let prepared = prepare(&app, 1, &run).await.unwrap().unwrap();
    let call = |prepared: &Prepared| {
        let g = app.mcp.grants.lock().unwrap()[&crate::store::hash(prepared.token())].clone();
        let name = prepared.tools[0].name.to_string();
        let app = app.clone();
        async move { dispatch(&app, &g, Request { method: "tools/call".into(), params: json!({"name":name,"arguments":{"value":"seen"}}) }).await.unwrap() }
    };
    app.busy.lock().unwrap().insert(run.id.clone());
    assert!(call(&prepared).await.to_string().contains("|absent"));
    assert_eq!(prepared.server_values().count(), 0);
    drop(prepared);
    set(1, &["RIGHTCOMMIT_TEST_MCP_TOKEN"]);
    let prepared = prepare(&app, 1, &run).await.unwrap().unwrap();
    let result = call(&prepared).await.to_string();
    assert!(result.contains(&format!("|{VALUE}")), "{result}");
    // Run evidence passes through the turn's redaction, which hides the value.
    assert_eq!(agents::redact_values(&result, prepared.server_values()).contains(VALUE), false);
    assert!(app.store.events(1, &run.id, 0).unwrap().iter().all(|e| !e.text.contains(VALUE)));
    drop(prepared);
    // Test connection uses the same launch, and its answer carries no value.
    let user = |id| User { id, email: String::new() };
    let probed = probe(State(app.clone()), user(1), Path("test".into())).await.unwrap().0;
    assert!(!probed.to_string().contains(VALUE));
    let settings = crate::api::public_settings(&app, &user(1), &app.store.settings(1).unwrap());
    assert!(settings.to_string().contains("RIGHTCOMMIT_TEST_MCP_TOKEN") && !settings.to_string().contains(VALUE));
    for missing in ["RIGHTCOMMIT_TEST_MCP_MISSING", "RIGHTCOMMIT_TEST_MCP_EMPTY"] {
        set(1, &["RIGHTCOMMIT_TEST_MCP_TOKEN", missing]);
        let error = probe(State(app.clone()), user(1), Path("test".into())).await.unwrap_err().1;
        assert!(error.contains(missing) && !error.contains(VALUE), "{error}");
        assert!(prepare(&app, 1, &run).await.is_err());
    }
    // Another account cannot obtain a server variable by writing a reference.
    set(2, &["RIGHTCOMMIT_TEST_MCP_TOKEN"]);
    let error = probe(State(app.clone()), user(2), Path("test".into())).await.unwrap_err().1;
    assert!(error.contains("ID 1") && !error.contains(VALUE), "{error}");
}
#[test]
fn secrets_are_masked_merged_and_redacted_and_auth_modes_validate() {
    let c = McpConfig {
        url: "https://mcp.example/mcp".into(),
        headers: BTreeMap::from([("X-Secret".into(), "header-secret".into())]),
        env: BTreeMap::from([("TOKEN".into(), "env-secret".into())]),
        auth: McpAuth {
            mode: "bearer".into(),
            token: "token-secret".into(),
            password: "password-secret".into(),
            client_secret: "client-secret".into(),
            access_token: "access-secret".into(),
            refresh_token: "refresh-secret".into(),
            ..Default::default()
        },
        ..Default::default()
    };
    let public = public_config(&c);
    for secret in [
        "header-secret",
        "env-secret",
        "token-secret",
        "password-secret",
        "client-secret",
        "access-secret",
        "refresh-secret",
    ] {
        assert!(!public.to_string().contains(secret));
    }
    let mut submitted: McpConfig = serde_json::from_value(public).unwrap();
    merge(&mut submitted, &c);
    assert_eq!(submitted.auth.token, "token-secret");
    assert_eq!(submitted.env["TOKEN"], "env-secret");
    assert_eq!(submitted.headers["X-Secret"], "header-secret");
    submitted.auth.token = "__CLEAR__".into();
    merge(&mut submitted, &c);
    assert!(submitted.auth.token.is_empty());
    let mut settings = Settings::default();
    settings.mcp_servers.insert("test".into(), c.clone());
    assert_eq!(
        agents::redact(
            "token-secret header-secret env-secret access-secret refresh-secret password-secret client-secret",
            &settings
        ),
        "[redacted] [redacted] [redacted] [redacted] [redacted] [redacted] [redacted]"
    );
    for mode in ["none", "bearer", "basic", "api_key"] {
        let mut cfg = c.clone();
        cfg.auth.mode = mode.into();
        validate(&cfg).unwrap();
    }
    let mut bad = c.clone();
    bad.env
        .insert("LD_PRELOAD".into(), "/tmp/untrusted.so".into());
    assert!(validate(&bad).is_err());
    assert!(crate::mcp_auth::url("http://external.example/mcp").is_err());
    assert!(crate::mcp_auth::url("https://name:secret@example.com/mcp").is_err());
}
#[derive(Clone)]
struct HttpFixture {
    calls: Arc<AtomicUsize>,
    tokens: Arc<Mutex<Vec<Value>>>,
}
async fn rpc(
    State(f): State<HttpFixture>,
    headers: HeaderMap,
    Json(q): Json<Value>,
) -> (StatusCode, Json<Value>) {
    let authorization = headers.get("authorization").and_then(|v| v.to_str().ok());
    let api = headers.get("x-api-key").and_then(|v| v.to_str().ok());
    if authorization != Some("Bearer fixture-token")
        && authorization != Some("Basic dXNlcjpwYXNz")
        && api != Some("api-secret")
    {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"unauthorized"})),
        );
    }
    if q.get("id").is_none() {
        return (StatusCode::ACCEPTED, Json(Value::Null));
    }
    let result = match q["method"].as_str().unwrap_or("") {
        "initialize" => {
            json!({"protocolVersion":q["params"]["protocolVersion"],"capabilities":{"tools":{}},"serverInfo":{"name":"http-fixture","version":"1"}})
        }
        "tools/list" => {
            json!({"tools":[{"name":"count","inputSchema":{"type":"object","properties":{}}}]})
        }
        "tools/call" => {
            f.calls.fetch_add(1, Ordering::SeqCst);
            json!({"content":[{"type":"text","text":"counted"}]})
        }
        _ => json!({}),
    };
    (
        StatusCode::OK,
        Json(json!({"jsonrpc":"2.0","id":q["id"],"result":result})),
    )
}
async fn token(
    State(f): State<HttpFixture>,
    axum::extract::Form(form): axum::extract::Form<BTreeMap<String, String>>,
) -> Json<Value> {
    f.tokens.lock().unwrap().push(json!(form));
    Json(
        json!({"access_token":"fixture-token","refresh_token":"refresh-rotated","expires_in":3600,"token_type":"Bearer"}),
    )
}
async fn http_fixture() -> (String, HttpFixture, tokio::task::JoinHandle<()>) {
    let f = HttpFixture {
        calls: Arc::new(AtomicUsize::new(0)),
        tokens: Arc::default(),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let router = axum::Router::new()
        .route("/mcp", post(rpc))
        .route("/token", post(token))
        .route("/authorize", get(|| async { "fixture" }))
        .with_state(f.clone());
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (base, f, task)
}
#[tokio::test]
async fn http_bearer_api_key_basic_and_oauth_client_credentials_refresh() {
    let (base, f, task) = http_fixture().await;
    let dir = tempfile::tempdir().unwrap();
    let app = crate::tests::app(dir.path());
    for mode in ["bearer", "api_key", "basic", "oauth_client_credentials"] {
        let c = McpConfig {
            url: format!("{base}/mcp"),
            auth: McpAuth {
                mode: mode.into(),
                token: if mode == "api_key" {
                    "api-secret"
                } else {
                    "fixture-token"
                }
                .into(),
                username: "user".into(),
                password: "pass".into(),
                client_id: "test-client".into(),
                client_secret: "client-secret".into(),
                token_auth: "post".into(),
                token_url: format!("{base}/token"),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut s = Settings::default();
        s.mcp_servers.insert("test".into(), c);
        app.store.save_settings(1, &s).unwrap();
        let (c, h) = crate::mcp_auth::headers(&app, 1, "test").await.unwrap();
        let session = connect(&app, 1, &c, h, dir.path()).await.unwrap();
        assert_eq!(session.service.list_all_tools().await.unwrap().len(), 1);
        let result = session
            .service
            .call_tool(CallToolRequestParams::new("count"))
            .await
            .unwrap();
        assert!(!result.is_error.unwrap_or(false));
    }
    assert_eq!(f.calls.load(Ordering::SeqCst), 4);
    let mut s = app.store.settings(1).unwrap();
    let c = s.mcp_servers.get_mut("test").unwrap();
    c.auth.expires = now() - 1;
    c.auth.refresh_token = "old-refresh".into();
    app.store.save_settings(1, &s).unwrap();
    crate::mcp_auth::headers(&app, 1, "test").await.unwrap();
    {
        let tokens = f.tokens.lock().unwrap();
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0]["grant_type"], "client_credentials");
        assert_eq!(tokens[1]["grant_type"], "refresh_token");
        assert_eq!(tokens[1]["refresh_token"], "old-refresh");
    }
    assert_eq!(
        app.store.settings(1).unwrap().mcp_servers["test"]
            .auth
            .refresh_token,
        "refresh-rotated"
    );
    assert!(crate::mcp_auth::headers(&app, 2, "test").await.is_err());
    task.abort();
}
#[tokio::test]
async fn oauth_pkce_state_user_scope_single_use_and_token_exchange() {
    let (base, f, task) = http_fixture().await;
    let dir = tempfile::tempdir().unwrap();
    let app = crate::tests::app(dir.path());
    let c = McpConfig {
        url: format!("{base}/mcp"),
        auth: McpAuth {
            mode: "oauth_pkce".into(),
            client_id: "public-client".into(),
            authorization_url: format!("{base}/authorize"),
            token_url: format!("{base}/token"),
            ..Default::default()
        },
        ..Default::default()
    };
    let mut s = Settings::default();
    s.mcp_servers.insert("test".into(), c);
    app.store.save_settings(1, &s).unwrap();
    let user = || User {
        id: 1,
        email: "test@example.invalid".into(),
    };
    let mut headers = HeaderMap::new();
    headers.insert("host", "127.0.0.1:8080".parse().unwrap());
    let start = crate::mcp_auth::start(
        State(app.clone()),
        user(),
        Path("test".into()),
        headers,
        Json(crate::mcp_auth::Start {
            redirect_uri: "http://127.0.0.1:8080/mcp/callback".into(),
        }),
    )
    .await
    .unwrap()
    .0;
    let auth = reqwest::Url::parse(start["url"].as_str().unwrap()).unwrap();
    let params: HashMap<_, _> = auth.query_pairs().into_owned().collect();
    assert_eq!(params["code_challenge_method"], "S256");
    assert!(params["code_challenge"].len() > 40);
    let cb = || crate::mcp_auth::Callback {
        state: params["state"].clone(),
        code: "fixture-code".into(),
    };
    assert!(
        crate::mcp_auth::callback(
            State(app.clone()),
            User {
                id: 2,
                email: String::new()
            },
            Json(cb())
        )
        .await
        .is_err()
    );
    assert!(
        crate::mcp_auth::callback(State(app.clone()), user(), Json(cb()))
            .await
            .is_ok()
    );
    assert!(
        crate::mcp_auth::callback(State(app.clone()), user(), Json(cb()))
            .await
            .is_err()
    );
    let tokens = f.tokens.lock().unwrap();
    assert_eq!(tokens.len(), 1);
    assert!(tokens[0]["code_verifier"].as_str().unwrap().len() >= 43);
    assert_eq!(tokens[0]["client_id"], "public-client");
    drop(tokens);
    let public = public_config(&app.store.settings(1).unwrap().mcp_servers["test"]);
    assert_eq!(public["auth"]["connected"], true);
    assert_eq!(public["auth"]["access_token"], "");
    task.abort();
}
