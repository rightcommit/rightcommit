//! Explicitly resume a stopped run from a verified control-flow checkpoint.
use crate::{App, agents, engine, model::*};
use anyhow::{Result, bail, ensure};
use std::collections::HashSet;

fn predecessor(r: &Run, node: &str) -> Option<Edge> {
    r.workflow
        .edges
        .iter()
        .find(|edge| {
            if edge.kind != "control"
                || edge.to != node
                || r.states.get(&edge.from).is_none_or(|s| s != "completed")
            {
                return false;
            }
            let Ok(previous) = r.workflow.node(&edge.from) else {
                return false;
            };
            let Some(output) = r
                .outputs
                .get(&edge.from)
                .filter(|o| o.status == "completed")
            else {
                return false;
            };
            let outcome = if previous.kind == "Gate" {
                output.decision.as_str()
            } else {
                ""
            };
            r.workflow
                .next(&edge.from, outcome)
                .is_some_and(|selected| selected.kind == "control" && selected.to == node)
        })
        .cloned()
}
/// Where the engine resumes when the user continues from a node.
struct Checkpoint {
    /// The node the engine runs first: the chosen node, or the Fork of a chosen branch step.
    current: String,
    /// The node the run arrives from (`last_node`); its result is the first step's handoff.
    previous: Option<String>,
    /// The link that shows the moving dots while the chosen node runs.
    shown: Option<Edge>,
    /// For a Join: its result rebuilt from the kept branch outputs.
    joined: Option<AgentResult>,
}
/// Each valid Fork with its branches, in workflow order.
fn forks(r: &Run) -> impl Iterator<Item = (&Node, Vec<Vec<Node>>)> {
    r.workflow
        .nodes
        .iter()
        .filter(|n| n.kind == "Fork")
        .filter_map(|fork| Some((fork, engine::parallel_branches(&r.workflow, fork).ok()?)))
}
/// A branch step is done as the Fork counts it: completed with a completed result, or skipped.
fn branch_done(r: &Run, n: &Node) -> bool {
    n.skip
        || r.states.get(&n.id).is_some_and(|s| s == "completed")
            && r.outputs.get(&n.id).is_some_and(|o| o.status == "completed")
}
fn checkpoint(r: &Run, node: &str) -> Option<Checkpoint> {
    if r.workflow.node(node).ok()?.kind == "Join" {
        // The Join collects every branch: all of them must be done, and their outputs are kept.
        let (fork, branches) = forks(r).find(|(fork, _)| fork.target == node)?;
        if r.states.get(&fork.id).is_none_or(|s| s != "completed")
            || !branches.iter().flatten().all(|n| branch_done(r, n))
        {
            return None;
        }
        // As in a run that reaches the Join: it arrives from the Fork, by no single link.
        return Some(Checkpoint {
            current: node.into(),
            previous: Some(fork.id.clone()),
            shown: None,
            joined: Some(engine::barrier_output(r, &branches).ok()?),
        });
    }
    let branch = forks(r).find_map(|(fork, branches)| {
        Some((fork, branches.into_iter().find(|b| b.iter().any(|n| n.id == node))?))
    });
    if let Some((fork, chain)) = branch {
        // A branch step runs again through its Fork, which runs only the branch steps that are
        // not done and then joins them with the kept outputs of the other branches.
        let entered = r
            .states
            .get(&fork.id)
            .is_some_and(|s| !["queued", "skipped"].contains(&s.as_str()));
        let arrival = predecessor(r, &fork.id);
        let earlier: Vec<_> = chain.iter().take_while(|n| n.id != node).collect();
        if !entered
            || arrival.is_none() && fork.id != r.workflow.entry
            || !earlier.iter().all(|n| branch_done(r, n))
        {
            return None;
        }
        let from = earlier.last().map_or(&fork.id, |n| &n.id);
        return Some(Checkpoint {
            current: fork.id.clone(),
            previous: arrival.map(|edge| edge.from),
            shown: r
                .workflow
                .edges
                .iter()
                .find(|e| e.kind == "control" && &e.from == from && e.to == node)
                .cloned(),
            joined: None,
        });
    }
    let incoming = predecessor(r, node)?;
    Some(Checkpoint {
        current: node.into(),
        previous: Some(incoming.from.clone()),
        shown: Some(incoming),
        joined: None,
    })
}
/// The node a completed step was reached from; a Join is reached from its Fork.
fn reached_from(r: &Run, id: &str) -> Option<String> {
    if r.workflow.node(id).is_ok_and(|n| n.kind == "Join") {
        return r
            .workflow
            .nodes
            .iter()
            .find(|n| n.kind == "Fork" && n.target == id)
            .map(|n| n.id.clone());
    }
    predecessor(r, id).map(|edge| edge.from)
}
fn restart_point(r: &Run, node: &str) -> bool {
    node == r.workflow.entry
        || r.workflow
            .nodes
            .iter()
            .find(|n| n.kind == "Source")
            .is_some_and(|n| n.id == node)
}
fn available(app: &App, uid: i64, r: &Run, busy: &HashSet<String>, restarting: bool) -> Result<()> {
    ensure!(
        !busy.contains(&r.id),
        "Metti la run in pausa prima di scegliere un nodo"
    );
    ensure!(
        (["paused", "failed", "interrupted", "completed"].contains(&r.status.as_str())
            || restarting && ["waiting_review", "waiting_user"].contains(&r.status.as_str()))
            && (restarting || r.pending.is_none()),
        "Risolvi le richieste in attesa prima di continuare da un nodo"
    );
    let mut children: Vec<_> = r.children.iter().map(|call| call.run.clone()).collect();
    let mut seen = HashSet::new();
    while let Some(id) = children.pop() {
        if !seen.insert(id.clone()) {
            continue;
        }
        let child = app.store.run(uid, &id)?;
        ensure!(
            ["completed", "failed", "interrupted"].contains(&child.status.as_str())
                && child.pending.is_none(),
            "Risolvi le attività e le richieste del workflow figlio prima di scegliere un nodo"
        );
        children.extend(child.children.iter().map(|call| call.run.clone()));
    }
    crate::subworkflows::can_launch_reserved(app, uid, r, busy)
}
pub fn candidates(app: &App, uid: i64, r: &Run) -> Vec<String> {
    let busy = app.busy.lock().unwrap();
    let can_restart = available(app, uid, r, &busy, true).is_ok();
    let can_continue = available(app, uid, r, &busy, false).is_ok();
    r.workflow
        .nodes
        .iter()
        .filter(|n| {
            if restart_point(r, &n.id) {
                can_restart
            } else {
                can_continue && checkpoint(r, &n.id).is_some()
            }
        })
        .map(|n| n.id.clone())
        .collect()
}
pub fn prepare(app: &App, uid: i64, rid: &str, node: &str, acknowledge: bool) -> Result<Run> {
    let busy = app.busy.lock().unwrap();
    let mut r = app.store.run(uid, rid)?;
    let restarting = restart_point(&r, node);
    available(app, uid, &r, &busy, restarting)?;
    let target = if restarting {
        r.workflow.entry.clone()
    } else {
        node.into()
    };
    let node = target.as_str();
    let resume = if restarting {
        Checkpoint {
            current: node.into(),
            previous: None,
            shown: None,
            joined: None,
        }
    } else if let Some(resume) = checkpoint(&r, node) {
        resume
    } else if r.workflow.node(node).is_ok_and(|n| n.kind == "Join") {
        bail!("Non tutti i rami del Fork sono completati: continua dal Fork o da un passo del ramo da rieseguire")
    } else {
        bail!("Il nodo precedente non è completato con successo sul percorso selezionato")
    };
    ensure!(
        acknowledge,
        "Conferma la ripresa: i passi da questo nodo saranno rieseguiti sui file correnti"
    );
    let mut affected = HashSet::new();
    let mut pending = vec![node.to_string()];
    while let Some(id) = pending.pop() {
        if !affected.insert(id.clone()) {
            continue;
        }
        pending.extend(
            r.workflow
                .edges
                .iter()
                .filter(|e| e.from == id && e.kind == "control")
                .map(|e| e.to.clone()),
        );
    }
    // Preserve attempts, supervisor budgets, sessions, child history and artifacts. The steps
    // that run again get their rework budget back; the others keep their count.
    for id in &affected {
        r.states.insert(id.clone(), "queued".into());
        r.outputs.remove(id);
        r.reworks.remove(id);
        for prefix in [
            "human-approved",
            "answer",
            "feedback",
            "test-proof",
            "effect",
            "effect-preview",
        ] {
            r.context.remove(&format!("{prefix}:{id}"));
        }
        if let Ok(n) = r.workflow.node(id)
            && n.kind == "Memory"
        {
            r.context.remove(&format!("handoff:{}", n.target));
        }
    }
    if resume.current != node {
        // The Fork of the chosen branch step runs again; the other branches keep their state.
        r.states.insert(resume.current.clone(), "queued".into());
    }
    if let Some(joined) = resume.joined {
        r.outputs.insert(node.into(), joined);
    }
    r.last_task = None;
    let mut cursor = resume.previous.clone();
    let mut seen = HashSet::new();
    while let Some(id) = cursor {
        if !seen.insert(id.clone()) {
            break;
        }
        if r.workflow.node(&id)?.kind == "Task" {
            r.last_task = Some(id);
            break;
        }
        cursor = reached_from(&r, &id);
    }
    r.current = resume.current;
    if let Some(id) = &resume.previous
        && let Ok(previous) = r.workflow.node(id)
        && previous.kind == "Memory"
        && let Some(output) = r.outputs.get(&previous.id)
    {
        r.context.insert(
            format!("handoff:{}", previous.target),
            output.summary.clone(),
        );
    }
    r.last_node = resume.previous;
    r.active_edge = resume.shown;
    r.pending = None;
    r.status = "paused".into();
    r.error = None;
    r.pause_requested = false;
    r.context.remove("review-feedback");
    engine::save(app, uid, &mut r)?;
    agents::emit(
        app,
        uid,
        rid,
        "continue",
        node,
        "Ripresa esplicita dal nodo: risultati e approvazioni a valle invalidati; file correnti e sessioni conservati",
    );
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;
    async fn fixture() -> (tempfile::TempDir, App, Run) {
        let dir = tempfile::tempdir().unwrap();
        let app = crate::tests::app(dir.path());
        let w = crate::presets::catalog()
            .into_iter()
            .find(|template| template.id == "as-is-sdd")
            .expect("baseline template for checkpoint tests")
            .workflow;
        let mut r = engine::create(&app, 1, w, "test".into(), None)
            .await
            .unwrap();
        for n in &r.workflow.nodes {
            if !["Source", "Agent"].contains(&n.kind.as_str()) {
                r.states.insert(n.id.clone(), "completed".into());
                r.outputs.insert(
                    n.id.clone(),
                    AgentResult {
                        status: "completed".into(),
                        decision: "pass".into(),
                        ..Default::default()
                    },
                );
            }
        }
        r.status = "completed".into();
        app.store.save_run(1, &r).unwrap();
        (dir, app, r)
    }
    #[tokio::test]
    async fn continuation_preserves_checkpoint_and_invalidates_downstream_approvals() {
        let (_dir, app, mut r) = fixture().await;
        r.attempts.insert("step-1".into(), 2);
        r.reworks.insert("baseline-review".into(), 1);
        r.context.insert("supervisor:count".into(), "3".into());
        for prefix in [
            "human-approved",
            "effect",
            "effect-preview",
            "answer",
            "test-proof",
        ] {
            r.context.insert(format!("{prefix}:step-1"), "stale".into());
        }
        r.context
            .insert("human-approved:baseline-review".into(), "stale".into());
        app.store.save_run(1, &r).unwrap();
        assert!(candidates(&app, 1, &r).contains(&"step-1".into()));
        assert!(candidates(&app, 1, &r).contains(&"step-0".into()));
        assert!(prepare(&app, 1, &r.id, "step-1", false).is_err());
        assert!(prepare(&app, 2, &r.id, "step-1", true).is_err());
        let resumed = prepare(&app, 1, &r.id, "step-1", true).unwrap();
        assert_eq!(resumed.id, r.id);
        assert_eq!(resumed.workspace, r.workspace);
        assert_eq!(resumed.current, "step-1");
        assert_eq!(resumed.last_node.as_deref(), Some("step-0"));
        assert_eq!(resumed.last_task.as_deref(), Some("step-0"));
        assert_eq!(resumed.states["step-0"], "completed");
        assert!(resumed.outputs.contains_key("step-0"));
        for id in ["step-1", "step-2", "step-3", "baseline-review"] {
            assert_eq!(resumed.states[id], "queued");
            assert!(!resumed.outputs.contains_key(id));
        }
        assert!(!resumed.context.contains_key("effect:step-1"));
        assert!(
            !resumed
                .context
                .contains_key("human-approved:baseline-review")
        );
        assert_eq!(resumed.attempts["step-1"], 2);
        // The review runs again after step-1, so it gets its rework budget back.
        assert!(!resumed.reworks.contains_key("baseline-review"));
        assert_eq!(resumed.context["supervisor:count"], "3");
    }
    #[tokio::test]
    async fn first_node_restarts_without_predecessor_and_invalidates_pending_approval() {
        let (_dir, app, mut r) = fixture().await;
        r.status = "waiting_review".into();
        r.pending = Some(Pending {
            id: "approval".into(),
            proposal: None,
            kind: "review".into(),
            node: "baseline-review".into(),
            questions: vec![],
            resume_task: None,
            commit: None,
        });
        app.store.save_run(1, &r).unwrap();
        let ids = candidates(&app, 1, &r);
        assert!(ids.contains(&"source".into()));
        assert!(ids.contains(&"step-0".into()));
        assert!(!ids.contains(&"step-1".into()));
        let resumed = prepare(&app, 1, &r.id, "source", true).unwrap();
        assert_eq!(resumed.current, "step-0");
        assert!(resumed.last_node.is_none());
        assert!(resumed.last_task.is_none());
        assert!(resumed.active_edge.is_none());
        // When it runs, the restarted first node shows the edge of the first start.
        let first = r.active_edge.clone().expect("first start shows an entry edge");
        let shown = engine::arrival_edge(&resumed, &resumed.current).expect("restart shows an entry edge");
        assert_eq!((shown.from, shown.to, shown.kind), (first.from, first.to, first.kind));
        assert!(resumed.pending.is_none());
        assert!(resumed.outputs.is_empty());
        assert_eq!(resumed.workspace, r.workspace);
        assert_eq!(resumed.id, r.id);
        r.states.remove("step-0");
        r.outputs.remove("step-0");
        r.pending = None;
        r.status = "paused".into();
        app.store.save_run(1, &r).unwrap();
        assert!(prepare(&app, 1, &r.id, "step-0", true).is_ok());
        assert!(prepare(&app, 1, &r.id, "step-1", true).is_err());
        app.busy.lock().unwrap().insert(r.id.clone());
        assert!(prepare(&app, 1, &r.id, "source", true).is_err());
    }
    #[tokio::test]
    async fn continuation_rejects_unsuccessful_predecessors_busy_runs_and_pending_reviews() {
        let (_dir, app, mut r) = fixture().await;
        app.busy.lock().unwrap().insert(r.id.clone());
        assert!(candidates(&app, 1, &r).is_empty());
        assert!(prepare(&app, 1, &r.id, "step-1", true).is_err());
        app.busy.lock().unwrap().clear();
        for status in ["failed", "running", "skipped", "queued"] {
            r.states.insert("step-0".into(), status.into());
            app.store.save_run(1, &r).unwrap();
            assert!(prepare(&app, 1, &r.id, "step-1", true).is_err());
        }
        r.states.insert("step-0".into(), "completed".into());
        r.status = "waiting_review".into();
        app.store.save_run(1, &r).unwrap();
        assert!(prepare(&app, 1, &r.id, "step-1", true).is_err());
        r.status = "paused".into();
        r.pending = Some(Pending {
            id: "pending".into(),
            proposal: None,
            kind: "review".into(),
            node: "baseline-review".into(),
            questions: vec![],
            resume_task: None,
            commit: None,
        });
        app.store.save_run(1, &r).unwrap();
        assert!(prepare(&app, 1, &r.id, "step-1", true).is_err());
    }
    #[tokio::test]
    async fn continuation_only_follows_the_successful_gate_branch() {
        let mut w = crate::presets::all().remove(1);
        let gate = w.nodes.iter_mut().find(|n| n.kind == "Gate").unwrap();
        let gid = gate.id.clone();
        let selected = w.next(&gid, "spec_change").unwrap().to.clone();
        let (_dir, _app, mut r) = fixture().await;
        r.workflow = w;
        r.states.clear();
        r.outputs.clear();
        r.states.insert(gid.clone(), "completed".into());
        r.outputs.insert(
            gid.clone(),
            AgentResult {
                status: "completed".into(),
                decision: "spec_change".into(),
                ..Default::default()
            },
        );
        assert!(predecessor(&r, &selected).is_some());
        r.outputs.get_mut(&gid).unwrap().decision = "needs_info".into();
        assert!(predecessor(&r, &selected).is_none());
    }
    fn result(summary: &str) -> AgentResult {
        AgentResult {
            status: "completed".into(),
            summary: summary.into(),
            decision: "pass".into(),
            ..Default::default()
        }
    }
    fn edge(from: &str, to: &str, kind: &str, outcome: &str) -> Edge {
        Edge {
            from: from.into(),
            to: to.into(),
            kind: kind.into(),
            outcome: outcome.into(),
        }
    }
    /// plan → Fork → (a1 → a2 | b) → Join → human Review → synthesis, as a completed run.
    async fn parallel_fixture() -> (tempfile::TempDir, App, Run) {
        let dir = tempfile::tempdir().unwrap();
        let app = crate::tests::app(dir.path());
        let mut w: Workflow = serde_json::from_value(serde_json::json!({
            "id":"parallel-continue", "name":"Parallel continue", "description":"", "entry":"plan",
            "nodes":[{"id":"source","kind":"Source","source_type":"example"},
                {"id":"coordinator","kind":"Agent"},{"id":"aa","kind":"Agent"},{"id":"ab","kind":"Agent"},
                {"id":"plan","kind":"Task","agent":"coordinator","prompt":"Plan"},
                {"id":"fork","kind":"Fork","target":"join"},
                {"id":"a1","kind":"Task","agent":"aa","prompt":"A1"},
                {"id":"a2","kind":"Task","agent":"aa","prompt":"A2"},
                {"id":"b","kind":"Task","agent":"ab","prompt":"B"},
                {"id":"join","kind":"Join"},
                {"id":"review","kind":"Review","mode":"human","prompt":"Review"},
                {"id":"synthesis","kind":"Task","agent":"coordinator","prompt":"Synthesis"}],
            "edges":[]
        }))
        .unwrap();
        w.edges = vec![
            edge("source", "coordinator", "context", ""),
            edge("coordinator", "plan", "context", ""),
            edge("coordinator", "synthesis", "context", ""),
            edge("aa", "a1", "context", ""),
            edge("aa", "a2", "context", ""),
            edge("ab", "b", "context", ""),
            edge("plan", "fork", "control", ""),
            edge("fork", "a1", "control", ""),
            edge("fork", "b", "control", ""),
            edge("a1", "a2", "control", ""),
            edge("a2", "join", "control", ""),
            edge("b", "join", "control", ""),
            edge("join", "review", "control", ""),
            edge("review", "synthesis", "control", ""),
        ];
        let mut r = engine::create(&app, 1, w, "test".into(), None)
            .await
            .unwrap();
        for id in ["plan", "fork", "a1", "a2", "b", "join", "review", "synthesis"] {
            r.states.insert(id.into(), "completed".into());
            if id != "fork" {
                r.outputs.insert(id.into(), result(&format!("{id} first pass")));
            }
        }
        r.status = "completed".into();
        r.current = "synthesis".into();
        r.last_node = Some("review".into());
        app.store.save_run(1, &r).unwrap();
        (dir, app, r)
    }
    #[tokio::test]
    async fn continuation_from_the_join_keeps_every_branch_and_runs_the_join_again() {
        let (_dir, app, r) = parallel_fixture().await;
        assert!(candidates(&app, 1, &r).contains(&"join".into()));
        let resumed = prepare(&app, 1, &r.id, "join", true).unwrap();
        assert_eq!(resumed.current, "join");
        // As in a run that reaches the Join: it arrives from the Fork, by no single link.
        assert_eq!(resumed.last_node.as_deref(), Some("fork"));
        assert!(resumed.active_edge.is_none());
        assert_eq!(resumed.last_task.as_deref(), Some("plan"));
        for id in ["plan", "fork", "a1", "a2", "b"] {
            assert_eq!(resumed.states[id], "completed", "{id}");
        }
        for id in ["plan", "a1", "a2", "b"] {
            assert_eq!(resumed.outputs[id].summary, format!("{id} first pass"));
        }
        for id in ["join", "review", "synthesis"] {
            assert_eq!(resumed.states[id], "queued", "{id}");
        }
        engine::drive(&app, 1, &r.id).await.unwrap();
        let saved = app.store.run(1, &r.id).unwrap();
        assert_eq!(saved.status, "waiting_review");
        assert_eq!(saved.pending.unwrap().node, "review");
        assert_eq!(saved.states["join"], "completed");
        assert_eq!(saved.attempts["join"], 1);
        assert!(!saved.attempts.contains_key("a1") && !saved.attempts.contains_key("b"));
        // The node after the Join receives every branch output, as in a normal run.
        let package: serde_json::Value =
            serde_json::from_str(&saved.context["handoff:coordinator"]).unwrap();
        assert_eq!(package["source_task"], "join");
        let results: serde_json::Value =
            serde_json::from_str(package["source_output"]["summary"].as_str().unwrap()).unwrap();
        for id in ["a1", "a2", "b"] {
            assert_eq!(results[id]["summary"], format!("{id} first pass"));
        }
    }
    #[tokio::test]
    async fn continuation_inside_a_branch_runs_that_branch_and_joins_the_kept_ones() {
        let (_dir, app, r) = parallel_fixture().await;
        let ids = candidates(&app, 1, &r);
        for id in ["fork", "a1", "a2", "b", "join", "review", "synthesis"] {
            assert!(ids.contains(&id.into()), "{id}");
        }
        let resumed = prepare(&app, 1, &r.id, "a2", true).unwrap();
        // The Fork runs again: it runs only the branch steps that are not completed.
        assert_eq!(resumed.current, "fork");
        assert_eq!(resumed.last_node.as_deref(), Some("plan"));
        assert_eq!(resumed.last_task.as_deref(), Some("plan"));
        let shown = resumed.active_edge.clone().expect("the link into the chosen step");
        assert_eq!((shown.from.as_str(), shown.to.as_str()), ("a1", "a2"));
        for id in ["plan", "a1", "b"] {
            assert_eq!(resumed.states[id], "completed", "{id}");
            assert_eq!(resumed.outputs[id].summary, format!("{id} first pass"));
        }
        for id in ["fork", "a2", "join", "review", "synthesis"] {
            assert_eq!(resumed.states[id], "queued", "{id}");
            assert!(!resumed.outputs.contains_key(id), "{id}");
        }
        // The chosen step completes in place of its agent; the Fork joins it with the kept branch.
        let mut ran = resumed.clone();
        ran.states.insert("a2".into(), "completed".into());
        ran.outputs.insert("a2".into(), result("a2 second pass"));
        app.store.save_run(1, &ran).unwrap();
        engine::drive(&app, 1, &r.id).await.unwrap();
        let saved = app.store.run(1, &r.id).unwrap();
        assert_eq!(saved.status, "waiting_review");
        assert_eq!(saved.states["fork"], "completed");
        assert_eq!(saved.states["join"], "completed");
        let results: serde_json::Value =
            serde_json::from_str(&saved.outputs["join"].summary).unwrap();
        assert_eq!(results["a1"]["summary"], "a1 first pass");
        assert_eq!(results["a2"]["summary"], "a2 second pass");
        assert_eq!(results["b"]["summary"], "b first pass");
        // The first step of a branch arrives by the Fork's link to it.
        let mut done = app.store.run(1, &r.id).unwrap();
        done.pending = None;
        done.status = "completed".into();
        app.store.save_run(1, &done).unwrap();
        let resumed = prepare(&app, 1, &r.id, "b", true).unwrap();
        assert_eq!(resumed.current, "fork");
        let shown = resumed.active_edge.clone().unwrap();
        assert_eq!((shown.from.as_str(), shown.to.as_str()), ("fork", "b"));
        assert_eq!(resumed.outputs["a2"].summary, "a2 second pass");
        assert_eq!(resumed.states["b"], "queued");
    }
    #[tokio::test]
    async fn continuation_from_the_fork_runs_every_branch_again() {
        let (_dir, app, r) = parallel_fixture().await;
        let resumed = prepare(&app, 1, &r.id, "fork", true).unwrap();
        assert_eq!(resumed.current, "fork");
        assert_eq!(resumed.last_node.as_deref(), Some("plan"));
        let shown = resumed.active_edge.clone().unwrap();
        assert_eq!((shown.from.as_str(), shown.to.as_str()), ("plan", "fork"));
        assert_eq!(resumed.outputs["plan"].summary, "plan first pass");
        for id in ["fork", "a1", "a2", "b", "join", "review", "synthesis"] {
            assert_eq!(resumed.states[id], "queued", "{id}");
            assert!(!resumed.outputs.contains_key(id), "{id}");
        }
    }
    #[tokio::test]
    async fn continuation_after_a_failed_branch_refuses_the_join_but_runs_the_branch_again() {
        let (_dir, app, mut r) = parallel_fixture().await;
        // As the engine leaves a run whose branch b failed: the Fork failed, nothing after it ran.
        r.states.insert("b".into(), "failed".into());
        r.outputs.insert("b".into(), AgentResult { status: "failed".into(), ..Default::default() });
        r.states.insert("fork".into(), "failed".into());
        for id in ["join", "review", "synthesis"] {
            r.states.insert(id.into(), "queued".into());
            r.outputs.remove(id);
        }
        r.status = "failed".into();
        r.current = "fork".into();
        r.last_node = Some("plan".into());
        app.store.save_run(1, &r).unwrap();
        let ids = candidates(&app, 1, &r);
        assert!(!ids.contains(&"join".into()));
        assert!(!ids.contains(&"review".into()));
        for id in ["fork", "a1", "a2", "b"] {
            assert!(ids.contains(&id.into()), "{id}");
        }
        let refused = prepare(&app, 1, &r.id, "join", true).unwrap_err().to_string();
        assert!(refused.contains("continua dal Fork"), "{refused}");
        let resumed = prepare(&app, 1, &r.id, "b", true).unwrap();
        assert_eq!(resumed.current, "fork");
        assert_eq!(resumed.states["b"], "queued");
        assert!(!resumed.outputs.contains_key("b"));
        for id in ["a1", "a2"] {
            assert_eq!(resumed.states[id], "completed");
            assert_eq!(resumed.outputs[id].summary, format!("{id} first pass"));
        }
    }
    /// A completed run of `w`, left as the engine leaves it: `skipped` Tasks are bypassed with
    /// no result, the Fork has no result of its own, every other step completed.
    async fn completed_run(app: &App, w: Workflow, done: &[&str], skipped: &[&str]) -> Run {
        let mut r = engine::create(app, 1, w, "test".into(), None)
            .await
            .unwrap();
        for id in done {
            r.states.insert((*id).into(), "completed".into());
            if r.workflow.node(id).unwrap().kind != "Fork" {
                r.outputs.insert((*id).into(), result(&format!("{id} first pass")));
            }
        }
        for id in skipped {
            r.states.insert((*id).into(), "bypassed".into());
        }
        r.status = "completed".into();
        app.store.save_run(1, &r).unwrap();
        r
    }
    /// Fork (the first step) → (a1 → a2 | b) → Join → human Review → synthesis.
    fn fork_first_workflow() -> Workflow {
        let mut w: Workflow = serde_json::from_value(serde_json::json!({
            "id":"fork-first-continue", "name":"Fork first continue", "description":"", "entry":"fork",
            "nodes":[{"id":"source","kind":"Source","source_type":"example"},
                {"id":"coordinator","kind":"Agent"},{"id":"aa","kind":"Agent"},{"id":"ab","kind":"Agent"},
                {"id":"fork","kind":"Fork","target":"join"},
                {"id":"a1","kind":"Task","agent":"aa","prompt":"A1"},
                {"id":"a2","kind":"Task","agent":"aa","prompt":"A2"},
                {"id":"b","kind":"Task","agent":"ab","prompt":"B"},
                {"id":"join","kind":"Join"},
                {"id":"review","kind":"Review","mode":"human","prompt":"Review"},
                {"id":"synthesis","kind":"Task","agent":"coordinator","prompt":"Synthesis"}],
            "edges":[]
        }))
        .unwrap();
        w.edges = vec![
            edge("source", "coordinator", "context", ""),
            edge("source", "aa", "context", ""),
            edge("source", "ab", "context", ""),
            edge("coordinator", "synthesis", "context", ""),
            edge("aa", "a1", "context", ""),
            edge("aa", "a2", "context", ""),
            edge("ab", "b", "context", ""),
            edge("fork", "a1", "control", ""),
            edge("fork", "b", "control", ""),
            edge("a1", "a2", "control", ""),
            edge("a2", "join", "control", ""),
            edge("b", "join", "control", ""),
            edge("join", "review", "control", ""),
            edge("review", "synthesis", "control", ""),
        ];
        engine::validate(&w).unwrap();
        w
    }
    #[tokio::test]
    async fn continuation_with_the_fork_as_first_step_arrives_from_no_previous_node() {
        let dir = tempfile::tempdir().unwrap();
        let app = crate::tests::app(dir.path());
        let all = ["fork", "a1", "a2", "b", "join", "review", "synthesis"];
        let r = completed_run(&app, fork_first_workflow(), &all, &[]).await;
        let ids = candidates(&app, 1, &r);
        for id in all {
            assert!(ids.contains(&id.into()), "{id}");
        }
        // A step inside a branch: the Fork runs again, with no step before it to arrive from.
        let resumed = prepare(&app, 1, &r.id, "a2", true).unwrap();
        assert_eq!(resumed.current, "fork");
        assert!(resumed.last_node.is_none());
        assert!(resumed.last_task.is_none());
        let shown = resumed.active_edge.clone().expect("the link into the chosen step");
        assert_eq!((shown.from.as_str(), shown.to.as_str()), ("a1", "a2"));
        for id in ["a1", "b"] {
            assert_eq!(resumed.states[id], "completed", "{id}");
            assert_eq!(resumed.outputs[id].summary, format!("{id} first pass"));
        }
        for id in ["fork", "a2", "join", "review", "synthesis"] {
            assert_eq!(resumed.states[id], "queued", "{id}");
            assert!(!resumed.outputs.contains_key(id), "{id}");
        }
        let mut ran = resumed.clone();
        ran.states.insert("a2".into(), "completed".into());
        ran.outputs.insert("a2".into(), result("a2 second pass"));
        app.store.save_run(1, &ran).unwrap();
        engine::drive(&app, 1, &r.id).await.unwrap();
        let saved = app.store.run(1, &r.id).unwrap();
        assert_eq!(saved.status, "waiting_review");
        assert_eq!(saved.pending.as_ref().unwrap().node, "review");
        assert_eq!(saved.states["fork"], "completed");
        assert_eq!(saved.states["join"], "completed");
        let results: serde_json::Value =
            serde_json::from_str(&saved.outputs["join"].summary).unwrap();
        assert_eq!(results["a1"]["summary"], "a1 first pass");
        assert_eq!(results["a2"]["summary"], "a2 second pass");
        assert_eq!(results["b"]["summary"], "b first pass");
        // The first step of a branch arrives by the Fork's link to it.
        let mut done = saved;
        done.pending = None;
        done.status = "completed".into();
        app.store.save_run(1, &done).unwrap();
        let resumed = prepare(&app, 1, &r.id, "b", true).unwrap();
        assert_eq!(resumed.current, "fork");
        assert!(resumed.last_node.is_none());
        let shown = resumed.active_edge.clone().unwrap();
        assert_eq!((shown.from.as_str(), shown.to.as_str()), ("fork", "b"));
        assert_eq!(resumed.outputs["a2"].summary, "a2 second pass");
        // The Join arrives from the Fork; no Task ran before it.
        let mut done = resumed;
        for id in ["fork", "b", "join", "review", "synthesis"] {
            done.states.insert(id.into(), "completed".into());
            if id != "fork" {
                done.outputs.insert(id.into(), result(&format!("{id} second pass")));
            }
        }
        done.status = "completed".into();
        app.store.save_run(1, &done).unwrap();
        let resumed = prepare(&app, 1, &r.id, "join", true).unwrap();
        assert_eq!(resumed.current, "join");
        assert_eq!(resumed.last_node.as_deref(), Some("fork"));
        assert!(resumed.last_task.is_none());
        assert!(resumed.active_edge.is_none());
        engine::drive(&app, 1, &r.id).await.unwrap();
        let saved = app.store.run(1, &r.id).unwrap();
        assert_eq!(saved.status, "waiting_review");
        let results: serde_json::Value =
            serde_json::from_str(&saved.outputs["join"].summary).unwrap();
        assert_eq!(results["b"]["summary"], "b second pass");
        assert_eq!(results["a1"]["summary"], "a1 first pass");
        // The Fork itself is the first step: continuing from it starts the run again.
        let mut done = saved;
        done.pending = None;
        done.status = "completed".into();
        app.store.save_run(1, &done).unwrap();
        let resumed = prepare(&app, 1, &r.id, "fork", true).unwrap();
        assert_eq!(resumed.current, "fork");
        assert!(resumed.last_node.is_none());
        assert!(resumed.active_edge.is_none());
        for id in all {
            assert_eq!(resumed.states[id], "queued", "{id}");
        }
    }
    /// plan → Fork → (a1 skipped → a2 | b1 → b2 skipped) → Join → human Review → synthesis.
    fn skipped_branches_workflow() -> Workflow {
        let mut w: Workflow = serde_json::from_value(serde_json::json!({
            "id":"skipped-continue", "name":"Skipped continue", "description":"", "entry":"plan",
            "nodes":[{"id":"source","kind":"Source","source_type":"example"},
                {"id":"coordinator","kind":"Agent"},{"id":"aa","kind":"Agent"},{"id":"ab","kind":"Agent"},
                {"id":"plan","kind":"Task","agent":"coordinator","prompt":"Plan"},
                {"id":"fork","kind":"Fork","target":"join"},
                {"id":"a1","kind":"Task","agent":"aa","skip":true},
                {"id":"a2","kind":"Task","agent":"aa","prompt":"A2"},
                {"id":"b1","kind":"Task","agent":"ab","prompt":"B1"},
                {"id":"b2","kind":"Task","agent":"ab","skip":true},
                {"id":"join","kind":"Join"},
                {"id":"review","kind":"Review","mode":"human","prompt":"Review"},
                {"id":"synthesis","kind":"Task","agent":"coordinator","prompt":"Synthesis"}],
            "edges":[]
        }))
        .unwrap();
        w.edges = vec![
            edge("source", "coordinator", "context", ""),
            edge("coordinator", "plan", "context", ""),
            edge("coordinator", "synthesis", "context", ""),
            edge("aa", "a1", "context", ""),
            edge("aa", "a2", "context", ""),
            edge("ab", "b1", "context", ""),
            edge("ab", "b2", "context", ""),
            edge("plan", "fork", "control", ""),
            edge("fork", "a1", "control", ""),
            edge("fork", "b1", "control", ""),
            edge("a1", "a2", "control", ""),
            edge("a2", "join", "control", ""),
            edge("b1", "b2", "control", ""),
            edge("b2", "join", "control", ""),
            edge("join", "review", "control", ""),
            edge("review", "synthesis", "control", ""),
        ];
        engine::validate(&w).unwrap();
        w
    }
    #[tokio::test]
    async fn continuation_counts_skipped_branch_tasks_as_done() {
        let dir = tempfile::tempdir().unwrap();
        let app = crate::tests::app(dir.path());
        let done = ["plan", "fork", "a2", "b1", "join", "review", "synthesis"];
        let r = completed_run(&app, skipped_branches_workflow(), &done, &["a1", "b2"]).await;
        let ids = candidates(&app, 1, &r);
        for id in ["fork", "a1", "a2", "b1", "b2", "join", "review", "synthesis"] {
            assert!(ids.contains(&id.into()), "{id}");
        }
        let joined = |run: &Run| -> serde_json::Value {
            serde_json::from_str(&run.outputs["join"].summary).unwrap()
        };
        let settle = |run: Run| {
            let mut run = run;
            run.pending = None;
            run.status = "completed".into();
            app.store.save_run(1, &run).unwrap();
        };
        // From the Join: the skipped Tasks stay bypassed and contribute nothing.
        let resumed = prepare(&app, 1, &r.id, "join", true).unwrap();
        assert_eq!(resumed.current, "join");
        assert_eq!(resumed.last_node.as_deref(), Some("fork"));
        for id in ["a1", "b2"] {
            assert_eq!(resumed.states[id], "bypassed", "{id}");
        }
        engine::drive(&app, 1, &r.id).await.unwrap();
        let saved = app.store.run(1, &r.id).unwrap();
        assert_eq!(saved.status, "waiting_review");
        assert_eq!(saved.states["join"], "completed");
        let results = joined(&saved);
        assert_eq!(results["a2"]["summary"], "a2 first pass");
        assert_eq!(results["b1"]["summary"], "b1 first pass");
        assert!(results.get("a1").is_none() && results.get("b2").is_none(), "{results}");
        settle(saved);
        // From the step after a skipped Task: the skipped one counts as done and is not run.
        let resumed = prepare(&app, 1, &r.id, "a2", true).unwrap();
        assert_eq!(resumed.current, "fork");
        assert_eq!(resumed.last_node.as_deref(), Some("plan"));
        let shown = resumed.active_edge.clone().expect("the link into the chosen step");
        assert_eq!((shown.from.as_str(), shown.to.as_str()), ("a1", "a2"));
        assert_eq!(resumed.states["a1"], "bypassed");
        assert_eq!(resumed.states["b2"], "bypassed");
        assert_eq!(resumed.states["a2"], "queued");
        let mut ran = resumed.clone();
        ran.states.insert("a2".into(), "completed".into());
        ran.outputs.insert("a2".into(), result("a2 second pass"));
        app.store.save_run(1, &ran).unwrap();
        engine::drive(&app, 1, &r.id).await.unwrap();
        let saved = app.store.run(1, &r.id).unwrap();
        assert_eq!(saved.status, "waiting_review");
        assert_eq!(saved.states["fork"], "completed");
        assert!(!saved.attempts.contains_key("a1") && !saved.attempts.contains_key("b2"));
        let results = joined(&saved);
        assert_eq!(results["a2"]["summary"], "a2 second pass");
        assert_eq!(results["b1"]["summary"], "b1 first pass");
        assert!(results.get("a1").is_none() && results.get("b2").is_none(), "{results}");
        settle(saved);
        // From the step before a skipped Task: the skipped one is queued, then bypassed again.
        let resumed = prepare(&app, 1, &r.id, "b1", true).unwrap();
        assert_eq!(resumed.current, "fork");
        let shown = resumed.active_edge.clone().unwrap();
        assert_eq!((shown.from.as_str(), shown.to.as_str()), ("fork", "b1"));
        assert_eq!(resumed.states["b2"], "queued");
        assert_eq!(resumed.states["a1"], "bypassed");
        let mut ran = resumed.clone();
        ran.states.insert("b1".into(), "completed".into());
        ran.outputs.insert("b1".into(), result("b1 second pass"));
        app.store.save_run(1, &ran).unwrap();
        engine::drive(&app, 1, &r.id).await.unwrap();
        let saved = app.store.run(1, &r.id).unwrap();
        assert_eq!(saved.status, "waiting_review");
        assert_eq!(saved.states["b2"], "bypassed");
        assert!(!saved.outputs.contains_key("b2"));
        let results = joined(&saved);
        assert_eq!(results["a2"]["summary"], "a2 second pass");
        assert_eq!(results["b1"]["summary"], "b1 second pass");
        settle(saved);
        // From the skipped Task itself: the Fork bypasses it again.
        let resumed = prepare(&app, 1, &r.id, "b2", true).unwrap();
        assert_eq!(resumed.current, "fork");
        let shown = resumed.active_edge.clone().unwrap();
        assert_eq!((shown.from.as_str(), shown.to.as_str()), ("b1", "b2"));
        engine::drive(&app, 1, &r.id).await.unwrap();
        let saved = app.store.run(1, &r.id).unwrap();
        assert_eq!(saved.status, "waiting_review");
        assert_eq!(saved.states["b2"], "bypassed");
        assert_eq!(joined(&saved)["b1"]["summary"], "b1 second pass");
    }
    #[tokio::test]
    async fn continuation_gives_back_the_rework_budget_of_the_steps_that_run_again() {
        let dir = tempfile::tempdir().unwrap();
        let app = crate::tests::app(dir.path());
        let mut w: Workflow = serde_json::from_value(serde_json::json!({
            "id":"rework-continue", "name":"Rework continue", "description":"", "entry":"t1",
            "nodes":[{"id":"source","kind":"Source","source_type":"example"},
                {"id":"agent","kind":"Agent"},
                {"id":"t1","kind":"Task","agent":"agent","prompt":"T1"},
                {"id":"r1","kind":"Review","mode":"human","prompt":"R1","max_rework":1},
                {"id":"t2","kind":"Task","agent":"agent","prompt":"T2"},
                {"id":"r2","kind":"Review","mode":"human","prompt":"R2","max_rework":1},
                {"id":"t3","kind":"Task","agent":"agent","prompt":"T3"}],
            "edges":[]
        }))
        .unwrap();
        w.edges = vec![
            edge("source", "agent", "context", ""),
            edge("agent", "t1", "context", ""),
            edge("agent", "t2", "context", ""),
            edge("agent", "t3", "context", ""),
            edge("t1", "r1", "control", ""),
            edge("r1", "t1", "rework", "rework"),
            edge("r1", "t2", "control", ""),
            edge("t2", "r2", "control", ""),
            edge("r2", "t2", "rework", "rework"),
            edge("r2", "t3", "control", ""),
        ];
        let mut r = engine::create(&app, 1, w, "test".into(), None)
            .await
            .unwrap();
        for id in ["t1", "r1", "t2", "r2", "t3"] {
            r.states.insert(id.into(), "completed".into());
            r.outputs.insert(id.into(), result(id));
        }
        // Both reviews used their only correction in the first pass.
        r.reworks.insert("r1".into(), 1);
        r.reworks.insert("r2".into(), 1);
        r.attempts.insert("t2".into(), 2);
        r.context.insert("supervisor:count".into(), "3".into());
        r.status = "completed".into();
        app.store.save_run(1, &r).unwrap();
        let mut resumed = prepare(&app, 1, &r.id, "t2", true).unwrap();
        // r2 runs again with its budget back; r1 does not run again and keeps its count.
        assert!(!resumed.reworks.contains_key("r2"));
        assert_eq!(resumed.reworks["r1"], 1);
        assert_eq!(resumed.attempts["t2"], 2);
        assert_eq!(resumed.context["supervisor:count"], "3");
        // The re-run review asks for a correction: it has one left.
        resumed.states.insert("t2".into(), "completed".into());
        resumed.outputs.insert("t2".into(), result("t2 again"));
        resumed.current = "r2".into();
        engine::route(&app, 1, &mut resumed, "rework").unwrap();
        assert_eq!(resumed.current, "t2");
        assert_eq!(resumed.reworks["r2"], 1);
        resumed.current = "r2".into();
        assert!(engine::route(&app, 1, &mut resumed, "rework").is_err());
    }
}
