//! Public OpenRouter prices, used only as an API-equivalent estimate.
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::OnceLock};

#[derive(Clone, Default)]
pub struct Catalog {
    pub at: i64,
    pub prices: BTreeMap<String, Value>,
    pub stale: bool,
    pub persisted: bool,
}
fn rate(v: &Value) -> Option<f64> {
    v.as_str()
        .and_then(|s| s.parse().ok())
        .or_else(|| v.as_f64())
        .filter(|n: &f64| n.is_finite() && *n >= 0.)
}
impl Catalog {
    pub fn parse(v: &Value) -> Option<Self> {
        let mut result = Self {
            at: crate::model::now(),
            ..Self::default()
        };
        for m in v["data"].as_array()? {
            let Some(id) = m["id"].as_str().filter(|s| s.starts_with("openai/")) else {
                continue;
            };
            let p = &m["pricing"];
            if rate(&p["prompt"]).is_some() && rate(&p["completion"]).is_some() {
                result.prices.insert(id.into(), p.clone());
            }
        }
        (!result.prices.is_empty()).then_some(result)
    }
    pub fn metadata(&self) -> Value {
        json!({"source":"https://openrouter.ai/api/v1/models","fetched_at":self.at,
            "available":!self.prices.is_empty(),"stale":self.stale,"persisted":self.persisted,"prices":self.prices,
            "basis":"current_catalog_api_equivalent"})
    }
    pub fn estimate(&self, u: &crate::finops::Usage) -> Option<f64> {
        if u.runtime != "codex" || u.usd.is_some() {
            return None;
        }
        // Exact IDs only. Do not guess prices for misspellings, aliases or other providers.
        let id = if u.model.starts_with("openai/") {
            u.model.clone()
        } else {
            format!("openai/{}", u.model)
        };
        let base = self.prices.get(&id)?;
        let mut p = base.clone();
        if let Some(overrides) = base.get("overrides") {
            for tier in overrides.as_array()? {
                // Unsupported time/provider pricing cannot safely be treated as a context tier.
                if tier.as_object()?.keys().any(|k| {
                    ![
                        "min_prompt_tokens",
                        "prompt",
                        "completion",
                        "input_cache_read",
                        "input_cache_write",
                    ]
                    .contains(&k.as_str())
                }) {
                    return None;
                }
                let min = tier["min_prompt_tokens"].as_u64()?;
                if u.source == "codex_native_history" && u.input > min {
                    for (key, value) in tier.as_object()? {
                        if key != "min_prompt_tokens" {
                            p[key] = value.clone();
                        }
                    }
                }
            }
        }
        let read = u.cache_read?;
        let write = u.cache_write.unwrap_or(0);
        let ordinary = u.input.checked_sub(read)?.checked_sub(write)?;
        let mut cost =
            ordinary as f64 * rate(&p["prompt"])? + u.output as f64 * rate(&p["completion"])?;
        if read > 0 {
            cost += read as f64 * rate(&p["input_cache_read"])?;
        }
        if write > 0 {
            cost += write as f64 * rate(&p["input_cache_write"])?;
        }
        cost.is_finite().then_some(cost)
    }
}

#[derive(Default)]
struct Cache {
    attempted_at: i64,
    catalog: Catalog,
}
impl Cache {
    fn load(store: &crate::store::Store) -> anyhow::Result<Self> {
        use rusqlite::OptionalExtension;
        let raw: Option<String> = store
            .db
            .lock()
            .unwrap()
            .query_row("SELECT json FROM finops_catalog WHERE id=1", [], |r| {
                r.get(0)
            })
            .optional()?;
        let Some(raw) = raw.filter(|s| s.len() <= 8 * 1024 * 1024) else {
            return Ok(Self::default());
        };
        let snapshot: Value = serde_json::from_str(&raw)?;
        anyhow::ensure!(snapshot["version"] == 1, "Formato cache tariffe non valido");
        let at = snapshot["fetched_at"].as_i64().unwrap_or(0);
        let mut catalog = if at == 0 {
            Catalog::default()
        } else {
            anyhow::ensure!(
                at > 0 && at <= crate::model::now() + 300,
                "Data cache tariffe non valida"
            );
            Catalog::parse(&snapshot).ok_or_else(|| anyhow::anyhow!("Cache tariffe non valida"))?
        };
        catalog.at = at; // Never replace the price date with the restart/load time.
        catalog.stale = snapshot["stale"].as_bool().unwrap_or(true);
        catalog.persisted = !catalog.prices.is_empty();
        Ok(Self {
            attempted_at: snapshot["attempted_at"].as_i64().unwrap_or(0),
            catalog,
        })
    }
    fn fresh(&self, now: i64) -> bool {
        self.attempted_at > 0 && now >= self.attempted_at && now - self.attempted_at < 300
    }
    fn update(&mut self, store: &crate::store::Store, now: i64, fetched: Option<Catalog>) {
        self.attempted_at = now;
        if let Some(catalog) = fetched {
            self.catalog = catalog;
        } else {
            self.catalog.stale = true;
        }
        let snapshot = json!({"version":1,"fetched_at":self.catalog.at,"attempted_at":now,"stale":self.catalog.stale,
            "data":self.catalog.prices.iter().map(|(id,pricing)|json!({"id":id,"pricing":pricing})).collect::<Vec<_>>()});
        // One atomic SQLite write preserves the last complete snapshot across restarts.
        let saved = store.db.lock().unwrap().execute(
            "INSERT INTO finops_catalog(id,json) VALUES(1,?) ON CONFLICT(id) DO UPDATE SET json=excluded.json",
            [snapshot.to_string()]
        );
        self.catalog.persisted = saved.is_ok() && !self.catalog.prices.is_empty();
        if let Err(error) = saved {
            tracing::warn!(%error, "Could not save the OpenRouter price cache");
        }
    }
}

pub async fn catalog(app: &crate::App) -> Catalog {
    static CACHE: OnceLock<tokio::sync::Mutex<BTreeMap<std::path::PathBuf, Cache>>> =
        OnceLock::new();
    let mut caches = CACHE
        .get_or_init(|| tokio::sync::Mutex::new(BTreeMap::new()))
        .lock()
        .await;
    let cache = caches.entry(app.data.clone()).or_insert_with(|| {
        Cache::load(&app.store).unwrap_or_else(|error| {
            tracing::warn!(%error, "OpenRouter price cache not available");
            Cache::default()
        })
    });
    let now = crate::model::now();
    if cache.fresh(now) {
        return cache.catalog.clone();
    }
    let fetched = async {
        let response = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(12))
            .build()
            .ok()?
            .get("https://openrouter.ai/api/v1/models")
            .send()
            .await
            .ok()?
            .error_for_status()
            .ok()?;
        let mut response = response;
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.ok()? {
            if bytes.len() + chunk.len() > 8 * 1024 * 1024 {
                return None;
            }
            bytes.extend_from_slice(&chunk);
        }
        Catalog::parse(&serde_json::from_slice::<Value>(&bytes).ok()?)
    }
    .await;
    cache.update(&app.store, now, fetched);
    cache.catalog.clone()
}

/// Only sessions explicitly retained by this user's run may be inspected.
pub fn history(
    r: &crate::model::Run,
    events: &[crate::model::Event],
    files: &[std::path::PathBuf],
    budget: &mut u64,
) -> (Vec<crate::finops::Usage>, usize) {
    use std::io::{BufRead, BufReader};
    let mut result = Vec::new();
    let mut missed = 0;
    for (binding, session) in &r.sessions {
        if session.runtime != "codex" {
            continue;
        }
        let sid = &session.native_id;
        if uuid::Uuid::parse_str(sid).is_err() {
            missed += 1;
            continue;
        }
        let suffix = format!("{sid}.jsonl");
        let Some(path) = files.iter().find(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().ends_with(&suffix))
        }) else {
            missed += 1;
            continue;
        };
        let Ok(meta) = path.metadata() else {
            missed += 1;
            continue;
        };
        if meta.len() > 32 * 1024 * 1024 || *budget + meta.len() > 128 * 1024 * 1024 {
            missed += 1;
            continue;
        }
        *budget += meta.len();
        let Ok(file) = std::fs::File::open(path) else {
            missed += 1;
            continue;
        };
        let mut valid = false;
        let mut failed = false;
        let mut model = session.model.clone();
        let mut turn = String::new();
        let mut seen = std::collections::HashSet::new();
        let mut rows = Vec::new();
        let mut open_turn = false;
        // The turns of the log (`task_started`): when each began and the rows written in it.
        let mut log_turns: Vec<(Option<i64>, Vec<usize>)> = Vec::new();
        for line in BufReader::new(file).lines() {
            let Ok(line) = line else {
                failed = true;
                break;
            };
            let Ok(v) = serde_json::from_str::<Value>(&line) else {
                failed = true;
                break;
            };
            let p = &v["payload"];
            if v["type"] == "session_meta" {
                // Compare paths, not strings: a direct source at the root of the sources
                // folder is saved with a trailing slash that Codex does not keep in its log.
                valid = p["id"].as_str() == Some(sid)
                    && p["cwd"]
                        .as_str()
                        .is_some_and(|cwd| std::path::Path::new(cwd) == std::path::Path::new(&r.workspace));
                if !valid {
                    failed = true;
                    break;
                }
            }
            if !valid {
                continue;
            }
            if v["type"] == "turn_context" {
                if let Some(m) = p["model"].as_str() {
                    model = m.into();
                }
                turn = p["turn_id"].as_str().unwrap_or("").into();
            }
            if v["type"] == "event_msg" && p["type"] == "task_started" {
                open_turn = true;
                log_turns.push((timestamp(&v), Vec::new()));
            }
            if v["type"] == "event_msg" && p["type"] == "task_complete" {
                open_turn = false;
            }
            if v["type"] != "event_msg" || p["type"] != "token_count" {
                continue;
            }
            let info = &p["info"];
            if info.is_null() {
                continue;
            }
            let usage = &info["last_token_usage"];
            let total = &info["total_token_usage"];
            if total.is_null()
                || !usage["input_tokens"].is_u64()
                || !usage["output_tokens"].is_u64()
            {
                failed = true;
                break;
            }
            // Codex can repeat the same token_count while publishing rate-limit updates.
            let key = serde_json::to_string(total).unwrap_or_default();
            if !seen.insert(key.clone()) {
                continue;
            }
            let Some(at) = timestamp(&v) else {
                failed = true;
                break;
            };
            if let Some((_, turn_rows)) = log_turns.last_mut() {
                turn_rows.push(rows.len());
            }
            let (node, attempt) = crate::finops::node_at(events, at);
            rows.push(crate::finops::Usage {
                id: crate::store::hash(format!("codex:{sid}:{turn}:{key}")),
                node,
                binding: binding.clone(),
                runtime: "codex".into(),
                model: model.clone(),
                attempt,
                at,
                input: usage["input_tokens"].as_u64().unwrap(),
                output: usage["output_tokens"].as_u64().unwrap(),
                cache_read: usage["cached_input_tokens"].as_u64(),
                cache_write: usage["cache_write_input_tokens"].as_u64(),
                reasoning: usage["reasoning_output_tokens"].as_u64(),
                usd: None,
                llm_ms: None,
                calls: Some(1),
                tools: 0,
                source: "codex_native_history".into(),
            });
        }
        if failed || !valid || open_turn || rows.is_empty() {
            missed += 1;
        } else {
            attribute_turns(binding, events, &log_turns, &mut rows);
            result.extend(rows);
        }
    }
    (result, missed)
}

fn timestamp(v: &Value) -> Option<i64> {
    v["timestamp"].as_str()?.parse::<jiff::Timestamp>().ok().map(|d| d.as_millisecond())
}

/// Gives every row of a log turn to the step that ran it. Run events have whole-second times, so
/// the last token count of a turn can carry a later millisecond time than the `running` event of
/// the next step started in the same second. Each `codex exec` turn writes one `task_started` and
/// one `agent_turn` event (its binding, node and attempt): when the binding's counts agree, the
/// turns are matched in order. Otherwise (a turn that failed before Codex started it, a manual
/// compaction, runs saved before `agent_turn`) the turn goes to the step running when it began,
/// which comes after that step's `running` event and before the next one.
fn attribute_turns(
    binding: &str,
    events: &[crate::model::Event],
    log_turns: &[(Option<i64>, Vec<usize>)],
    rows: &mut [crate::finops::Usage],
) {
    let run_turns: Vec<(String, u32)> = events
        .iter()
        .filter(|e| e.kind == "agent_turn")
        .filter_map(|e| serde_json::from_str::<Value>(&e.text).ok())
        .filter(|v| v["binding"] == binding && v["runtime"] == "codex")
        .map(|v| {
            let attempt = v["attempt"].as_u64().unwrap_or(0);
            (v["node"].as_str().unwrap_or("").to_owned(), u32::try_from(attempt).unwrap_or(u32::MAX))
        })
        .collect();
    let in_order = run_turns.len() == log_turns.len();
    for (i, (started, turn_rows)) in log_turns.iter().enumerate() {
        let Some(&first) = turn_rows.first() else {
            continue;
        };
        let (node, attempt) = if in_order {
            run_turns[i].clone()
        } else {
            crate::finops::node_at(events, started.unwrap_or(rows[first].at))
        };
        for &row in turn_rows {
            rows[row].node.clone_from(&node);
            rows[row].attempt = attempt;
        }
    }
}

/// The thread totals saved in a native session's log, read before a resumed turn: `turn.completed`
/// of `codex exec` reports the thread's running totals, which Codex restores from this log. A log
/// with no token count yet gives an empty object; a missing, ambiguous or foreign log gives None.
pub fn thread_total(home: &std::path::Path, sid: &str) -> Option<Value> {
    use std::io::{BufRead, BufReader};
    uuid::Uuid::parse_str(sid).ok()?;
    let dir = home.join(".codex/sessions");
    if dir.ancestors().take_while(|p| p.starts_with(home)).any(|p| p.is_symlink()) {
        return None;
    }
    let mut files = Vec::new();
    crate::finops::pi_files(&dir, &mut files, 0);
    let suffix = format!("{sid}.jsonl");
    let mut matches = files
        .iter()
        .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().ends_with(&suffix)));
    let path = matches.next()?;
    if matches.next().is_some() || path.metadata().ok()?.len() > 32 * 1024 * 1024 {
        return None;
    }
    let (mut valid, mut total) = (false, json!({}));
    for line in BufReader::new(std::fs::File::open(path).ok()?).lines() {
        let v: Value = serde_json::from_str(&line.ok()?).ok()?;
        let p = &v["payload"];
        if v["type"] == "session_meta" {
            valid = p["id"].as_str() == Some(sid);
        }
        if !valid {
            return None;
        }
        if v["type"] == "event_msg" && p["type"] == "token_count" && p["info"]["total_token_usage"].is_object() {
            total = p["info"]["total_token_usage"].clone();
        }
    }
    valid.then_some(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn thread_total_is_the_last_token_count_of_the_session_log() {
        let home = tempfile::tempdir().unwrap();
        let sid = "019abc00-0000-7000-8000-000000000001";
        let folder = home.path().join(".codex/sessions/2026/11/27");
        std::fs::create_dir_all(&folder).unwrap();
        let log = folder.join(format!("rollout-2026-11-27T10-00-00-{sid}.jsonl"));
        assert!(thread_total(home.path(), sid).is_none());
        let meta = json!({"type":"session_meta","payload":{"id":sid}});
        std::fs::write(&log, format!("{meta}\n")).unwrap();
        assert_eq!(thread_total(home.path(), sid), Some(json!({})));
        let lines = [
            meta.clone(),
            log_line("token_count", "2026-11-27T10:00:01Z", 100, 100, None),
            json!({"type":"event_msg","payload":{"type":"token_count","info":null}}),
            log_line("token_count", "2026-11-27T10:00:02Z", 250, 150, None),
        ];
        std::fs::write(&log, lines.iter().map(|l| format!("{l}\n")).collect::<String>()).unwrap();
        assert_eq!(thread_total(home.path(), sid).unwrap()["input_tokens"], 250);
        // Another session's log under this name, or an id that is not Codex's, is never read.
        std::fs::write(&log, format!("{}\n", json!({"type":"session_meta","payload":{"id":"other"}}))).unwrap();
        assert!(thread_total(home.path(), sid).is_none());
        assert!(thread_total(home.path(), "../escape").is_none());
    }
    fn sample() -> crate::finops::Usage {
        serde_json::from_value(json!({"id":"u","node":"n","binding":"author","runtime":"codex","model":"gpt-test","attempt":1,"at":1,"input":100,"output":20,"cache_read":40,"cache_write":10,"reasoning":5,"usd":null,"llm_ms":null,"calls":1,"tools":0,"source":"codex_native_history"})).unwrap()
    }
    pub fn prices() -> Catalog {
        Catalog::parse(&json!({"data":[{"id":"openai/gpt-test","pricing":{"prompt":"0.01","completion":"0.1","input_cache_read":"0.001","input_cache_write":"0.02","overrides":[{"min_prompt_tokens":100,"prompt":"0.02","completion":"0.15","input_cache_read":"0.002","input_cache_write":"0.04"}]}}]})).unwrap()
    }
    #[test]
    fn persisted_prices_survive_reopen_and_offline_refresh_with_original_date() {
        let d = tempfile::tempdir().unwrap();
        let now = crate::model::now();
        let date = now - 3600;
        {
            let store = crate::store::Store::open(d.path()).unwrap();
            let mut cache = Cache::default();
            let mut catalog = prices();
            catalog.at = date;
            cache.update(&store, now, Some(catalog));
            assert!(cache.catalog.persisted);
        }
        {
            let store = crate::store::Store::open(d.path()).unwrap();
            let mut restarted = Cache::load(&store).unwrap();
            assert_eq!(restarted.catalog.at, date);
            assert!(restarted.fresh(now + 1));
            assert!(!restarted.fresh(now + 301));
            let before = restarted.catalog.estimate(&sample()).unwrap();
            restarted.update(&store, now + 301, None);
            assert!(restarted.catalog.stale);
            assert_eq!(restarted.catalog.at, date);
            assert_eq!(restarted.catalog.estimate(&sample()), Some(before));
        }
        let store = crate::store::Store::open(d.path()).unwrap();
        let again = Cache::load(&store).unwrap();
        assert!(again.catalog.persisted && again.catalog.stale);
        assert_eq!(again.catalog.at, date);
        assert!(again.fresh(now + 302));
        assert!(!again.fresh(now)); // a clock rollback must not freeze refreshes
        assert!(again.catalog.estimate(&sample()).is_some());
    }
    #[test]
    fn missing_or_corrupt_persistent_prices_never_become_zero_costs() {
        let d = tempfile::tempdir().unwrap();
        let store = crate::store::Store::open(d.path()).unwrap();
        let mut cache = Cache::load(&store).unwrap();
        assert!(cache.catalog.estimate(&sample()).is_none());
        cache.update(&store, crate::model::now(), None);
        let offline = Cache::load(&store).unwrap();
        assert!(offline.catalog.stale);
        assert!(offline.catalog.prices.is_empty());
        assert!(offline.catalog.estimate(&sample()).is_none());
        store
            .db
            .lock()
            .unwrap()
            .execute("UPDATE finops_catalog SET json='broken'", [])
            .unwrap();
        assert!(Cache::load(&store).is_err());
    }
    #[test]
    fn prices_cache_context_threshold_and_unknown_models() {
        let c = prices();
        let mut u = sample();
        assert!((c.estimate(&u).unwrap() - 2.74).abs() < 1e-10);
        u.input = 101;
        assert!((c.estimate(&u).unwrap() - 4.5).abs() < 1e-10);
        u.source = "live".into();
        assert!((c.estimate(&u).unwrap() - 2.75).abs() < 1e-10); // aggregate is explicitly approximate
        u.model = "unknown".into();
        assert!(c.estimate(&u).is_none());
        u.model = "gpt-test".into();
        u.usd = Some(0.);
        assert!(c.estimate(&u).is_none());
        u.usd = None;
        u.cache_read = Some(200);
        assert!(c.estimate(&u).is_none());
    }
    #[test]
    fn rejects_invalid_prices_and_unsupported_pricing_rules() {
        assert!(
            Catalog::parse(
                &json!({"data":[{"id":"openai/x","pricing":{"prompt":"NaN","completion":"1"}}]})
            )
            .is_none()
        );
        let mut c = prices();
        c.prices.get_mut("openai/gpt-test").unwrap()["overrides"] =
            json!([{"utc_start":"12:00","prompt":"0"}]);
        assert!(c.estimate(&sample()).is_none());
    }
    /// A Codex log line as Codex writes it on a ChatGPT sign-in (`plan`) or with an API key (`None`).
    fn log_line(kind: &str, at: &str, total: u64, last: u64, plan: Option<&str>) -> Value {
        let usage = |input: u64| json!({"input_tokens":input,"cached_input_tokens":input/2,"cache_write_input_tokens":0,"output_tokens":10,"reasoning_output_tokens":3,"total_tokens":input+10});
        let rate_limits = plan.map(|plan| json!({"limit_id":"codex","limit_name":null,"primary":{"used_percent":1.0,"window_minutes":300,"resets_at":1},"secondary":null,"credits":{"has_credits":false,"unlimited":false,"balance":null},"plan_type":plan,"individual_limit":null,"rate_limit_reached_type":null,"spend_control_reached":false}));
        match kind {
            "token_count" => json!({"timestamp":at,"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":usage(total),"last_token_usage":usage(last),"last_usage_includes_reasoning":true,"model_context_window":475000},"rate_limits":rate_limits}}),
            // Newer Codex versions also repeat each request here: it must not be counted twice.
            _ => json!({"timestamp":at,"type":"token_usage_record","payload":{"response_id":"r","turn_id":"t","root_turn_id":"t","usage":usage(last),"turn_token_usage":usage(total),"thread_token_usage":usage(total)}}),
        }
    }
    #[tokio::test]
    async fn trailing_slash_workspace_log_is_read_and_priced_with_or_without_a_plan() {
        let d = tempfile::tempdir().unwrap();
        let app = crate::tests::app(d.path());
        let mut r = crate::engine::create(&app, 1, crate::presets::all().remove(0), "plan".into(), None)
            .await
            .unwrap();
        let sid = "01a213a4-2ee0-7ff1-9db1-1c417f6921e3";
        r.sessions.insert(
            "author".into(),
            crate::model::NativeSession {
                provider: String::new(), base_url: String::new(), runtime: "codex".into(),
                native_id: sid.into(), model: String::new(), turns: 1,
            },
        );
        // A direct source at the root of the sources folder: the run keeps a trailing slash.
        let cwd = r.workspace.trim_end_matches('/').to_string();
        r.workspace = format!("{cwd}/");
        let write = |plan: Option<&str>| {
            let path = d.path().join(format!("rollout-2026-11-25T17-57-36-{sid}.jsonl"));
            let lines = [
                json!({"timestamp":"2026-11-25T16:57:36Z","type":"session_meta","payload":{"id":sid,"session_id":sid,"cwd":cwd,"originator":"codex_exec","source":"exec","model_provider":"openai"}}),
                json!({"timestamp":"2026-11-25T16:57:37Z","type":"turn_context","payload":{"turn_id":"t","cwd":cwd,"model":"gpt-test"}}),
                json!({"timestamp":"2026-11-25T16:57:37Z","type":"event_msg","payload":{"type":"task_started","turn_id":"t"}}),
                log_line("record", "2026-11-25T16:57:43Z", 100, 100, plan),
                log_line("token_count", "2026-11-25T16:57:44Z", 100, 100, plan),
                log_line("record", "2026-11-25T16:57:47Z", 220, 120, plan),
                log_line("token_count", "2026-11-25T16:57:48Z", 220, 120, plan),
                // Codex repeats a token_count when only the rate limits change.
                log_line("token_count", "2026-11-25T16:57:49Z", 220, 120, plan),
                json!({"timestamp":"2026-11-25T16:57:51Z","type":"event_msg","payload":{"type":"task_complete","turn_id":"t"}}),
            ];
            std::fs::write(&path, lines.iter().map(|l| l.to_string() + "\n").collect::<String>()).unwrap();
            vec![path]
        };
        let cost = |rows: &[crate::finops::Usage]| {
            rows.iter().map(|u| prices().estimate(u).unwrap()).sum::<f64>()
        };
        let files = write(Some("prolite"));
        let (rows, missed) = history(&r, &[], &files, &mut 0);
        assert_eq!(missed, 0);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows.iter().map(|u| u.input).sum::<u64>(), 220);
        assert_eq!(rows.iter().map(|u| u.output).sum::<u64>(), 20);
        assert_eq!(rows[0].cache_read, Some(50));
        assert_eq!(rows[0].reasoning, Some(3));
        assert_eq!(rows[0].model, "gpt-test");
        // A ChatGPT sign-in is priced at API rates like any other Codex run.
        let chatgpt = cost(&rows);
        assert!(chatgpt > 0.);
        // The same log written with an API key has no plan and gets the same estimate.
        let files = write(None);
        let (rows, missed) = history(&r, &[], &files, &mut 0);
        assert_eq!(missed, 0);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].model, "gpt-test");
        assert_eq!(cost(&rows), chatgpt);
        // Another folder is still refused.
        r.workspace = format!("{cwd}-other/");
        assert_eq!(history(&r, &[], &files, &mut 0).1, 1);
    }
    /// Observed with two Tasks on one Codex session, the second started in the second in which the
    /// first ended: its last token count (22:04:07.141) went to the second Task (13249 / 63353).
    #[tokio::test]
    async fn token_counts_go_to_the_turn_that_wrote_them_when_steps_change_within_a_second() {
        let d = tempfile::tempdir().unwrap();
        let app = crate::tests::app(d.path());
        let mut r = crate::engine::create(&app, 1, crate::presets::all().remove(0), "split".into(), None)
            .await
            .unwrap();
        let sid = "019ac1f0-0000-7000-8000-00000000c0de";
        r.sessions.insert(
            "author".into(),
            crate::model::NativeSession {
                provider: String::new(), base_url: String::new(), runtime: "codex".into(),
                native_id: sid.into(), model: "gpt-test".into(), turns: 2,
            },
        );
        let token = |at: &str, total: u64, input: u64, output: u64| json!({"timestamp":at,"type":"event_msg","payload":{"type":"token_count",
            "info":{"total_token_usage":{"input_tokens":total,"output_tokens":0},"last_token_usage":{"input_tokens":input,"cached_input_tokens":0,"output_tokens":output}}}});
        let task = |at: &str, kind: &str, turn: &str| json!({"timestamp":at,"type":"event_msg","payload":{"type":kind,"turn_id":turn}});
        let lines = [
            json!({"timestamp":"2026-11-26T22:03:40.500Z","type":"session_meta","payload":{"id":sid,"cwd":r.workspace}}),
            json!({"timestamp":"2026-11-26T22:03:40.510Z","type":"turn_context","payload":{"turn_id":"t1","model":"gpt-test"}}),
            task("2026-11-26T22:03:40.512Z", "task_started", "t1"),
            token("2026-11-26T22:03:52.300Z", 13249, 13000, 249),
            token("2026-11-26T22:04:07.141Z", 26808, 13400, 159),
            task("2026-11-26T22:04:07.150Z", "task_complete", "t1"),
            json!({"timestamp":"2026-11-26T22:04:07.618Z","type":"turn_context","payload":{"turn_id":"t2","model":"gpt-test"}}),
            task("2026-11-26T22:04:07.620Z", "task_started", "t2"),
            token("2026-11-26T22:04:30.004Z", 76602, 49000, 794),
            task("2026-11-26T22:04:30.010Z", "task_complete", "t2"),
        ];
        let path = d.path().join(format!("rollout-2026-11-26T23-03-40-{sid}.jsonl"));
        std::fs::write(&path, lines.iter().map(|l| l.to_string() + "\n").collect::<String>()).unwrap();
        let files = vec![path];
        let second = |at: &str| at.parse::<jiff::Timestamp>().unwrap().as_second();
        let event = |kind: &str, node: &str, text: String, at: &str| crate::model::Event {
            seq: 0, run: r.id.clone(), kind: kind.into(), node: node.into(), text, created: second(at),
        };
        let turn = |node: &str, at: &str| event("agent_turn", node, json!({"binding":"author","runtime":"codex","node":node,"attempt":1,"status":"cli_success"}).to_string(), at);
        // Run events keep whole seconds: the second Task starts at 22:04:07, read as .000.
        let states = vec![
            event("state", "first", "running".into(), "2026-11-26T22:03:40Z"),
            turn("first", "2026-11-26T22:04:07Z"),
            event("state", "first", "completed".into(), "2026-11-26T22:04:07Z"),
            event("state", "second", "running".into(), "2026-11-26T22:04:07Z"),
            turn("second", "2026-11-26T22:04:30Z"),
            event("state", "second", "completed".into(), "2026-11-26T22:04:30Z"),
        ];
        let split = |events: &[crate::model::Event]| {
            let (rows, missed) = history(&r, events, &files, &mut 0);
            assert_eq!(missed, 0);
            let tokens = |node: &str| rows.iter().filter(|u| u.node == node).map(|u| u.input + u.output).sum::<u64>();
            assert!(rows.iter().all(|u| u.attempt == 1));
            (tokens("first"), tokens("second"))
        };
        // Matched in order with the run's turns on the binding.
        assert_eq!(split(&states), (26808, 49794));
        // Runs saved before agent_turn, or a turn that never reached the log: each log turn goes
        // to the step running when it began.
        let legacy: Vec<_> = states.iter().filter(|e| e.kind != "agent_turn").cloned().collect();
        assert_eq!(split(&legacy), (26808, 49794));
        let mut unmatched = states.clone();
        unmatched.insert(1, turn("first", "2026-11-26T22:03:40Z"));
        assert_eq!(split(&unmatched), (26808, 49794));
    }
    #[tokio::test]
    async fn native_history_deduplicates_checks_ownership_and_rejects_partial_logs() {
        let d = tempfile::tempdir().unwrap();
        let app = crate::tests::app(d.path());
        let mut r = crate::engine::create(
            &app,
            1,
            crate::presets::all().remove(0),
            "history".into(),
            None,
        )
        .await
        .unwrap();
        let sid = crate::model::id();
        r.sessions.insert(
            "author".into(),
            crate::model::NativeSession {
                provider: String::new(), base_url: String::new(),
runtime: "codex".into(),
                native_id: sid.clone(),
                model: "gpt-test".into(),
                turns: 1,
            },
        );
        let path = d.path().join(format!("rollout-{sid}.jsonl"));
        let header = json!({"type":"session_meta","payload":{"id":sid,"cwd":r.workspace}});
        let context = json!({"type":"turn_context","payload":{"model":"gpt-test","turn_id":"t"}});
        let token = json!({"type":"event_msg","timestamp":"2026-10-31T10:00:00Z","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"output_tokens":20},"last_token_usage":{"input_tokens":100,"output_tokens":20,"cached_input_tokens":40,"cache_write_input_tokens":10}}}});
        let body = format!("{header}\n{context}\n{token}\n{token}\n");
        std::fs::write(&path, &body).unwrap();
        let files = vec![path.clone()];
        let (rows, missed) = history(&r, &[], &files, &mut 0);
        assert_eq!(missed, 0);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].input, 100);
        assert_eq!(rows[0].cache_write, Some(10));
        r.workspace = "other".into();
        assert_eq!(history(&r, &[], &files, &mut 0).1, 1);
        r.workspace = header["payload"]["cwd"].as_str().unwrap().into();
        std::fs::write(
            &path,
            format!("{body}{{\"type\":\"event_msg\",\"payload\":{{\"type\":\"task_started\"}}}}\n"),
        )
        .unwrap();
        assert!(history(&r, &[], &files, &mut 0).0.is_empty());
        assert_eq!(history(&r, &[], &files, &mut (128 * 1024 * 1024)).1, 1);
    }
}
