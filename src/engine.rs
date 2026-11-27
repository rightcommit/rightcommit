use crate::{
    App,
    agents::{self, emit},
    connectors,
    model::*,
};
use anyhow::{Context, Result, bail, ensure};
use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
};

pub fn edge_allowed(a: &Node, b: &Node, kind: &str, outcome: &str) -> bool {
    if a.kind == "Comment" || b.kind == "Comment" {
        return false;
    }
    match kind {
        "context" => {
            ["MCP", "RAG"].contains(&a.kind.as_str()) && b.kind == "Agent"
                || a.kind == "Source"
                    && ["Agent", "Connection", "Workflow", "A2A"].contains(&b.kind.as_str())
                || a.kind == "Agent" && ["Task", "Review", "Gate"].contains(&b.kind.as_str()) && b.agent == a.id
        }
        "control" => {
            !["Source", "Agent", "MCP", "RAG"].contains(&a.kind.as_str())
                && !["Source", "Agent", "MCP", "RAG"].contains(&b.kind.as_str())
        }
        "rework" => {
            (a.kind == "Gate" || a.kind == "Review")
                && ["Task", "Workflow", "Memory"].contains(&b.kind.as_str())
                && outcome == "rework"
        }
        _ => false,
    }
}
// Parallel regions are deliberately structured: disjoint Task chains and one barrier.
pub(crate) fn parallel_branches(w: &Workflow, fork: &Node) -> Result<Vec<Vec<Node>>> {
    let join = fork_join(w, fork)?;
    let starts: Vec<_> = w
        .edges
        .iter()
        .filter(|e| e.from == fork.id && e.kind == "control")
        .collect();
    ensure!(starts.len() >= 2, "Fork {} non è configurato correttamente: servono almeno due rami, collega almeno due Task in uscita", label(fork));
    let mut seen = HashSet::new();
    let mut bindings = std::collections::HashMap::new();
    let mut branches = Vec::new();
    for (index, edge) in starts.iter().enumerate() {
        ensure!(edge.outcome.is_empty(), "Fork {}: i collegamenti in uscita non possono avere un esito, rimuovi l’esito {}", label(fork), edge.outcome);
        let mut current = edge.to.clone();
        let mut branch = Vec::new();
        while current != fork.target {
            let node = w.node(&current)?;
            ensure!(seen.insert(current.clone()), "Fork {}: il nodo {} appartiene a più rami o forma un ciclo, ogni ramo deve avere i propri Task fino al Join", label(fork), label(node));
            ensure!(node.kind == "Task", "Fork {}: il ramo contiene il nodo {} di tipo {}, i rami paralleli possono contenere solo Task", label(fork), label(node), node.kind);
            if let Some(owner) = bindings.insert(node.agent.clone(), index) {
                ensure!(owner == index, "Fork {}: il Task {} usa un Agent già impegnato in un altro ramo, assegna un Agent diverso a ogni ramo", label(fork), label(node));
            }
            ensure!(
                w.edges
                    .iter()
                    .filter(|e| e.to == current && e.kind != "context")
                    .count()
                    == 1,
                "Fork {}: il Task {} riceve un collegamento da fuori dal suo ramo, rimuovilo", label(fork), label(node)
            );
            let next: Vec<_> = w
                .edges
                .iter()
                .filter(|e| e.from == current && e.kind != "context")
                .collect();
            ensure!(
                next.len() == 1 && next[0].kind == "control" && next[0].outcome.is_empty(),
                "Fork {}: il Task {} deve avere un solo collegamento in uscita, senza esito, che prosegue fino al Join", label(fork), label(node)
            );
            branch.push(node.clone());
            current = next[0].to.clone();
        }
        ensure!(!branch.is_empty(), "Fork {}: un ramo arriva al Join senza alcun Task, inserisci almeno un Task", label(fork));
        branches.push(branch);
    }
    ensure!(
        w.edges
            .iter()
            .filter(|e| e.to == fork.target && e.kind != "context")
            .all(|e| seen.contains(&e.from)),
        "Join {}: riceve un collegamento che non arriva dai rami del Fork {}, rimuovilo", label(join), label(fork)
    );
    Ok(branches)
}
// The Join a Fork names in its field, with an error that says what to set when it is missing.
fn fork_join<'a>(w: &'a Workflow, fork: &Node) -> Result<&'a Node> {
    ensure!(!fork.target.is_empty(), "Fork {} non è configurato: scegli il Nodo di ricongiungimento, cioè il Join in cui si riuniscono i rami", label(fork));
    w.nodes.iter().find(|j| j.id == fork.target && j.kind == "Join").with_context(|| format!("Fork {} non è configurato correttamente: il Nodo di ricongiungimento non esiste più, scegli di nuovo il Join", label(fork)))
}
fn validate_parallel(w: &Workflow) -> Result<()> {
    let mut joins = HashSet::new();
    for fork in w.nodes.iter().filter(|n| n.kind == "Fork") {
        let join = fork_join(w, fork)?;
        ensure!(joins.insert(fork.target.clone()), "Fork {}: il Join {} è già usato da un altro Fork, scegli un Join diverso", label(fork), label(join));
        parallel_branches(w, fork)?;
    }
    for join in w.nodes.iter().filter(|n| n.kind == "Join") {
        ensure!(joins.contains(&join.id), "Join {} non è configurato correttamente: nessun Fork lo usa, sceglilo come Nodo di ricongiungimento nel Fork che apre i rami", label(join));
        ensure!(w.entry != join.id, "Il nodo {} non può essere il nodo di ingresso: usa come ingresso un nodo eseguibile, per esempio un Task", label(join));
        ensure!(
            w.edges
                .iter()
                .filter(|e| e.from == join.id && e.kind == "control")
                .count()
                <= 1,
            "Join {}: ha più collegamenti in uscita, lasciane uno solo", label(join)
        );
    }
    Ok(())
}

pub fn validate_draft(w: &Workflow) -> Result<()> {
    ensure!(
        !w.name.trim().is_empty()
            && w.name.len() < 160
            && w.nodes.len() <= 200
            && w.edges.len() <= 600,
        "Dimensioni o nome della bozza non validi"
    );
    let mut ids = HashSet::new();
    for n in &w.nodes {
        ensure!(
            !n.id.is_empty()
                && n.id.len() < 80
                && n.id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                && ids.insert(&n.id),
            "ID nodo non valido o duplicato"
        );
        ensure!(
            NODE_KINDS.contains(&n.kind.as_str()),
            "Tipo nodo non disponibile: {}",
            n.kind
        );
        ensure!(
            n.title.len() < 180
                && n.prompt.len() < 64000
                && n.text.len() <= 64000
                && n.x.is_finite()
                && n.y.is_finite()
                && n.width.is_finite()
                && n.height.is_finite(),
            "Configurazione nodo non valida"
        );
        ensure!(!n.skip || n.kind == "Task", "Salta task è disponibile solo sui nodi Task");
        if n.kind=="Agent" {
            ensure!(n.pi_provider.is_empty() || crate::pi_providers::known(&n.pi_provider),"Provider Pi non supportato: {}",n.pi_provider);
            ensure!(n.pi_base_url.len()<=2048,"URL del provider locale non valido");
            if !n.pi_base_url.is_empty(){crate::pi_local::validate(&AgentConfig {provider:"local".into(),base_url:n.pi_base_url.clone(),..Default::default()})?;}
        }
        if n.kind == "Comment" {
            ensure!(
                (n.width == 0.0 || (220.0..=1200.0).contains(&n.width))
                    && (n.height == 0.0 || (160.0..=1200.0).contains(&n.height)),
                "Dimensioni commento non valide"
            );
        }
        if !n.agent.is_empty() && ["Task", "Gate", "Review"].contains(&n.kind.as_str()) {
            ensure!(w.nodes.iter().any(|a| a.id == n.agent && a.kind == "Agent"), "{} {} non è configurato correttamente: l’Agent assegnato non esiste più, sceglilo di nuovo", n.kind, label(n));
        }
    }
    if !w.entry.is_empty() {
        entry(w)?;
    }
    let mut edges = HashSet::new();
    for e in &w.edges {
        let a = w.node(&e.from)?;
        let b = w.node(&e.to)?;
        ensure!(a.id != b.id, "Il nodo {} è collegato a sé stesso: rimuovi il collegamento", label(a));
        ensure!(edges.insert((&e.from, &e.to, &e.kind, &e.outcome)), "Collegamento duplicato da {} a {}: rimuovine uno", label(a), label(b));
        ensure!(!(a.kind == "Agent" && e.kind == "context" && ["Task", "Review", "Gate"].contains(&b.kind.as_str()) && b.agent != a.id), "Il nodo {} è collegato all’Agent {} ma ne ha assegnato un altro: rimuovi il collegamento oppure scegli {} come Agent del nodo", label(b), label(a), label(a));
        ensure!(edge_allowed(a, b, &e.kind, &e.outcome), "Collegamento non valido da {} a {}: un collegamento {} non può unire un nodo {} a un nodo {}, rimuovilo", label(a), label(b), e.kind, a.kind, b.kind);
    }
    Ok(())
}
// The entry node, with an error that says what to do when it is unset, gone or not a step.
fn entry(w: &Workflow) -> Result<&Node> {
    ensure!(!w.entry.is_empty(), "Il workflow non ha un nodo di ingresso: seleziona il primo nodo da eseguire e scegli Usa come ingresso");
    let n = w.nodes.iter().find(|n| n.id == w.entry).context("Il nodo di ingresso del workflow non esiste più: seleziona il primo nodo da eseguire e scegli Usa come ingresso")?;
    ensure!(!["Agent", "Source", "MCP", "RAG", "Comment"].contains(&n.kind.as_str()), "Il nodo {} non può essere il nodo di ingresso: usa come ingresso un nodo eseguibile, per esempio un Task", label(n));
    Ok(n)
}
// How an error names a node to the user: its title, or its id when it has none.
fn label(n: &Node) -> &str {
    if n.title.trim().is_empty() { &n.id } else { &n.title }
}
pub fn validate(w: &Workflow) -> Result<()> {
    validate_draft(w)?;
    crate::supervision::validate(&w.execution)?;
    validate_parallel(w)?;
    ensure!(
        !w.name.trim().is_empty() && w.name.len() < 160,
        "Nome workflow non valido"
    );
    ensure!(
        !w.nodes.is_empty() && w.nodes.len() <= 200 && w.edges.len() <= 600,
        "Il workflow richiede 1–200 nodi"
    );
    let mut ids = HashSet::new();
    for n in &w.nodes {
        ensure!(
            !n.id.is_empty()
                && n.id.len() < 80
                && n.id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                && ids.insert(&n.id),
            "ID nodo non valido o duplicato"
        );
        ensure!(
            NODE_KINDS.contains(&n.kind.as_str()),
            "Tipo nodo non valido"
        );
        ensure!(
            n.prompt.len() < 64000 && n.title.len() < 180 && n.x.is_finite() && n.y.is_finite(),
            "Configurazione nodo troppo grande o non valida"
        );
        if n.kind == "Comment" {
            ensure!(
                n.text.len() <= 64000
                    && n.width.is_finite()
                    && n.height.is_finite()
                    && (n.width == 0.0 || (220.0..=1200.0).contains(&n.width))
                    && (n.height == 0.0 || (160.0..=1200.0).contains(&n.height)),
                "Commento troppo grande o dimensioni non valide"
            );
        }
        if n.kind == "Agent" {
            ensure!(n.runtime.is_empty() || ["codex", "claude", "pi"].contains(&n.runtime.as_str()), "L’Agent {} non è configurato correttamente: il runtime {} non è supportato, scegli Codex, Claude Code o Pi", label(n), n.runtime);
        }
        if ["Task", "Gate"].contains(&n.kind.as_str()) || n.kind == "Review" && n.mode != "human" {
            ensure!(!n.agent.is_empty(), "{} {} non è configurato: scegli l’Agent che lo esegue", n.kind, label(n));
            ensure!(w.nodes.iter().any(|a| a.id == n.agent && a.kind == "Agent"), "{} {} non è configurato correttamente: l’Agent assegnato non esiste più, sceglilo di nuovo", n.kind, label(n));
        }
        if n.kind == "A2A" {
            ensure!(!n.a2a_id.is_empty(), "A2A {} non è configurato: scegli l’Agente remoto", label(n));
        }
        if n.kind == "RAG" {
            ensure!(!n.rag_index.is_empty(), "RAG {} non è configurato: scegli la Cartella indicizzata", label(n));
            ensure!((n.rag_mode.is_empty() || ["keyword", "semantic", "hybrid"].contains(&n.rag_mode.as_str())) && n.rag_top_k <= 30, "RAG {} non è configurato correttamente: scegli una Modalità di ricerca valida e al massimo 30 risultati", label(n));
        }
        if n.kind == "MCP" {
            ensure!(!n.mcp_id.is_empty(), "MCP {} non è configurato: scegli il Server MCP personale", label(n));
            ensure!(n.mcp_tools.len() <= 500 && n.mcp_tools.iter().all(|t| t.len() <= 256), "MCP {} non è configurato correttamente: seleziona al massimo 500 tool", label(n));
        }
        if n.kind == "Workflow" {
            ensure!(!n.workflow_id.is_empty(), "Workflow {} non è configurato: scegli il Workflow da richiamare", label(n));
            ensure!(n.workflow_id != w.id, "Workflow {} non è configurato correttamente: non può richiamare il workflow che lo contiene, scegline un altro", label(n));
        }
        if n.kind == "Task" && !n.skip {
            ensure!(!n.prompt.trim().is_empty(), "Task {} non è configurato: scrivi il prompt con Imposta prompt oppure attiva Salta task", label(n));
        }
        if n.kind == "Gate" {
            ensure!(!n.outcomes.is_empty(), "Gate {} non è configurato: aggiungi almeno un esito", label(n));
            ensure!(n.outcomes.len() < 12, "Gate {} non è configurato correttamente: ha troppi esiti, il massimo è 11", label(n));
            for o in &n.outcomes {
                ensure!(
                    o.len() < 60 && o.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
                    "Gate {} non è configurato correttamente: l’esito {} non è valido, usa meno di 60 caratteri tra lettere senza accenti, cifre e _", label(n), o.chars().take(60).collect::<String>()
                );
                if o != "needs_info" && o != "blocked" {
                    ensure!(
                        w.next(&n.id, o).is_some(),
                        "Gate {} senza ramo {}",
                        n.title,
                        o
                    );
                }
            }
        }
        if n.kind == "Memory" {
            // Name the Memory and the missing field: a bare "Nodo inesistente:" for an empty reference says nothing.
            ensure!(!n.agent.is_empty(), "Memory {} non è configurato: scegli il Task sorgente, cioè il Task di cui passare il risultato", label(n));
            ensure!(!n.target.is_empty(), "Memory {} non è configurato: scegli l’Agent destinatario, cioè l’Agent del Task che riceve il risultato", label(n));
            ensure!(w.nodes.iter().any(|t| t.id == n.agent && t.kind == "Task"), "Memory {} non è configurato correttamente: il Task sorgente non esiste più, sceglilo di nuovo", label(n));
            ensure!(w.nodes.iter().any(|a| a.id == n.target && a.kind == "Agent"), "Memory {} non è configurato correttamente: l’Agent destinatario non esiste più, sceglilo di nuovo", label(n));
            ensure!(!w.node(&n.agent)?.skip, "Memory {} non è configurato correttamente: serve il risultato del Task {}, disattiva Salta task o scegli un altro Task sorgente", label(n), label(w.node(&n.agent)?));
        }
    }
    entry(w)?;
    for e in &w.edges {
        let a = w.node(&e.from)?;
        let b = w.node(&e.to)?;
        ensure!(
            a.kind != "Comment" && b.kind != "Comment",
            "I commenti non hanno collegamenti"
        );
    }
    fn cycle(
        w: &Workflow,
        id: &str,
        vis: &mut HashSet<String>,
        done: &mut HashSet<String>,
    ) -> Option<String> {
        if done.contains(id) {
            return None;
        }
        if !vis.insert(id.into()) {
            return Some(id.into());
        }
        for e in w
            .edges
            .iter()
            .filter(|e| e.from == id && e.kind == "control")
        {
            if let Some(found) = cycle(w, &e.to, vis, done) {
                return Some(found);
            }
        }
        vis.remove(id);
        done.insert(id.into());
        None
    }
    // A correction branch starts at a rework edge: its steps are checked like the main path.
    let mut done = HashSet::new();
    for start in std::iter::once(&w.entry).chain(w.edges.iter().filter(|e| e.kind == "rework").map(|e| &e.to)) {
        if let Some(found) = cycle(w, start, &mut HashSet::new(), &mut done) {
            bail!("Ciclo nel percorso: dal nodo {} i collegamenti control tornano allo stesso nodo. Per ripetere un passo usa un collegamento rework con limite", label(w.node(&found)?));
        }
    }
    let main = main_path(w);
    let mut live = HashSet::new();
    let mut stack = vec![w.entry.clone()];
    while let Some(x) = stack.pop() {
        if !live.insert(x.clone()) {
            continue;
        }
        for e in w
            .edges
            .iter()
            .filter(|e| e.from == x && (e.kind == "control" || e.kind == "rework"))
        {
            stack.push(e.to.clone())
        }
    }
    for n in w.nodes.iter().filter(|n| n.kind != "Comment") {
        if !["Agent", "Source", "MCP", "RAG"].contains(&n.kind.as_str()) {
            ensure!(live.contains(&n.id), "Nodo non raggiungibile: {}", n.title);
        } else {
            // An Agent chosen in a Task, Review or Gate field is in use even without an edge.
            let assigned = n.kind == "Agent" && w.nodes.iter().any(|m| m.agent == n.id && ["Task", "Review", "Gate"].contains(&m.kind.as_str()));
            ensure!(
                assigned || w.edges.iter().any(|e| e.from == n.id || e.to == n.id),
                "Nodo senza collegamenti: {}",
                n.title
            );
        }
    }
    for n in w
        .nodes
        .iter()
        .filter(|n| ["Source", "Agent", "MCP", "RAG"].contains(&n.kind.as_str()))
    {
        let mut pending = vec![n.id.clone()];
        let mut seen = HashSet::new();
        let mut feeds = false;
        while let Some(x) = pending.pop() {
            if !seen.insert(x.clone()) {
                continue;
            }
            if live.contains(&x) {
                feeds = true;
                break;
            }
            // Task, Review and Gate bind their agent by field; an edge is optional.
            if w.node(&x).is_ok_and(|node| node.kind == "Agent") {
                pending.extend(
                    w.nodes
                        .iter()
                        .filter(|node| {
                            node.agent == x
                                && (node.kind == "Gate"
                                    || node.kind == "Task" && !node.skip
                                    || node.kind == "Review" && node.mode != "human")
                        })
                        .map(|node| node.id.clone()),
                );
            }
            for e in w
                .edges
                .iter()
                .filter(|e| e.from == x && e.kind == "context")
            {
                pending.push(e.to.clone());
            }
        }
        ensure!(feeds, "{} non è collegato al percorso eseguibile", n.title);
    }
    let mut routes = HashSet::new();
    for e in w
        .edges
        .iter()
        .filter(|e| e.kind == "control" || e.kind == "rework")
    {
        let from = w.node(&e.from)?;
        if from.kind != "Fork" && !routes.insert((&e.from, &e.outcome)) {
            if e.outcome.is_empty() { bail!("Il nodo {} ha più collegamenti in uscita: lasciane uno solo", label(from)); }
            bail!("Il nodo {} ha più collegamenti in uscita per l’esito {}: lasciane uno solo", label(from), e.outcome);
        }
    }
    // Fork/Join perform explicit automatic handoffs. Other binding changes still need Memory.
    for t in w.nodes.iter().filter(|n| n.kind == "Task") {
        let mut q = vec![(t.id.clone(), t.agent.clone())];
        let mut seen = HashSet::new();
        while let Some((x, binding)) = q.pop() {
            if !seen.insert((x.clone(), binding.clone())) {
                continue;
            }
            for e in w.edges.iter().filter(|e| e.from == x && follows(e, &main)) {
                let n = w.node(&e.to)?;
                if n.kind == "Task" {
                    ensure!(
                        n.agent == binding,
                        "{} → {}: cambio Agent senza Memory corretto",
                        t.title,
                        n.title
                    );
                } else if n.kind == "Fork" || n.kind == "Join" {
                    // Structured regions are checked separately; do not carry a single
                    // binding across fan-out or across the result barrier.
                    continue;
                } else if n.kind == "Memory" {
                    ensure!(reachable(w, &n.agent, &n.id), "Memory {} non è configurato correttamente: il Task sorgente {} non viene eseguito prima di questo Memory, scegli un Task che lo precede", label(n), label(w.node(&n.agent)?));
                    q.push((n.id.clone(), n.target.clone()));
                } else {
                    q.push((n.id.clone(), binding.clone()));
                }
            }
        }
    }
    Ok(())
}
/// The nodes reached from the entry through control edges alone.
fn main_path(w: &Workflow) -> HashSet<String> {
    let mut main = HashSet::new();
    let mut stack = vec![w.entry.clone()];
    while let Some(x) = stack.pop() {
        if !main.insert(x.clone()) {
            continue;
        }
        stack.extend(w.edges.iter().filter(|e| e.from == x && e.kind == "control").map(|e| e.to.clone()));
    }
    main
}
/// A rework edge back to the main path repeats a step whose Agent already has the context.
/// One that leaves the main path opens a correction branch: its steps follow on like control.
fn follows(e: &crate::model::Edge, main: &HashSet<String>) -> bool {
    e.kind == "control" || e.kind == "rework" && !main.contains(&e.to)
}
fn reachable(w: &Workflow, a: &str, b: &str) -> bool {
    let main = main_path(w);
    let mut seen = HashSet::new();
    let mut s = vec![a.to_string()];
    while let Some(x) = s.pop() {
        if x == b {
            return true;
        }
        if !seen.insert(x.clone()) {
            continue;
        }
        for e in w.edges.iter().filter(|e| e.from == x && follows(e, &main)) {
            s.push(e.to.clone());
        }
    }
    false
}

fn memory_source_instructions(workflow: &Workflow, task: &Node) -> &'static str {
    if workflow.nodes.iter().any(|node| {
        node.kind == "Memory"
            && node.agent == task.id
            && reachable(workflow, &task.id, &node.id)
    }) {
        "\nHandoff through Memory: in the details field of the result, or in the report before <nodo_result>, include \
         decisions and their reasons, constraints, files to consult, the next step and open questions. Separate what you \
         verified from what remains to be verified. Before answering, count the Unicode characters of the report (not tokens). \
         If it exceeds 16,000 characters, save the complete report in a Markdown file in the workspace, with a unique name \
         under .rightcommit/handoffs/. Check that the file exists. Put the relative path and the sections to read at the start of \
         summary, include the path in artifacts and pass in details or in the text report only a summary \
         within 16,000 characters. Tell the next agent to read the file before continuing."
    } else {
        ""
    }
}

/// The checks of the workflow a root run starts from, before anything depends on the user's
/// credentials: run creation calls it first, so a graph or configuration error is reported before
/// the refusal for an Agent without credentials.
pub fn validate_start(app: &App, uid: i64, w: &Workflow) -> Result<()> {
    crate::subworkflows::validate_tree(app, uid, w)?;
    let sources: Vec<_> = w.nodes.iter().filter(|n| n.kind == "Source").collect();
    ensure!(
        sources.iter().all(|n| !n.id.is_empty()
            && n.id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')),
        "ID Source non valido"
    );
    ensure!(!sources.is_empty(), "Aggiungi almeno un Source");
    Ok(())
}
pub async fn create(
    app: &App,
    uid: i64,
    w: Workflow,
    input: String,
    base_run: Option<String>,
) -> Result<Run> {
    create_reserved(app, uid, w, input, base_run, None).await
}
pub async fn create_reserved(
    app: &App, uid: i64, mut w: Workflow, input: String,
    base_run: Option<String>, reserved_id: Option<String>,
) -> Result<Run> {
    validate_start(app, uid, &w)?;
    // Starts without a person (schedules, A2A, API calls without the flag) never create folders.
    crate::storage::check_source_folders(app, &w, base_run.is_some(), false)?;
    let settings = app.store.settings(uid)?;
    let active_agents: HashSet<_> = w.nodes.iter().filter(|n| {
        n.kind == "Task" && !n.skip || n.kind == "Gate" || n.kind == "Review" && n.mode != "human"
    }).map(|n| n.agent.clone()).collect();
    for n in w.nodes.iter_mut().filter(|n| n.kind == "Agent") {
        if n.runtime.is_empty() {
            n.runtime = settings.default_runtime.clone();
        }
        if n.runtime=="pi" && active_agents.contains(&n.id) {
            let config=settings.agents.get("pi").cloned().unwrap_or_default();
            if config.multi_provider {
                let selected=crate::pi_providers::preflight(app,uid,&settings,&n.pi_provider,&n.model,&n.pi_base_url)
                    .map_err(|error|anyhow::anyhow!("Agent {}: {error}",n.title))?;
                n.pi_provider=selected.provider;n.pi_base_url=selected.base_url;n.model=selected.model;
            } else {n.pi_provider=config.provider;n.pi_base_url=config.base_url;}
        }
    }
    let rid = reserved_id.unwrap_or_else(id);
    let home = agents::user_home(app, uid)?;
    let run_folder = home.join("workspaces").join(&rid);
    let mut workspace = run_folder.clone();
    let mut source_context = BTreeMap::new();
    let sources: Vec<_> = w.nodes.iter().filter(|n| n.kind == "Source").collect();
    // Resolve every direct binding before changing permissions or preparing files.
    for (i, n) in sources.iter().enumerate() {
        if crate::source_access::direct(n) && !(i == 0 && base_run.is_some()) {
            let path = crate::storage::node_source_folder(app, n)?;
            let checked = path.clone();
            crate::artifacts::blocking(move || crate::source_access::validate_tree(&checked)).await?;
            source_context.insert(
                format!("direct-source:{}", n.id),
                path.to_string_lossy().into_owned(),
            );
            if i == 0 {
                source_context.insert("direct-primary".into(), n.id.clone());
                workspace = path;
            }
        }
    }
    crate::source_access::available(app, &crate::source_access::paths(&source_context), &[])?;
    let mut folders = vec![];
    for (i, n) in sources.iter().enumerate() {
        if let Some(path) = source_context.get(&format!("direct-source:{}", n.id)) {
            agents::own_tree_async(app, uid, Path::new(path)).await?;
            folders.push(PathBuf::from(path));
            continue;
        }
        let dest = if i == 0 {
            workspace.clone()
        } else {
            run_folder.join(".rightcommit/sources").join(&n.id)
        };
        folders.push(dest.clone());
        if i > 0 && source_context.contains_key("direct-primary") {
            source_context.insert(
                format!("source-path:{}", n.id),
                dest.to_string_lossy().into_owned(),
            );
        }
        if i == 0
            && let Some(base) = &base_run
        {
            let old = app.store.run(uid, base)?;
            ensure!(
                old.status == "completed",
                "Il run di partenza deve essere completato"
            );
            let prior:Vec<_>=app.store.decisions(uid,&old.id)?.into_iter().filter(|d|d.decision=="approve").map(|d|serde_json::json!({"id":d.id,"node":d.node,"user":d.user,"created":d.created})).collect();
            source_context.insert("source-run".into(),serde_json::to_string(&serde_json::json!({"run_id":old.id,"status":old.status,"historical_approvals":prior,"local_delivery":old.outputs.get("commit"),"note":"Historical provenance, not an approval of the new run. Revalidate its changes."}))?);
            agents::copy_tree_async(Path::new(&old.workspace), &dest).await?;
        } else {
            match n.source_type.as_str() {
                "example" | "" => {
                    agents::copy_tree_async(&app.assets.join("examples/helpdesk"), &dest).await?;
                    if n.location == "baseline" {
                        agents::copy_tree_async(&app.assets.join("examples/living-spec"), &dest).await?;
                    }
                }
                "folder" | "examples" => {
                    let namespace = if n.source_type == "examples" {
                        "examples"
                    } else {
                        "sources"
                    };
                    let src = if namespace == "sources" {
                        crate::storage::node_source_folder(app, n)?
                    } else {
                        crate::storage::source_folder(app, namespace, &n.location)?
                    };
                    if namespace == "sources" {
                        let (src, dest, limit) = (src.clone(), dest.clone(), settings.source_copy_limit_bytes()?);
                        crate::artifacts::blocking(move || agents::copy_source_tree(&src, &dest, limit)).await?;
                    } else {
                        agents::copy_tree_async(&src, &dest).await?;
                    }
                }
                "git" => {
                    let url = reqwest::Url::parse(&n.location)?;
                    ensure!(
                        url.scheme() == "https"
                            && url.username().is_empty()
                            && url.password().is_none(),
                        "Repo: usa un URL HTTPS senza credenziali"
                    );
                    std::fs::create_dir_all(&dest)?;
                    agents::own_tree_async(app, uid, &dest).await?;
                    agents::tool(
                        app,
                        uid,
                        &dest,
                        "git",
                        &["clone", "--depth", "1", "--", &n.location, "."],
                        180,
                    )
                    .await?;
                }
                _ => bail!("Tipo Source non supportato"),
            }
        }
    }
    agents::own_tree_async(app, uid, &workspace).await?;
    if run_folder.exists() {
        agents::own_tree_async(app, uid, &run_folder).await?;
    }
    let direct_primary = source_context.contains_key("direct-primary");
    if !direct_primary && !workspace.join(".git").exists() {
        agents::tool(app, uid, &workspace, "git", &["init", "-b", "main"], 30).await?;
        // The copy has no dependency folders; the exclude keeps those the run creates out of
        // `git add --all` and `git status` in the workspace.
        crate::local_commit::exclude_dependency_dirs(app, uid, &workspace).await?;
        agents::tool(app, uid, &workspace, "git", &["add", "--all"], 30).await?;
        agents::tool(
            app,
            uid,
            &workspace,
            "git",
            &[
                "-c",
                "core.hooksPath=/dev/null",
                "commit",
                "-m",
                "Baseline source",
            ],
            30,
        )
        .await?;
    }
    if !direct_primary {
        agents::tool(
            app,
            uid,
            &workspace,
            "git",
            &["checkout", "-b", &format!("nodo/{}", &rid[..8])],
            30,
        )
        .await?;
    }
    // Edits already in the workspace (a direct Source is the user's own folder) stay out of
    // the local commit. Recorded before setup, whose files belong to the run.
    if let Some(start) = crate::local_commit::start(app, uid, &workspace).await? {
        source_context.insert(crate::local_commit::START.into(), start);
    }
    // After the baseline commit, so files added here show up in the run's changes.
    // A workspace continued from an earlier run was already prepared.
    let mut prepared = vec![];
    for (i, (n, folder)) in sources.iter().zip(&folders).enumerate() {
        let fresh = !(i == 0 && base_run.is_some());
        for line in crate::source_setup::prepare(app, uid, n, folder, fresh).await? {
            prepared.push((n.id.clone(), line));
        }
    }
    let states = w
        .nodes
        .iter()
        .filter(|n| n.kind != "Comment")
        .map(|n| {
            (
                n.id.clone(),
                if ["Source", "Agent", "MCP", "RAG"].contains(&n.kind.as_str()) {
                    "completed"
                } else {
                    "queued"
                }
                .into(),
            )
        })
        .collect();
    let r = Run {
        execution: {
            let mut policy = w.execution.clone();
            if policy.runtime.is_empty() {
                policy.runtime = app.store.settings(uid)?.default_runtime;
            }
            policy
        },
        parent: None,
        children: Vec::new(),
        id: rid,
        workflow: w.clone(),
        status: "paused".into(),
        current: w.entry.clone(),
        created: now(),
        updated: now(),
        workspace: workspace.to_string_lossy().into(),
        states,
        sessions: BTreeMap::new(),
        outputs: BTreeMap::new(),
        pending: None,
        active_edge: entry_edge(&w),
        pause_requested: false,
        attempts: BTreeMap::new(),
        reworks: BTreeMap::new(),
        context: source_context,
        last_task: None,
        last_node: None,
        error: None,
        input,
    };
    // Name a missing skill now rather than when the run reaches its Task.
    for n in r.workflow.nodes.iter().filter(|n| n.kind == "Task" && !n.skip && !n.command.trim().is_empty()) {
        let (rt, ..) = runtime(&r, &n.agent)?;
        agents::resolve_command(&n.command, &rt, Path::new(&r.workspace))
            .map_err(|e| anyhow::anyhow!("Task {}: {e}", n.title))?;
    }
    crate::artifacts::baseline_async(app, uid, &r).await?;
    app.store.save_run(uid, &r)?;
    emit(
        app,
        uid,
        &r.id,
        "created",
        "",
        "Source preparate secondo la modalità selezionata. Nessun risultato simulato.",
    );
    for (node, line) in prepared {
        emit(app, uid, &r.id, "output", &node, &line);
    }
    Ok(r)
}
#[derive(Clone)]
pub struct RunControl {
    pub stop: tokio::sync::watch::Sender<bool>,
    pub done: tokio::sync::watch::Receiver<bool>,
}
/// The edge a new run shows entering its first node: the context link into the entry.
fn entry_edge(w: &Workflow) -> Option<Edge> {
    w.edges
        .iter()
        .find(|e| e.to == w.entry && e.kind == "context")
        .cloned()
}
/// The edge shown entering `node` when the run reaches it with no recorded route,
/// as after a resume or a restart. A restart from the first node shows the same
/// edge as the run's first start.
pub(crate) fn arrival_edge(r: &Run, node: &str) -> Option<Edge> {
    match &r.last_node {
        Some(previous) => r
            .workflow
            .edges
            .iter()
            .find(|e| e.to == node && &e.from == previous)
            .cloned(),
        None if node == r.workflow.entry => entry_edge(&r.workflow),
        None => None,
    }
}
pub fn launch(app: App, uid: i64, rid: String) -> Result<()> {
    crate::auth::ensure_active(&app, uid)?;
    let mut queued = app.store.run(uid, &rid)?;
    {
        let mut busy = app.busy.lock().unwrap();
        crate::subworkflows::can_launch_reserved(&app, uid, &queued, &busy)?;
        ensure!(busy.insert(rid.clone()), "Esecuzione già attiva");
    }
    queued.status = "starting".into();
    queued.pause_requested = false;
    if let Err(e) = save(&app, uid, &mut queued) {
        app.busy.lock().unwrap().remove(&rid);
        return Err(e);
    }
    let (stop, mut stopped) = tokio::sync::watch::channel(false);
    let (finished, done) = tokio::sync::watch::channel(false);
    app.workers
        .lock()
        .unwrap()
        .insert(rid.clone(), RunControl { stop, done });
    tokio::spawn(async move {
        // Preparation is joined before honoring Stop: filesystem work must not
        // outlive the worker's done notification and race workspace deletion.
        let (scan_app, scan_run) = (app.clone(), queued.clone());
        let prepared = crate::artifacts::blocking(move || {
            let lineage = crate::subworkflows::ancestors(&scan_app, uid, &scan_run)?;
            let root = lineage.last().unwrap_or(&scan_run);
            for path in crate::source_access::paths(&root.context) {
                crate::source_access::validate_tree(&path)?;
                agents::own_tree(&scan_app, uid, &path)?;
            }
            crate::artifacts::baseline(&scan_app, uid, &scan_run)
        }).await;
        // Dropping drive drops active CLI process-group guards. A final snapshot
        // joins any in-flight snapshot via the per-run inventory lock.
        let result = match prepared {
            Err(error) => Some(Err(error)),
            Ok(()) => tokio::select! {
                biased;
                _ = stopped.changed() => None,
                result = async {
                    let _permit = app.slots.acquire(&app.store, uid).await;
                    crate::auth::ensure_active(&app, uid)?;
                    drive_supervised(&app, uid, &rid).await
                } => Some(result),
            },
        };
        let cancelled = result.is_none();
        if (cancelled || result.as_ref().is_some_and(|r| r.is_err()))
            && let Ok(run) = app.store.run(uid, &rid)
            && let Err(error) = crate::artifacts::snapshot_async(&app, uid, &run, &run.current, "", "").await
        {
            emit(
                &app,
                uid,
                &rid,
                "error",
                &run.current,
                &format!("Raccolta artefatti: {error}"),
            );
        }
        if let Some(Err(e)) = result
            && let Ok(mut r) = app.store.run(uid, &rid)
        {
            r.status = "failed".into();
            r.error = Some(e.to_string());
            r.states.insert(r.current.clone(), "failed".into());
            let _ = save(&app, uid, &mut r);
            emit(&app, uid, &rid, "error", &r.current, &e.to_string());
        }
        app.busy.lock().unwrap().remove(&rid);
        // A waiting parent continues to watch the workspace shared by its children.
        if !app.store.run(uid, &rid).is_ok_and(|r| r.status == "waiting_child") {
            crate::artifacts::stop_monitor(&app, uid, &rid);
        }
        app.workers.lock().unwrap().remove(&rid);
        let _ = finished.send(true);
        if cancelled {
            return;
        }
        let _guard = app.lifecycle.read().await;
        if let Ok(r) = app.store.run(uid, &rid) {
            if r.status == "completed" {
                app.mcp.release_run(uid, &rid).await;
            }
            let _ = crate::subworkflows::settle(&app, uid, &r);
        }
    });
    Ok(())
}
pub(crate) fn save(app: &App, uid: i64, r: &mut Run) -> Result<()> {
    r.updated = now();
    if r.workflow.node(&r.current).is_ok_and(|n| n.kind == "Fork") {
        let live = app.store.run(uid, &r.id)?;
        r.sessions.extend(live.sessions);
        r.pause_requested |= live.pause_requested;
    }
    app.store.save_run(uid, r)?;
    emit(app, uid, &r.id, "state", &r.current, &r.status);
    crate::subworkflows::notify_parent(app, uid, r)?;
    Ok(())
}
fn runtime(r: &Run, agent: &str) -> Result<(String, String, String, String)> {
    let a = r.workflow.node(agent)?;
    ensure!(a.kind == "Agent", "Associazione Agent non valida");
    Ok((a.runtime.clone(), a.model.clone(), a.pi_provider.clone(), a.pi_base_url.clone()))
}
fn incoming(app: &App, uid: i64, r: &Run) -> Result<String> {
    let sources: Vec<_> = r.workflow.nodes.iter().filter(|n| n.kind == "Source").enumerate().map(|(i,n)| {
        let agents: Vec<_> = r.workflow.edges.iter().filter(|e|e.from==n.id && e.kind=="context").map(|e|e.to.as_str()).collect();
        serde_json::json!({"id":n.id,"title":n.title,"workspace_path":crate::source_access::source_path(r, i, n),"attached_agents":agents})
    }).collect();
    let output = r
        .last_node
        .as_ref()
        .and_then(|id| r.outputs.get(id))
        .map(serde_json::to_string)
        .transpose()?
        .unwrap_or_default();
    let approvals: Vec<_> = app
        .store
        .decisions(uid, &r.id)?
        .into_iter()
        .filter(|d| {
            d.decision == "approve" && r.states.get(&d.node).is_some_and(|s| s == "completed")
        })
        .collect();
    let human_steps: Vec<_> = r
        .workflow
        .nodes
        .iter()
        .filter(|n| n.kind == "Review" && n.mode != "agent")
        .map(|n| serde_json::json!({"id":n.id,"status":r.states.get(&n.id)}))
        .collect();
    Ok(format!(
        "Available Sources (paths relative to the workspace, or absolute for direct sources; Agent bindings): {}\nOriginal run request: {}\nLater authenticated clarification (takes precedence over ambiguities in the original request): {}\nSource provenance: {}\nApprovals actually recorded by the server: {}\nPlanned approval Reviews and their states: {}\nThe server handles approval requests according to the run's policy: do not attest future outcomes, but do not require as a technical prerequisite a human step explicitly planned downstream.\nPrevious output: {}\nAuthenticated answer for this node: {}",
        r.context
            .get("inherited-sources")
            .cloned()
            .unwrap_or(serde_json::to_string(&sources)?),
        r.input,
        r.context.get("user-note").cloned().unwrap_or_default(),
        r.context.get("source-run").cloned().unwrap_or_default(),
        serde_json::to_string(&approvals)?,
        serde_json::to_string(&human_steps)?,
        format_args!(
            "{output}\nExecution policy: {}",
            serde_json::to_string(&r.execution)?
        ),
        r.context
            .get(&format!("answer:{}", r.current))
            .cloned()
            .unwrap_or_default()
    ))
}
fn pause_for(
    app: &App,
    uid: i64,
    r: &mut Run,
    kind: &str,
    questions: Vec<String>,
    resume_task: Option<String>,
) -> Result<()> {
    pause_with(app, uid, r, kind, questions, resume_task, None)
}
fn pause_with(
    app: &App,
    uid: i64,
    r: &mut Run,
    kind: &str,
    questions: Vec<String>,
    resume_task: Option<String>,
    commit: Option<crate::local_commit::Preview>,
) -> Result<()> {
    r.status = if kind == "review" || kind == "effect" {
        "waiting_review"
    } else {
        "waiting_user"
    }
    .into();
    r.states.insert(r.current.clone(), r.status.clone());
    r.pending = Some(Pending {
        id: id(),
        proposal: None,
        kind: kind.into(),
        node: r.current.clone(),
        questions,
        resume_task,
        commit,
    });
    save(app, uid, r)
}
pub(crate) fn route(app: &App, uid: i64, r: &mut Run, outcome: &str) -> Result<()> {
    route_state(r, outcome)?;
    save(app, uid, r)
}
fn route_state(r: &mut Run, outcome: &str) -> Result<()> {
    let edge = r.workflow.next(&r.current, outcome).cloned();
    if let Some(e) = edge {
        if e.kind == "rework" {
            let count = r.reworks.entry(e.from.clone()).or_default();
            *count += 1;
            let max = r.workflow.node(&e.from)?.max_rework.clamp(1, 5);
            ensure!(*count <= max, "Limite di rework ({max}) raggiunto");
            let feedback = r
                .context
                .remove("review-feedback")
                .or_else(|| {
                    r.outputs
                        .get(&e.from)
                        .and_then(|o| serde_json::to_string(o).ok())
                })
                .unwrap_or_else(|| "Review rejected: examine the current artifacts again.".into());
            // A correction branch may open with a Memory: the finding is for the Task after it.
            let mut receiver = e.to.clone();
            let mut hops = 0;
            while r.workflow.node(&receiver)?.kind == "Memory" && hops < 200 {
                let Some(next) = r.workflow.next(&receiver, "") else { break };
                receiver = next.to.clone();
                hops += 1;
            }
            r.context.insert(format!("feedback:{receiver}"), feedback);
            // Invalidate every reachable downstream result/approval but keep sessions and artifact versions.
            let mut seen = HashSet::new();
            let mut q = vec![e.to.clone()];
            while let Some(x) = q.pop() {
                if !seen.insert(x.clone()) {
                    continue;
                }
                r.states.insert(x.clone(), "queued".into());
                r.outputs.remove(&x);
                r.context.remove(&format!("human-approved:{x}"));
                r.context.remove(&format!("answer:{x}"));
                for e in r
                    .workflow
                    .edges
                    .iter()
                    .filter(|e| e.from == x && e.kind == "control")
                {
                    q.push(e.to.clone());
                }
            }
        }
        r.last_node = Some(r.current.clone());
        r.current = e.to.clone();
        r.active_edge = Some(e);
        r.status = "running".into();
    } else {
        ensure!(
            outcome.is_empty() || outcome == "pass",
            "Nessun ramo per esito {outcome}"
        );
        r.status = "completed".into();
        r.active_edge = None;
        for s in r.states.values_mut() {
            if s == "queued" {
                *s = "skipped".into();
            }
        }
    }
    Ok(())
}
pub(crate) fn tests_current(r: &Run) -> Result<bool> {
    let proofs: Vec<_> = r
        .workflow
        .nodes
        .iter()
        .filter(|n| n.kind == "Connection" && n.provider == "local" && n.operation == "test")
        .filter(|n| {
            r.states.get(&n.id).is_some_and(|s| s == "completed")
                && r.outputs
                    .get(&n.id)
                    .is_some_and(|o| o.status == "completed" && o.decision == "pass")
        })
        .filter_map(|n| r.context.get(&format!("test-proof:{}", n.id)))
        .collect();
    if proofs.is_empty() {
        return Ok(false);
    }
    let current = agents::workspace_fingerprint(Path::new(&r.workspace))?;
    Ok(proofs.iter().any(|proof| **proof == current))
}

async fn tests_current_async(run: &Run) -> Result<bool> {
    let run = run.clone();
    crate::artifacts::blocking(move || tests_current(&run)).await
}

async fn drive_supervised(app: &App, uid: i64, rid: &str) -> Result<()> {
    loop {
        if app.store.run(uid, rid)?.pending.is_some()
            && !crate::supervision::resolve(app, uid, rid).await?
        {
            return Ok(());
        }
        drive(app, uid, rid).await?;
        if app.store.run(uid, rid)?.pending.is_none() {
            return Ok(());
        }
    }
}

// A bypass is an explicit execution state, distinct from an unvisited branch.
// Do not synthesize an agent result or carry a stale result across a retry.
fn bypass_task(r: &mut Run, n: &Node) {
    r.states.insert(n.id.clone(), "bypassed".into());
    r.outputs.remove(&n.id);
    if r.last_task.as_ref() == Some(&n.id) { r.last_task = None; }
}

pub(crate) async fn drive(app: &App, uid: i64, rid: &str) -> Result<()> {
    let mut r = app.store.run(uid, rid)?;
    if r.pause_requested {
        r.status = "paused".into();
        r.pause_requested = false;
        save(app, uid, &mut r)?;
        return Ok(());
    }
    ensure!(
        r.pending.is_none(),
        "Rispondi alla review prima di continuare"
    );
    r.status = "running".into();
    r.error = None;
    r.pause_requested = false;
    save(app, uid, &mut r)?;
    if crate::subworkflows::resume_child(app, uid, &mut r)? {
        return Ok(());
    }
    for _ in 0..500 {
        let live = app.store.run(uid, rid)?;
        if live.pause_requested {
            r.status = "paused".into();
            r.pause_requested = false;
            save(app, uid, &mut r)?;
            return Ok(());
        }
        if r.status != "running" {
            return Ok(());
        }
        let mut n = r.workflow.node(&r.current)?.clone();
        if n.kind == "Connection" && n.provider == "servicenow" && connectors::is_write(&n) {
            let settings = app.store.settings(uid)?;
            let config = settings.connections.get("servicenow").context("Configura ServiceNow nelle Configurazioni personali")?;
            n.command = serde_json::to_string(&crate::servicenow::prepare(&n, &r, config)?)?;
        }
        if r.active_edge.is_none() {
            r.active_edge = arrival_edge(&r, &n.id);
        }
        if n.kind == "Task" && n.skip {
            bypass_task(&mut r, &n);
            route(app, uid, &mut r, "")?;
            continue;
        }
        if n.kind == "Fork" {
            execute_parallel(app, uid, &mut r, &n).await?;
            continue;
        }
        // The local commit asks like an external write, with the exact content as the effect.
        // Unattended runs have nobody to ask: the commit goes ahead with its usual checks.
        let mut commit_digest = None;
        if connectors::confirms_commit(&n) && r.execution.mode != ExecutionMode::Unattended {
            let (mut preview, digest) = crate::local_commit::preview(app, uid, &r).await?;
            let approved = r.context.remove(&format!("effect:{}", n.id));
            // Nothing to commit needs no approval; the digest still stops anything new.
            if !preview.files.is_empty() && approved.as_deref() != Some(&digest) {
                preview.changed = approved.is_some();
                r.context.insert(format!("effect-preview:{}", n.id), digest);
                // Server-written text that reaches agents (the supervisor reads the request) is English;
                // web/i18n.js shows it in the interface language.
                let question = format!(
                    "{}Confirm the local commit (files: {})? Check the files, the changes and the tests before approving.",
                    if preview.changed { "The files changed after the approval. " } else { "" },
                    preview.files.len() + preview.more_files
                );
                return pause_with(app, uid, &mut r, "effect", vec![question], None, Some(preview));
            }
            commit_digest = Some((digest, approved.is_some()));
        }
        if n.kind == "Connection" && connectors::is_write(&n) {
            let digest = connectors::effect_digest(&n, &r, &app.store.settings(uid)?);
            if r.context.remove(&format!("effect:{}", n.id)).as_deref() != Some(&digest) {
                r.context.insert(format!("effect-preview:{}", n.id), digest);
                return pause_for(
                    app,
                    uid,
                    &mut r,
                    "effect",
                    vec![format!(
                        "Authorize {} on {}?{} Check the target and the artifacts in the settings and in the run.",
                        n.operation,
                        n.provider,
                        if n.provider == "github" && n.operation == "api_request" {
                            let settings=app.store.settings(uid)?;
                            let config=settings.connections.get("github").context("Configura GitHub nelle Configurazioni personali")?;
                            format!(" {}", agents::redact(&crate::github_api::preview(&n,config)?, &settings))
                        } else if n.provider == "atlassian" {
                            format!(" twg {}", n.command)
                        } else if n.provider == "servicenow" {
                            format!(" {}", agents::redact(&n.command, &app.store.settings(uid)?))
                        } else {
                            String::new()
                        }
                    )],
                    None,
                );
            }
        }
        if n.kind == "Workflow" {
            crate::subworkflows::invoke(app, uid, &mut r, &n)?;
            return Ok(());
        }
        r.states.insert(
            n.id.clone(),
            if ["Connection", "A2A"].contains(&n.kind.as_str()) {
                "waiting_external"
            } else {
                "running"
            }
            .into(),
        );
        *r.attempts.entry(n.id.clone()).or_default() += 1;
        save(app, uid, &mut r)?;
        let mut outcome = String::new();
        let result = match n.kind.as_str() {
            "Join" => {
                let output = r
                    .outputs
                    .get(&n.id)
                    .cloned()
                    .context("Join raggiunto senza risultati paralleli")?;
                join_handoffs(app, uid, &mut r, &n)?;
                output
            }
            "Task" => {
                let (rt, model, provider, base_url) = runtime(&r, &n.agent)?;
                let cmd = agents::resolve_command(&n.command, &rt, Path::new(&r.workspace))?;
                let scope = "Full tools: you can read, write and edit files and run the commands you need in the workspace. Respect the scope of the SDD phase: prepare and correct the documents before the implementation, without anticipating future tasks that were not requested.";
                let handoff = r
                    .context
                    .remove(&format!("handoff:{}", n.agent))
                    .unwrap_or_default();
                let answer = r
                    .context
                    .remove(&format!("answer:{}", n.id))
                    .unwrap_or_default();
                let feedback = r
                    .context
                    .remove(&format!("feedback:{}", n.id))
                    .unwrap_or_default();
                let memory_instructions = memory_source_instructions(&r.workflow, &n);
                let prompt = format!(
                    "{cmd}\n{scope}\nAgent binding: {}\nStep instructions:\n{}{memory_instructions}\n{}\nExplicit handoff: {handoff}\nRecorded answers: {answer}\nFindings to fix in this rework: {feedback}",
                    n.agent,
                    n.prompt,
                    incoming(app, uid, &r)?
                );
                let out = agents::turn(
                    app,
                    uid,
                    &mut r,
                    agents::Turn {
                        binding: &n.agent,
                        runtime: &rt,
                        model_override: &model,
                        provider_override: &provider,
                        base_url_override: &base_url,
                        prompt: &prompt,
                    },
                )
                .await?;
                crate::artifacts::snapshot_async(app, uid, &r, &n.id, &n.agent, &out.report()).await?;
                r.last_task = Some(n.id.clone());
                out
            }
            "Review" => {
                if let Some(receipt) = r.context.remove(&format!("human-approved:{}", n.id)) {
                    let mut out = r.outputs.get(&n.id).cloned().unwrap_or(AgentResult {
                        status: "completed".into(),
                        summary: "Review approved".into(),
                        ..Default::default()
                    });
                    out.summary
                        .push_str(&format!("\nApproval recorded by the server: {receipt}"));
                    out
                } else {
                    if n.mode != "human" {
                        let (rt, model, provider, base_url) = runtime(&r, &n.agent)?;
                        let prompt = format!(
                            "REVIEW. Step instructions:\n{}\n{}\nYou can edit files and run the commands needed to verify and correct the work. Report the actual changes and tests. Return decision pass or rework.",
                            n.prompt,
                            incoming(app, uid, &r)?
                        );
                        let out = agents::turn(
                            app,
                            uid,
                            &mut r,
                            agents::Turn {
                                binding: &format!("review:{}", n.id),
                                runtime: &rt,
                                model_override: &model,
                        provider_override: &provider,
                        base_url_override: &base_url,
                                prompt: &prompt,
                            },
                        )
                        .await?;
                        crate::artifacts::snapshot_async(
                            app,
                            uid,
                            &r,
                            &n.id,
                            &format!("review:{}", n.id),
                            &out.report(),
                        ).await?;
                        if out.status == "needs_input" {
                            r.outputs.insert(n.id.clone(), out.clone());
                            let last = r.last_task.clone();
                            return pause_for(app, uid, &mut r, "question", out.questions, last);
                        }
                        ensure!(
                            out.status == "completed",
                            "Review bloccata: {}",
                            out.summary
                        );
                        if out.decision == "rework" {
                            r.outputs.insert(n.id.clone(), out.clone());
                            rework_review(app, uid, &mut r)?;
                            continue;
                        }
                        ensure!(out.decision == "pass", "Review senza decision pass/rework");
                        r.outputs.insert(n.id.clone(), out);
                    }
                    if n.mode != "agent" {
                        return pause_for(app, uid, &mut r, "review", vec![n.prompt.clone()], None);
                    }
                    r.outputs.get(&n.id).cloned().unwrap()
                }
            }
            "Gate" => {
                if n.require_tasks_closed {
                    let open = open_tasks(Path::new(&r.workspace), &n.location)?;
                    let reported_rework = r
                        .last_node
                        .as_ref()
                        .and_then(|id| r.outputs.get(id))
                        .is_some_and(|o| o.decision == "rework");
                    if open > 0 || reported_rework {
                        ensure!(
                            n.outcomes.iter().any(|s| s == "rework"),
                            "Gate di convergenza senza esito rework"
                        );
                        let summary = format!(
                            "Convergence not reached: tasks still open: {open}; previous report rework={reported_rework}. Deterministic check, no approval simulated."
                        );
                        r.outputs.insert(
                            n.id.clone(),
                            AgentResult {
                                status: "completed".into(),
                                summary: summary.clone(),
                                decision: "rework".into(),
                                ..Default::default()
                            },
                        );
                        crate::artifacts::snapshot_async(app, uid, &r, &n.id, "", &summary).await?;
                        r.states.insert(n.id.clone(), "completed".into());
                        route(app, uid, &mut r, "rework")?;
                        continue;
                    }
                }
                if n.require_tests && !tests_current_async(&r).await? {
                    ensure!(
                        n.outcomes.iter().any(|o| o == "rework"),
                        "Quality gate: riesegui i test deterministici sul workspace corrente"
                    );
                    let summary="The server's tests are missing or no longer valid for the current files. Keep the fixes and go back through implementation, CI and review before delivery.".to_string();
                    crate::artifacts::snapshot_async(app, uid, &r, &n.id, "", &summary).await?;
                    r.outputs.insert(
                        n.id.clone(),
                        AgentResult {
                            status: "completed".into(),
                            decision: "rework".into(),
                            summary,
                            ..Default::default()
                        },
                    );
                    r.states.insert(n.id.clone(), "completed".into());
                    route(app, uid, &mut r, "rework")?;
                    continue;
                }
                let (rt, model, provider, base_url) = runtime(&r, &n.agent)?;
                let prompt = format!(
                    "Decision GATE with full tools. Step instructions:\n{}\n{}\nAllowed outcomes: {}. Put exactly one of these outcomes in the decision field. You can edit files and run the commands you need; report every change and validate the result again. If a human decision is missing, use status needs_input.",
                    n.prompt,
                    incoming(app, uid, &r)?,
                    n.outcomes.join(", ")
                );
                let mut out = agents::turn(
                    app,
                    uid,
                    &mut r,
                    agents::Turn {
                        binding: &format!("gate:{}", n.id),
                        runtime: &rt,
                        model_override: &model,
                        provider_override: &provider,
                        base_url_override: &base_url,
                        prompt: &prompt,
                    },
                )
                .await?;
                if out.status == "completed" {
                    if n.require_tests && out.decision == "pass" && !tests_current_async(&r).await? {
                        ensure!(
                            n.outcomes.iter().any(|o| o == "rework"),
                            "Il gate ha cambiato i file dopo CI: nuova verifica richiesta"
                        );
                        out.decision = "rework".into();
                        // On its own paragraph: the interface translates it, the Gate's own summary stays as written.
                        out.summary = format!(
                            "The gate changed the workspace after the tests: CI and review must run again.\n\n{}",
                            out.summary
                        );
                    }
                    ensure!(
                        n.outcomes.contains(&out.decision),
                        "Esito Gate non ammesso: {}",
                        out.decision
                    );
                    outcome = out.decision.clone();
                }
                crate::artifacts::snapshot_async(
                    app,
                    uid,
                    &r,
                    &n.id,
                    &format!("gate:{}", n.id),
                    &out.report(),
                ).await?;
                out
            }
            "Memory" => {
                let details_truncated = r.outputs.get(&n.agent)
                    .context("Memory: output Task sorgente assente")?
                    .details_truncated;
                let text = handoff_package(app, uid, &r, &n.agent, &n.target)?;
                r.context
                    .insert(format!("handoff:{}", n.target), text.clone());
                crate::artifacts::snapshot_async(app, uid, &r, &n.id, "", &text).await?;
                AgentResult {
                    status: "completed".into(),
                    summary: text,
                    details_truncated,
                    ..Default::default()
                }
            }
            "A2A" => {
                let out = crate::a2a::execute(app, uid, &mut r, &n).await?;
                if out.status == "completed" {
                    crate::artifacts::snapshot_async(app, uid, &r, &n.id, "", &out.report()).await?;
                }
                out
            }
            "Connection" if connectors::confirms_commit(&n) => {
                match crate::local_commit::commit(app, uid, &r, commit_digest.as_ref().map(|(digest, _)| digest.as_str())).await? {
                    Some(mut summary) => {
                        if commit_digest.is_none() {
                            summary.push_str("\nCommitted without confirmation: the run is in Unattended mode, so nobody could approve it.");
                        }
                        let out = AgentResult { status: "completed".into(), summary, decision: "pass".into(), ..Default::default() };
                        crate::artifacts::snapshot_async(app, uid, &r, &n.id, "", &out.report()).await?;
                        out
                    }
                    // Changed after the check above: ask again, saying so if it had been approved.
                    None => {
                        if let Some((digest, true)) = commit_digest {
                            r.context.insert(format!("effect:{}", n.id), digest);
                        }
                        continue;
                    }
                }
            }
            "Connection" => {
                let out = connectors::execute(app, uid, &r, &n).await?;
                if n.provider == "local"
                    && n.operation == "test"
                    && out.status == "completed"
                    && out.decision == "pass"
                {
                    let workspace = PathBuf::from(&r.workspace);
                    let proof = crate::artifacts::blocking(move || agents::workspace_fingerprint(&workspace)).await?;
                    r.context.insert(format!("test-proof:{}", n.id), proof);
                }
                crate::artifacts::snapshot_async(app, uid, &r, &n.id, "", &out.report()).await?;
                out
            }
            _ => bail!("Nodo non eseguibile: {}", n.kind),
        };
        if n.kind == "A2A" && result.status == "paused" {
            r.status = "paused".into();
            r.pause_requested = false;
            r.states.insert(n.id.clone(), "paused".into());
            save(app, uid, &mut r)?;
            return Ok(());
        }
        r.outputs.insert(n.id.clone(), result.clone());
        if result.status == "needs_input" {
            let task = Some(n.id.clone());
            return pause_for(app, uid, &mut r, "question", result.questions, task);
        }
        ensure!(
            result.status == "completed",
            "Agente bloccato: {}",
            result.summary
        );
        if n.kind == "Gate" && (outcome == "needs_info" || outcome == "blocked") {
            return pause_for(app, uid, &mut r, "gate", vec![result.summary], None);
        }
        r.states.insert(n.id.clone(), "completed".into());
        route(app, uid, &mut r, &outcome)?;
    }
    bail!("Limite di transizioni del run superato")
}
fn handoff_package(app: &App, uid: i64, r: &Run, source: &str, target: &str) -> Result<String> {
    let output = r
        .outputs
        .get(source)
        .context("Handoff: output sorgente assente")?;
    let arts = app.store.artifacts(uid, &r.id)?;
    let limits = if output.details_truncated {
        "Context handoff; native model-private state is not copied. The source report was truncated at 16,000 characters. Read the full report file if the source lists one. If essential context is missing, ask for clarification before proceeding."
    } else {
        "Context handoff; native model-private state is not copied."
    };
    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "schema":1, "source_task":source, "source_output":output,
        "target_agent":target, "request":r.input,
        "decisions":app.store.decisions(uid,&r.id)?,
        "artifacts":arts.iter().map(|a|serde_json::json!({"id":a.id,"path":a.path,"version":a.version,"sha256":a.hash})).collect::<Vec<_>>(),
        "limits":limits
    }))?)
}

// Deliver the barrier package even when Review/Gate steps intervene before the
// consumer. Memory and subsequent parallel regions establish their own handoffs.
fn join_handoffs(app: &App, uid: i64, r: &mut Run, join: &Node) -> Result<()> {
    let mut queue = vec![join.id.clone()];
    let mut seen = HashSet::new();
    while let Some(id) = queue.pop() {
        if !seen.insert(id.clone()) {
            continue;
        }
        for edge in r
            .workflow
            .edges
            .iter()
            .filter(|e| e.from == id && e.kind == "control")
        {
            let node = r.workflow.node(&edge.to)?;
            match node.kind.as_str() {
                "Task" => {
                    let package = handoff_package(app, uid, r, &join.id, &node.agent)?;
                    r.context.insert(format!("handoff:{}", node.agent), package);
                }
                "Memory" | "Fork" | "Join" => {}
                _ => queue.push(node.id.clone()),
            }
        }
    }
    Ok(())
}

fn parallel_prompt(app: &App, uid: i64, branch: &Run, task: &Node, fork: &Node) -> Result<String> {
    let (rt, _, _, _) = runtime(branch, &task.agent)?;
    let cmd = agents::resolve_command(&task.command, &rt, Path::new(&branch.workspace))?;
    let memory_instructions = memory_source_instructions(&branch.workflow, task);
    let handoff = if let Some(source) = branch
        .last_node
        .as_ref()
        .filter(|id| branch.outputs.contains_key(*id))
    {
        handoff_package(app, uid, branch, source, &task.agent)?
    } else {
        branch
            .context
            .get(&format!("handoff:{}", task.agent))
            .cloned()
            .unwrap_or_default()
    };
    Ok(format!(
        "{}\nStep instructions:\n{}{memory_instructions}\n{}\nExplicit handoff: {}\nClarification for the branches: {}\nParallel task: shared workspace, keep to the files assigned to your branch.",
        cmd,
        task.prompt,
        incoming(app, uid, branch)?,
        handoff,
        branch
            .context
            .get(&format!("answer:{}", fork.id))
            .cloned()
            .unwrap_or_default()
    ))
}

async fn execute_parallel(app: &App, uid: i64, r: &mut Run, fork: &Node) -> Result<()> {
    use futures::{StreamExt, stream::FuturesUnordered};
    let branches = parallel_branches(&r.workflow, fork)?;
    r.states.insert(fork.id.clone(), "running".into());
    loop {
        if app.store.run(uid, &r.id)?.pause_requested {
            r.status = "paused".into();
            r.pause_requested = false;
            return save(app, uid, r);
        }
        let ready: Vec<_> = branches
            .iter()
            .filter_map(|branch| {
                branch
                    .iter()
                    .find(|n| r.states.get(&n.id).is_none_or(|s| !["completed", "bypassed"].contains(&s.as_str())))
                    .cloned()
            })
            .collect();
        if ready.is_empty() {
            break;
        }
        for n in ready.iter().filter(|n| n.skip) { bypass_task(r, n); }
        let ready: Vec<_> = ready.into_iter().filter(|n| !n.skip).collect();
        if ready.is_empty() { save(app, uid, r)?; continue; }
        for n in &ready {
            r.states.insert(n.id.clone(), "running".into());
            *r.attempts.entry(n.id.clone()).or_default() += 1;
        }
        save(app, uid, r)?;
        let mut work = FuturesUnordered::new();
        for n in ready {
            let mut branch = r.clone();
            branch.current = n.id.clone();
            branch.context.insert("parallel-turn".into(), "true".into());
            if let Some(chain) = branches.iter().find(|b| b.iter().any(|t| t.id == n.id))
                && let Some(previous) = chain.iter().take_while(|t| t.id != n.id).last()
            {
                branch.last_node = Some(previous.id.clone());
            }
            work.push(async move {
                let result: Result<AgentResult> = async {
                    let (rt, model, provider, base_url) = runtime(&branch, &n.agent)?;
                    let prompt = parallel_prompt(app, uid, &branch, &n, fork)?;
                    agents::turn(
                        app,
                        uid,
                        &mut branch,
                        agents::Turn {
                            binding: &n.agent,
                            runtime: &rt,
                            model_override: &model,
                        provider_override: &provider,
                        base_url_override: &base_url,
                            prompt: &prompt,
                        },
                    )
                    .await
                }
                .await;
                (n, branch, result)
            });
        }
        let mut questions = Vec::new();
        let mut errors = Vec::new();
        while let Some((n, branch, result)) = work.next().await {
            if let Some(session) = branch.sessions.get(&n.agent) {
                r.sessions.insert(n.agent.clone(), session.clone());
            }
            match result {
                Ok(out) => {
                    if let Err(e) = crate::artifacts::snapshot_async(app, uid, r, &n.id, &n.agent, &out.report()).await {
                        errors.push(format!("{}: {e}", n.title));
                    }
                    let state = if out.status == "completed" {
                        "completed"
                    } else if out.status == "needs_input" {
                        "waiting_user"
                    } else {
                        "failed"
                    };
                    r.states.insert(n.id.clone(), state.into());
                    if state == "waiting_user" {
                        questions
                            .extend(out.questions.iter().map(|q| format!("{}: {}", n.title, q)));
                    }
                    if state == "failed" {
                        errors.push(format!("{}: {}", n.title, out.summary));
                    }
                    r.outputs.insert(n.id.clone(), out);
                }
                Err(e) => {
                    r.states.insert(n.id.clone(), "failed".into());
                    errors.push(format!("{}: {e}", n.title));
                }
            }
            save(app, uid, r)?;
        }
        ensure!(
            errors.is_empty(),
            "Rami paralleli falliti: {}",
            errors.join("; ")
        );
        if !questions.is_empty() {
            return pause_for(app, uid, r, "question", questions, Some(fork.id.clone()));
        }
    }
    let joined = barrier_output(r, &branches)?;
    r.outputs.insert(fork.target.clone(), joined);
    r.states.insert(fork.id.clone(), "completed".into());
    r.last_node = Some(fork.id.clone());
    r.current = fork.target.clone();
    r.active_edge = None;
    save(app, uid, r)
}

/// The Join's result: the completed output of every branch Task. Skipped Tasks contribute nothing.
pub(crate) fn barrier_output(r: &Run, branches: &[Vec<Node>]) -> Result<AgentResult> {
    let outputs: BTreeMap<_, _> = branches
        .iter()
        .flatten()
        .filter(|n| !n.skip)
        .map(|n| {
            let output = r
                .outputs
                .get(&n.id)
                .context("Join: risultato di un ramo assente")?;
            ensure!(
                output.status == "completed",
                "Join: risultato di un ramo incompleto"
            );
            Ok((n.id.clone(), output))
        })
        .collect::<Result<_>>()?;
    Ok(AgentResult {
        status: "completed".into(),
        summary: serde_json::to_string(&outputs)?,
        ..Default::default()
    })
}

fn rework_review(app: &App, uid: i64, r: &mut Run) -> Result<()> {
    rework_review_state(r)?;
    save(app, uid, r)
}
fn rework_review_state(r: &mut Run) -> Result<()> {
    if !r.context.contains_key("review-feedback")
        && let Some(out) = r.outputs.get(&r.current)
    {
        r.context
            .insert("review-feedback".into(), serde_json::to_string(out)?);
    }
    if r.workflow.next(&r.current, "rework").is_some() {
        return route_state(r, "rework");
    }
    let gate = r
        .workflow
        .next(&r.current, "")
        .and_then(|e| r.workflow.node(&e.to).ok())
        .filter(|n| n.kind == "Gate")
        .context("Review rifiutata senza ramo rework configurato")?
        .id
        .clone();
    r.states.insert(r.current.clone(), "rework".into());
    r.current = gate;
    route_state(r, "rework")
}
pub fn decide(app: &App, user: &User, rid: &str, action: &str, answer: &str) -> Result<Run> {
    ensure!(
        !app.busy.lock().unwrap().contains(rid),
        "Attendi la chiusura del turno attivo"
    );
    apply_decision(app, user, rid, action, answer, None)
}
pub(crate) fn apply_decision(
    app: &App,
    user: &User,
    rid: &str,
    action: &str,
    answer: &str,
    supervisor: Option<SupervisorProposal>,
) -> Result<Run> {
    let mut r = app.store.run(user.id, rid)?;
    let p = r.pending.clone().context("Nessuna decisione in attesa")?;
    let commit = r.workflow.node(&p.node).is_ok_and(connectors::confirms_commit);
    ensure!(
        supervisor.is_none() || r.execution.mode == ExecutionMode::Unattended,
        "Decisione automatica non autorizzata"
    );
    ensure!(
        supervisor.is_none()
            || p.kind != "effect"
            || action != "approve"
            || r.execution.allow_external_writes
            || commit,
        "Scritture esterne non autorizzate"
    );
    ensure!(
        ["approve", "reject", "answer"].contains(&action),
        "Azione sconosciuta"
    );
    if p.kind == "question" || p.kind == "gate" {
        ensure!(
            action == "answer" && !answer.trim().is_empty(),
            "Risposta obbligatoria"
        );
    } else {
        ensure!(action != "answer", "Approva o rifiuta questa review");
    }
    let artifacts = app
        .store
        .artifacts(user.id, rid)?
        .iter()
        .map(|a| a.id.clone())
        .collect();
    let receipt = Decision {
        actor: if supervisor.is_some() {
            "ai"
        } else if user.email.starts_with("a2a:") {
            "a2a"
        } else {
            "human"
        }
        .into(),
        id: if supervisor.is_some() {
            format!("ai:{}", p.id)
        } else {
            id()
        },
        supervisor,
        run: rid.into(),
        node: p.node.clone(),
        decision: action.into(),
        answer: answer.into(),
        user: user.email.clone(),
        created: now(),
        artifacts,
    };
    r.pending = None;
    r.status = "paused".into();
    r.error = None;
    match action {
        "reject" => {
            r.context.insert(
                "review-feedback".into(),
                serde_json::to_string(
                    &serde_json::json!({"approval":receipt,"review_output":r.outputs.get(&p.node)}),
                )?,
            );
            if p.kind == "effect" {
                r.status = "failed".into();
                r.error = Some(if commit {
                    "Commit locale rifiutato: nessun commit creato, i file restano nel workspace come li ha lasciati la run"
                } else {
                    "Operazione remota rifiutata; nessuna scrittura eseguita"
                }.into());
            } else if rework_review_state(&mut r).is_err() {
                r.status = "failed".into();
                r.error = Some(
                    "Review rifiutata. Correggi il workflow o avvia una nuova esecuzione.".into(),
                );
            }
        }
        "approve" => {
            if p.kind == "effect" {
                let digest = r
                    .context
                    .remove(&format!("effect-preview:{}", p.node))
                    .context("Anteprima effetto mancante")?;
                r.context.insert(format!("effect:{}", p.node), digest);
            } else {
                r.context.insert(
                    format!("human-approved:{}", p.node),
                    serde_json::to_string(&receipt)?,
                );
            }
        }
        "answer" => {
            let target = if ["Gate", "Review"].contains(&r.workflow.node(&p.node)?.kind.as_str()) {
                p.node.clone()
            } else {
                p.resume_task.unwrap_or_else(|| p.node.clone())
            };
            r.context
                .insert(format!("answer:{target}"), serde_json::to_string(&receipt)?);
            r.last_node = Some(p.node.clone());
            if target != p.node {
                r.active_edge = None;
            }
            r.current = target.clone();
            r.states.insert(target, "queued".into());
        }
        _ => unreachable!(),
    }
    app.store
        .checkpoint_decision(user.id, &r, &receipt, &p.id)?;
    save(app, user.id, &mut r)?;
    emit(
        app,
        user.id,
        rid,
        if receipt.actor == "ai" {
            "ai_decision"
        } else {
            "decision"
        },
        &p.node,
        &format!("{} · {}", action, answer),
    );
    Ok(r)
}
pub fn annotate(app: &App, user: &User, rid: &str, text: &str) -> Result<Run> {
    ensure!(
        !app.busy.lock().unwrap().contains(rid),
        "Metti il run in pausa prima di aggiungere contesto"
    );
    ensure!(
        !text.trim().is_empty() && text.len() <= 12000,
        "Chiarimento obbligatorio (max 12000 caratteri)"
    );
    let mut r = app.store.run(user.id, rid)?;
    ensure!(
        ["paused", "waiting_review", "waiting_user"].contains(&r.status.as_str()),
        "Il run deve essere in pausa o in attesa di una decisione"
    );
    let receipt = Decision {
        actor: "human".into(),
        supervisor: None,
        id: id(),
        run: rid.into(),
        node: r.current.clone(),
        decision: "clarification".into(),
        answer: text.into(),
        user: user.email.clone(),
        created: now(),
        artifacts: app
            .store
            .artifacts(user.id, rid)?
            .into_iter()
            .map(|a| a.id)
            .collect(),
    };
    app.store.add_decision(user.id, &receipt)?;
    r.context
        .insert("user-note".into(), serde_json::to_string(&receipt)?);
    save(app, user.id, &mut r)?;
    Ok(r)
}
pub(crate) fn open_tasks(workspace: &Path, location: &str) -> Result<usize> {
    let workspace = &std::fs::canonicalize(workspace)?;
    let path = if location.is_empty() {
        let specs = workspace.join("specs");
        let matches: Vec<_> = std::fs::read_dir(specs)?
            .filter_map(|e| e.ok())
            .map(|e| e.path().join("tasks.md"))
            .filter(|p| p.is_file())
            .collect();
        ensure!(
            matches.len() == 1,
            "Seleziona il percorso tasks.md nel Gate: feature assente o ambigua"
        );
        matches[0].clone()
    } else {
        workspace.join(location)
    };
    let path = std::fs::canonicalize(path)?;
    ensure!(path.starts_with(workspace), "tasks.md fuori workspace");
    let text = std::fs::read_to_string(path)?;
    let lines: Vec<_> = text.lines().map(str::trim).collect();
    let is_open =
        |s: &str| s.starts_with("- [ ]") || s.starts_with("* [ ]") || s.starts_with("+ [ ]");
    ensure!(
        lines
            .iter()
            .any(|s| is_open(s) || s.starts_with("- [x]") || s.starts_with("- [X]")),
        "tasks.md senza task verificabili"
    );
    Ok(lines.iter().filter(|s| is_open(s)).count())
}

#[cfg(test)]
mod parallel_tests {
    use super::*;
    fn graph() -> Workflow {
        serde_json::from_value(serde_json::json!({
            "id":"parallel", "name":"Parallel", "description":"", "entry":"fork",
            "nodes":[{"id":"fork","kind":"Fork","target":"join"},
                {"id":"a","kind":"Task","agent":"aa","prompt":"A"},
                {"id":"b","kind":"Task","agent":"ab","prompt":"B"},
                {"id":"aa","kind":"Agent"},{"id":"ab","kind":"Agent"},
                {"id":"join","kind":"Join"},{"id":"source","kind":"Source","source_type":"example"}],
            "edges":[{"from":"source","to":"aa","kind":"context"},
                {"from":"aa","to":"a","kind":"context"},
                {"from":"ab","to":"b","kind":"context"},
                {"from":"fork","to":"a","kind":"control"},
                {"from":"fork","to":"b","kind":"control"},
                {"from":"a","to":"join","kind":"control"},
                {"from":"b","to":"join","kind":"control"}]
        }))
        .unwrap()
    }
    #[tokio::test]
    async fn memory_preserves_source_report_truncation_in_run_and_handoff() {
        let dir = tempfile::tempdir().unwrap();
        let app = crate::tests::app(dir.path());
        let mut w = graph();
        w.entry = "a".into();
        w.nodes.retain(|n| !["fork", "join", "b", "ab"].contains(&n.id.as_str()));
        w.nodes.push(serde_json::from_value(serde_json::json!({
            "id":"memory","kind":"Memory","agent":"a","target":"aa"
        })).unwrap());
        w.edges.retain(|e| e.kind == "context" && e.to != "b");
        w.edges.push(Edge {
            from: "a".into(),
            to: "memory".into(),
            kind: "control".into(),
            outcome: String::new(),
        });
        assert!(memory_source_instructions(&w, w.node("a").unwrap()).contains(".rightcommit/handoffs/"));

        let mut run = create(&app, 1, w, "handoff".into(), None).await.unwrap();
        run.current = "memory".into();
        run.states.insert("a".into(), "completed".into());
        run.outputs.insert("a".into(), AgentResult {
            details_truncated: true,
            ..completed("Source report")
        });
        save(&app, 1, &mut run).unwrap();
        drive(&app, 1, &run.id).await.unwrap();

        let saved = app.store.run(1, &run.id).unwrap();
        assert_eq!(saved.states["memory"], "completed");
        assert!(saved.outputs["memory"].details_truncated);
        let handoff: serde_json::Value = serde_json::from_str(&saved.context["handoff:aa"]).unwrap();
        assert_eq!(handoff["source_output"]["details_truncated"], true);
        assert!(handoff["limits"].as_str().unwrap().contains("source report was truncated"));
    }

    #[tokio::test]
    async fn skipped_parallel_tasks_make_no_calls_and_contribute_no_outputs() {
        let dir = tempfile::tempdir().unwrap();
        let app = crate::tests::app(dir.path());
        let mut w = graph();
        for n in w.nodes.iter_mut().filter(|n| n.kind == "Task") {
            assert!(!n.skip); // Older workflows keep their existing behavior.
            n.skip = true;
            n.prompt.clear();
            n.command = "missing-command-must-never-run".into();
        }
        let mut run = create(&app, 1, w, "skip".into(), None).await.unwrap();
        run.outputs.insert("a".into(), completed("stale output"));
        save(&app, 1, &mut run).unwrap();
        drive(&app, 1, &run.id).await.unwrap();
        let saved = app.store.run(1, &run.id).unwrap();
        assert_eq!(saved.status, "completed");
        for id in ["a", "b"] {
            assert_eq!(saved.states[id], "bypassed");
            assert!(!saved.outputs.contains_key(id));
            assert!(!saved.attempts.contains_key(id));
        }
        assert!(saved.sessions.is_empty());
        assert_eq!(saved.outputs["join"].summary, "{}");
        assert!(app.store.artifacts(1, &run.id).unwrap().iter().all(|a| a.node != "a" && a.node != "b"));
    }
    #[tokio::test]
    async fn skipped_serial_task_continues_without_a_result_or_attempt() {
        let dir = tempfile::tempdir().unwrap();
        let app = crate::tests::app(dir.path());
        let mut w = graph();
        w.entry = "a".into();
        w.nodes.retain(|n| !["fork", "join", "b", "ab"].contains(&n.id.as_str()));
        let task = w.nodes.iter_mut().find(|n| n.id == "a").unwrap();
        task.prompt = "   ".into();
        w.nodes.iter_mut().find(|n| n.id == "a").unwrap().skip = true;
        w.nodes.push(serde_json::from_value(serde_json::json!({"id":"review","kind":"Review","mode":"human"})).unwrap());
        w.edges.retain(|e| e.kind == "context" && e.from != "ab");
        w.edges.push(Edge {from:"a".into(),to:"review".into(),kind:"control".into(),outcome:String::new()});
        // A normal Task still needs a prompt; bypassed Tasks do not.
        w.nodes.iter_mut().find(|n| n.id == "a").unwrap().skip = false;
        assert!(validate(&w).unwrap_err().to_string().contains("scrivi il prompt"));
        w.nodes.iter_mut().find(|n| n.id == "a").unwrap().skip = true;
        validate(&w).unwrap();
        let mut dependent = w.clone();
        dependent.nodes.push(serde_json::from_value(serde_json::json!({"id":"memory","kind":"Memory","agent":"a","target":"aa"})).unwrap());
        dependent.edges.iter_mut().find(|e|e.from=="a" && e.kind=="control").unwrap().to = "memory".into();
        dependent.edges.push(Edge {from:"memory".into(),to:"review".into(),kind:"control".into(),outcome:String::new()});
        assert!(validate(&dependent).unwrap_err().to_string().contains("disattiva Salta task"));
        let mut r = create(&app, 1, w, "skip".into(), None).await.unwrap();
        r.outputs.insert("a".into(), completed("stale output"));
        save(&app, 1, &mut r).unwrap();
        drive(&app, 1, &r.id).await.unwrap();
        let saved = app.store.run(1, &r.id).unwrap();
        assert_eq!(saved.status, "waiting_review");
        assert_eq!(saved.pending.unwrap().node, "review");
        assert_eq!(saved.states["a"], "bypassed");
        assert!(!saved.outputs.contains_key("a"));
        assert!(!saved.attempts.contains_key("a"));
        assert!(saved.sessions.is_empty());
    }
    fn coordinator_graph() -> Workflow {
        let mut w = graph();
        w.entry = "plan".into();
        for value in [
            serde_json::json!({"id":"coordinator","kind":"Agent"}),
            serde_json::json!({"id":"plan","kind":"Task","agent":"coordinator","prompt":"Analyze brief"}),
            serde_json::json!({"id":"synthesis","kind":"Task","agent":"coordinator","prompt":"Editorial synthesis"}),
        ] {
            w.nodes.push(serde_json::from_value(value).unwrap());
        }
        for (from, to, kind) in [
            ("coordinator", "plan", "context"),
            ("coordinator", "synthesis", "context"),
            ("plan", "fork", "control"),
            ("join", "synthesis", "control"),
        ] {
            w.edges.push(Edge {
                from: from.into(),
                to: to.into(),
                kind: kind.into(),
                outcome: String::new(),
            });
        }
        w
    }
    fn completed(summary: &str) -> AgentResult {
        AgentResult {
            status: "completed".into(),
            summary: summary.into(),
            ..Default::default()
        }
    }
    #[tokio::test]
    async fn a_step_answer_without_structured_result_gets_one_follow_up_without_a_new_attempt() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let app = crate::tests::app(dir.path());
        app.store.update_settings(1, |s| {
            s.agents.insert("codex".into(), AgentConfig { api_key: "test-codex".into(), ..Default::default() });
            Ok(())
        }).unwrap();
        let tools = crate::distribution::tools(dir.path());
        std::fs::create_dir_all(&tools).unwrap();
        // Answers in plain text; the follow-up returns the block unless the step asks for "twice".
        std::fs::write(tools.join("codex"), r#"#!/bin/sh
prompt=$(cat)
printf '%s\n' "$*" >> "$HOME/codex-calls.log"
printf '%s\n' '{"type":"thread.started","thread_id":"step-session"}'
case "$prompt" in
  *"ended without the required <nodo_result> block"*)
    if [ -f "$HOME/twice" ]; then printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"Still plain."}}'; exit 0; fi
    printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"<nodo_result>{\"status\":\"completed\",\"summary\":\"Plan: A then B\",\"decision\":\"pass\"}</nodo_result>"}}' ;;
  *CASE-TWICE*) touch "$HOME/twice"; printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"FULL PLAN"}}' ;;
  *) printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"FULL PLAN: step A, then step B"}}' ;;
esac
"#).unwrap();
        std::fs::set_permissions(tools.join("codex"), std::fs::Permissions::from_mode(0o700)).unwrap();
        let home = crate::agents::user_home(&app, 1).unwrap();
        for (kind, prompt) in [("Task", "Plan the work"), ("Gate", "Decide"), ("Task", "CASE-TWICE plan")] {
            let _ = std::fs::remove_file(home.join("codex-calls.log"));
            let mut w: Workflow = serde_json::from_value(serde_json::json!({
                "id":"follow-up", "name":"Follow-up", "description":"", "entry":"step",
                "nodes":[{"id":"source","kind":"Source","source_type":"example"},
                    {"id":"author","kind":"Agent","runtime":"codex"},
                    {"id":"step","kind":kind,"agent":"author","prompt":prompt,"outcomes":["pass"]},
                    {"id":"done","kind":"Task","agent":"author","skip":true}],
                "edges":[{"from":"source","to":"author","kind":"context"},
                    {"from":"author","to":"step","kind":"context"},{"from":"step","to":"done","kind":"control","outcome":"pass"}]
            })).unwrap();
            if kind == "Task" { w.edges.last_mut().unwrap().outcome.clear(); }
            let r = create(&app, 1, w, "Request".into(), None).await.unwrap();
            let result = drive(&app, 1, &r.id).await;
            let saved = app.store.run(1, &r.id).unwrap();
            let events = app.store.events(1, &r.id, 0).unwrap();
            let calls = std::fs::read_to_string(home.join("codex-calls.log")).unwrap();
            // Two turns, both recorded, the second resuming the same native session, one attempt.
            assert_eq!(events.iter().filter(|e| e.kind == "agent_turn").count(), 2, "{prompt}");
            assert_eq!(calls.lines().count(), 2, "{prompt}");
            assert!(!calls.lines().next().unwrap().contains("resume") && calls.lines().nth(1).unwrap().contains("resume step-session"));
            assert_eq!(saved.attempts["step"], 1);
            if prompt.starts_with("CASE-TWICE") {
                assert_eq!(result.unwrap_err().to_string(), "Risposta agente priva del risultato strutturato rightcommit. Riprendi nella stessa sessione con Retry.");
                assert!(!saved.outputs.contains_key("step"));
                continue;
            }
            result.unwrap();
            assert_eq!(saved.status, "completed", "{prompt}");
            let out = &saved.outputs["step"];
            assert_eq!(out.summary, "Plan: A then B");
            assert_eq!(out.details, "FULL PLAN: step A, then step B", "the text already written is kept");
            assert_eq!(saved.sessions[if kind == "Gate" { "gate:step" } else { "author" }].turns, 1);
        }
    }
    #[test]
    fn rework_opens_a_correction_branch_that_changes_agent_only_through_memory() {
        let graph = |fixer: &str, memory: bool| -> Workflow {
            let mut w: Workflow = serde_json::from_value(serde_json::json!({
                "id":"w","name":"Correction branch","description":"","entry":"write",
                "nodes":[
                    {"id":"author","kind":"Agent","runtime":"pi"},
                    {"id":"fixer","kind":"Agent","runtime":"claude"},
                    {"id":"write","kind":"Task","agent":"author","prompt":"Write the post"},
                    {"id":"review","kind":"Review","agent":"author","mode":"human","prompt":"Approve the post"},
                    {"id":"fix","kind":"Task","agent":fixer,"prompt":"Correct the post"}
                ],
                "edges":[
                    {"from":"write","to":"review","kind":"control","outcome":""},
                    {"from":"fix","to":"review","kind":"control","outcome":""}
                ]
            })).unwrap();
            let rework = |to: &str| Edge {from: "review".into(), to: to.into(), kind: "rework".into(), outcome: "rework".into()};
            if memory {
                w.nodes.push(serde_json::from_value(serde_json::json!({"id":"handoff","kind":"Memory","agent":"write","target":fixer})).unwrap());
                w.edges.push(rework("handoff"));
                w.edges.push(Edge {from: "handoff".into(), to: "fix".into(), kind: "control".into(), outcome: String::new()});
            } else {
                w.edges.push(rework("fix"));
            }
            if fixer == "author" {
                w.nodes.retain(|n| n.id != "fixer");
            }
            w
        };
        // The branch is reached only through the rework edge and still counts as reachable.
        validate(&graph("author", false)).unwrap();
        validate(&graph("fixer", true)).unwrap();
        // Another Agent takes over only through a Memory, as on the main path.
        assert!(validate(&graph("fixer", false)).unwrap_err().to_string().contains("cambio Agent senza Memory"));
        // A Memory left unconfigured is named, with the field to fill in.
        let mut unset = graph("fixer", true);
        unset.nodes.iter_mut().find(|n| n.id == "handoff").unwrap().target.clear();
        assert_eq!(validate(&unset).unwrap_err().to_string(), "Memory handoff non è configurato: scegli l’Agent destinatario, cioè l’Agent del Task che riceve il risultato");
        unset.nodes.iter_mut().find(|n| n.id == "handoff").unwrap().agent.clear();
        assert_eq!(validate(&unset).unwrap_err().to_string(), "Memory handoff non è configurato: scegli il Task sorgente, cioè il Task di cui passare il risultato");
        // A step that no edge reaches is still rejected, and so is a cycle inside the branch.
        let mut orphan = graph("fixer", true);
        orphan.edges.retain(|e| e.kind != "rework");
        assert!(validate(&orphan).unwrap_err().to_string().contains("non raggiungibile"));
        let mut cyclic = graph("author", false);
        cyclic.nodes.push(serde_json::from_value(serde_json::json!({"id":"again","kind":"Task","agent":"author","prompt":"Again"})).unwrap());
        for (from, to) in [("fix", "again"), ("again", "fix")] {
            cyclic.edges.push(Edge {from: from.into(), to: to.into(), kind: "control".into(), outcome: String::new()});
        }
        cyclic.edges.retain(|e| !(e.from == "fix" && e.to == "review"));
        assert!(validate(&cyclic).unwrap_err().to_string().contains("Ciclo nel percorso"));
        // The source Task is told to prepare the handoff, and the finding reaches the Task after the Memory.
        let w = graph("fixer", true);
        assert!(memory_source_instructions(&w, w.node("write").unwrap()).contains("Memory"));
        let mut run = crate::manager::runtime_run("run", &w, std::env::temp_dir());
        run.current = "review".into();
        run.context.insert("review-feedback".into(), "Tone is too formal".into());
        route_state(&mut run, "rework").unwrap();
        assert_eq!(run.current, "handoff");
        assert_eq!(run.context["feedback:fix"], "Tone is too formal");
        assert!(!run.context.contains_key("feedback:handoff"));
        assert_eq!(run.states["fix"], "queued");
    }
    #[test]
    fn accepts_coordinator_fanout_and_synthesis_but_rejects_sequential_binding_changes() {
        validate(&coordinator_graph()).unwrap();
        let mut w = coordinator_graph();
        w.nodes.push(serde_json::from_value(serde_json::json!({"id":"a2","kind":"Task","agent":"coordinator","prompt":"Wrong binding"})).unwrap());
        w.edges
            .iter_mut()
            .find(|e| e.from == "a" && e.to == "join")
            .unwrap()
            .to = "a2".into();
        w.edges.push(Edge {
            from: "a2".into(),
            to: "join".into(),
            kind: "control".into(),
            outcome: String::new(),
        });
        assert!(validate(&w).unwrap_err().to_string().contains("Memory"));
        let mut w = coordinator_graph();
        w.nodes
            .iter_mut()
            .find(|n| n.id == "synthesis")
            .unwrap()
            .agent = "aa".into();
        w.edges
            .iter_mut()
            .find(|e| e.to == "synthesis" && e.kind == "context")
            .unwrap()
            .from = "aa".into();
        // A new consumer after the Join is supported as well as the original Coordinator.
        validate(&w).unwrap();
    }
    #[tokio::test]
    async fn coordinator_plan_reaches_both_branches_and_join_hands_all_results_back() {
        let dir = tempfile::tempdir().unwrap();
        let app = crate::tests::app(dir.path());
        let w = coordinator_graph();
        let mut r = create(&app, 1, w.clone(), "Research request".into(), None)
            .await
            .unwrap();
        r.outputs
            .insert("plan".into(), completed("Coordinator research plan"));
        r.last_node = Some("plan".into());
        r.current = "fork".into();
        let fork = w.node("fork").unwrap();
        for id in ["a", "b"] {
            let task = w.node(id).unwrap();
            let prompt = parallel_prompt(&app, 1, &r, task, fork).unwrap();
            assert!(prompt.contains("Coordinator research plan"));
            assert!(prompt.contains("Research request"));
            assert!(prompt.contains(&task.agent));
            assert!(prompt.contains("native model-private state is not copied"));
            assert!(prompt.contains("Step instructions:\n") && prompt.contains("Parallel task: shared workspace, keep to the files assigned to your branch."));
        }
        r.states.insert("a".into(), "completed".into());
        r.outputs
            .insert("a".into(), completed("Research A findings"));
        // Resuming the other branch retains the Coordinator's plan, not its sibling's result.
        let prompt = parallel_prompt(&app, 1, &r, w.node("b").unwrap(), fork).unwrap();
        assert!(prompt.contains("Coordinator research plan"));
        assert!(!prompt.contains("Research A findings"));
        r.states.insert("b".into(), "completed".into());
        r.outputs
            .insert("b".into(), completed("Research B findings"));
        execute_parallel(&app, 1, &mut r, fork).await.unwrap();
        join_handoffs(&app, 1, &mut r, w.node("join").unwrap()).unwrap();
        let package: serde_json::Value =
            serde_json::from_str(&r.context["handoff:coordinator"]).unwrap();
        assert_eq!(package["source_task"], "join");
        assert_eq!(package["target_agent"], "coordinator");
        let results: serde_json::Value =
            serde_json::from_str(package["source_output"]["summary"].as_str().unwrap()).unwrap();
        assert_eq!(results["a"]["summary"], "Research A findings");
        assert_eq!(results["b"]["summary"], "Research B findings");
        assert!(r.attempts.is_empty());
        // The handoff is persisted with the same state used for pause/restart.
        save(&app, 1, &mut r).unwrap();
        assert_eq!(
            app.store.run(1, &r.id).unwrap().context["handoff:coordinator"],
            r.context["handoff:coordinator"]
        );
        r.outputs.remove("b");
        assert!(
            execute_parallel(&app, 1, &mut r, fork)
                .await
                .unwrap_err()
                .to_string()
                .contains("assente")
        );
        r.outputs.insert(
            "b".into(),
            AgentResult {
                status: "failed".into(),
                ..Default::default()
            },
        );
        assert!(
            execute_parallel(&app, 1, &mut r, fork)
                .await
                .unwrap_err()
                .to_string()
                .contains("incompleto")
        );
    }
    #[tokio::test]
    async fn drive_persists_join_handoff_before_human_review() {
        let dir = tempfile::tempdir().unwrap();
        let app = crate::tests::app(dir.path());
        let mut w = coordinator_graph();
        w.nodes.push(
            serde_json::from_value(serde_json::json!({
                "id":"review", "kind":"Review", "mode":"human", "prompt":"Review research"
            }))
            .unwrap(),
        );
        w.edges.iter_mut().find(|e| e.from == "join").unwrap().to = "review".into();
        w.edges.push(Edge {
            from: "review".into(),
            to: "synthesis".into(),
            kind: "control".into(),
            outcome: String::new(),
        });
        let mut r = create(&app, 1, w.clone(), "Research".into(), None)
            .await
            .unwrap();
        r.current = "fork".into();
        r.last_node = Some("plan".into());
        r.outputs.insert("plan".into(), completed("Plan"));
        for id in ["a", "b"] {
            r.states.insert(id.into(), "completed".into());
            r.outputs.insert(id.into(), completed(id));
        }
        save(&app, 1, &mut r).unwrap();
        drive(&app, 1, &r.id).await.unwrap();
        let saved = app.store.run(1, &r.id).unwrap();
        assert_eq!(saved.status, "waiting_review");
        assert_eq!(saved.pending.unwrap().node, "review");
        assert_eq!(saved.states["join"], "completed");
        assert!(!saved.attempts.contains_key("a"));
        assert!(!saved.attempts.contains_key("b"));
        let package: serde_json::Value =
            serde_json::from_str(&saved.context["handoff:coordinator"]).unwrap();
        assert_eq!(package["source_task"], "join");
        assert_eq!(package["target_agent"], "coordinator");
        let results: serde_json::Value =
            serde_json::from_str(package["source_output"]["summary"].as_str().unwrap()).unwrap();
        assert_eq!(results["a"]["summary"], "a");
        assert_eq!(results["b"]["summary"], "b");
    }
    #[tokio::test]
    async fn step_wrappers_are_english_and_ask_for_the_language_of_the_step() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let app = crate::tests::app(dir.path());
        app.store.update_settings(1, |s| {
            s.agents.insert("codex".into(), AgentConfig { api_key: "test-codex".into(), ..Default::default() });
            Ok(())
        }).unwrap();
        let tools = crate::distribution::tools(dir.path());
        std::fs::create_dir_all(&tools).unwrap();
        // Keeps every prompt it receives and passes every step.
        std::fs::write(tools.join("codex"), r#"#!/bin/sh
cat > "$HOME/prompt-$(date +%s%N)-$$.txt"
printf '%s\n' '{"type":"thread.started","thread_id":"language-session"}'
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"<nodo_result>{\"status\":\"completed\",\"summary\":\"Done\",\"decision\":\"pass\"}</nodo_result>"}}'
"#).unwrap();
        std::fs::set_permissions(tools.join("codex"), std::fs::Permissions::from_mode(0o700)).unwrap();
        let w: Workflow = serde_json::from_value(serde_json::json!({
            "id":"language", "name":"Language", "description":"", "entry":"write",
            "nodes":[{"id":"source","kind":"Source","source_type":"example"},
                {"id":"author","kind":"Agent","runtime":"codex"},{"id":"checker","kind":"Agent","runtime":"codex"},
                {"id":"write","kind":"Task","agent":"author","prompt":"Write a one-line plan in English."},
                {"id":"review","kind":"Review","mode":"agent","agent":"author","prompt":"Check the plan."},
                {"id":"gate","kind":"Gate","agent":"author","prompt":"Is the plan complete?","outcomes":["pass"]},
                {"id":"memory","kind":"Memory","agent":"write","target":"checker"},
                {"id":"check","kind":"Task","agent":"checker","prompt":"Confirm the plan."}],
            "edges":[{"from":"source","to":"author","kind":"context"},{"from":"source","to":"checker","kind":"context"},
                {"from":"write","to":"review","kind":"control"},{"from":"review","to":"gate","kind":"control"},
                {"from":"gate","to":"memory","kind":"control","outcome":"pass"},{"from":"memory","to":"check","kind":"control"}]
        })).unwrap();
        let r = create(&app, 1, w, "Plan the release".into(), None).await.unwrap();
        drive(&app, 1, &r.id).await.unwrap();
        assert_eq!(app.store.run(1, &r.id).unwrap().status, "completed");
        let home = crate::agents::user_home(&app, 1).unwrap();
        let prompts: Vec<String> = std::fs::read_dir(&home).unwrap().filter_map(|e| e.ok()).map(|e| e.path())
            .filter(|p| p.file_name().unwrap().to_string_lossy().starts_with("prompt-"))
            .map(|p| std::fs::read_to_string(p).unwrap()).collect();
        assert_eq!(prompts.len(), 4, "Task, Review, Gate and the Task after Memory");
        for prompt in &prompts {
            assert!(prompt.contains(crate::agents::LANGUAGE), "every step asks for the language of its own instructions");
            assert!(prompt.contains("Step instructions:\n") && prompt.contains("Original run request: Plan the release"));
            for italian in ["GATE decisionale", "Strumenti completi", "Output precedente", "Richiesta originale", "Handoff esplicito", "Esiti ammessi", "Puoi modificare", "Passaggio di consegne"] {
                assert!(!prompt.contains(italian), "{italian}");
            }
        }
        assert!(prompts.iter().any(|p| p.contains("Decision GATE with full tools") && p.contains("Allowed outcomes: pass")));
        assert!(prompts.iter().any(|p| p.contains("REVIEW. Step instructions:\nCheck the plan.")));
        assert!(prompts.iter().any(|p| p.contains("Handoff through Memory") && p.contains("Write a one-line plan in English.")));
    }
    #[tokio::test]
    async fn server_written_results_reach_the_next_step_in_english() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let app = crate::tests::app(dir.path());
        app.store.update_settings(1, |s| {
            s.agents.insert("codex".into(), AgentConfig { api_key: "test-codex".into(), ..Default::default() });
            Ok(())
        }).unwrap();
        let w: Workflow = serde_json::from_value(serde_json::json!({
            "id":"server-results", "name":"Server results", "description":"", "entry":"write",
            "nodes":[{"id":"source","kind":"Source","source_type":"example"},{"id":"author","kind":"Agent","runtime":"codex"},
                {"id":"write","kind":"Task","agent":"author","prompt":"Write the plan."},
                {"id":"review","kind":"Review","mode":"human","prompt":"Approve the plan."},
                {"id":"check","kind":"Task","agent":"author","prompt":"Check the plan."},
                {"id":"gate","kind":"Gate","agent":"author","prompt":"Is it done?","outcomes":["pass","rework"],"max_rework":2,
                    "require_tasks_closed":true,"location":"tasks.md","require_tests":true},
                {"id":"done","kind":"Task","agent":"author","prompt":"Report."}],
            "edges":[{"from":"source","to":"author","kind":"context"},{"from":"write","to":"review","kind":"control"},
                {"from":"review","to":"check","kind":"control"},{"from":"check","to":"gate","kind":"control"},
                {"from":"gate","to":"done","kind":"control","outcome":"pass"},{"from":"gate","to":"check","kind":"rework","outcome":"rework"}]
        })).unwrap();
        let r = create(&app, 1, w, "Plan the release".into(), None).await.unwrap();
        let tasks = Path::new(&r.workspace).join("tasks.md");
        std::fs::write(&tasks, "- [ ] a\n").unwrap();
        let tools = crate::distribution::tools(dir.path());
        std::fs::create_dir_all(&tools).unwrap();
        // Keeps every prompt it receives and closes the open task once the Gate reports it.
        std::fs::write(tools.join("codex"), format!(r#"#!/bin/sh
prompt=$(cat)
printf '%s' "$prompt" > "$HOME/prompt-$(date +%s%N)-$$.txt"
case "$prompt" in *"Convergence not reached"*) printf '%s\n' '- [x] a' > '{}';; esac
printf '%s\n' '{{"type":"thread.started","thread_id":"results-session"}}'
printf '%s\n' '{{"type":"item.completed","item":{{"type":"agent_message","text":"<nodo_result>{{\"status\":\"completed\",\"summary\":\"Done\",\"decision\":\"pass\"}}</nodo_result>"}}}}'
"#, tasks.display())).unwrap();
        std::fs::set_permissions(tools.join("codex"), std::fs::Permissions::from_mode(0o700)).unwrap();
        drive(&app, 1, &r.id).await.unwrap();
        assert_eq!(app.store.run(1, &r.id).unwrap().pending.unwrap().node, "review");
        decide(&app, &User { id: 1, email: "reviewer@test.local".into() }, &r.id, "approve", "Looks right").unwrap();
        // Review, Gate convergence check, Gate test check, then the rework limit stops the run.
        assert!(drive(&app, 1, &r.id).await.unwrap_err().to_string().contains("Limite di rework (2)"));
        let home = crate::agents::user_home(&app, 1).unwrap();
        let mut prompts: Vec<_> = std::fs::read_dir(&home).unwrap().filter_map(|e| e.ok()).map(|e| e.path())
            .filter(|p| p.file_name().unwrap().to_string_lossy().starts_with("prompt-")).collect();
        prompts.sort();
        let prompts: Vec<String> = prompts.iter().map(|p| std::fs::read_to_string(p).unwrap()).collect();
        assert_eq!(prompts.len(), 4, "write, then check after the Review and after each Gate check");
        assert!(prompts[1].contains(r#""summary":"Review approved\nApproval recorded by the server: {"#), "{}", prompts[1]);
        assert!(prompts[2].contains("Convergence not reached: tasks still open: 1; previous report rework=false. Deterministic check, no approval simulated."), "{}", prompts[2]);
        assert!(prompts[3].contains("The server's tests are missing or no longer valid for the current files. Keep the fixes and go back through implementation, CI and review before delivery."), "{}", prompts[3]);
        for prompt in &prompts {
            for italian in ["Review approvata", "Approvazione registrata", "Convergenza", "I test del server"] {
                assert!(!prompt.contains(italian), "{italian}");
            }
        }
        let reports = app.store.artifacts(1, &r.id).unwrap();
        let gate = reports.iter().filter(|a| a.path == ".rightcommit/reports/gate.md").max_by_key(|a| a.version).unwrap();
        assert!(gate.content.starts_with("The server's tests are missing"), "{}", gate.content);
    }
    #[test]
    fn accepts_structured_parallel_tasks() {
        validate(&graph()).unwrap();
    }
    #[test]
    fn rejects_shared_sessions_and_unjoined_paths() {
        let mut w = graph();
        w.nodes[2].agent = "aa".into();
        assert!(
            validate_parallel(&w)
                .unwrap_err()
                .to_string()
                .contains("Agent diverso")
        );
        let mut w = graph();
        w.edges.pop();
        assert!(validate(&w).is_err());
        let mut w = graph();
        w.nodes[0].target = "a".into();
        assert!(validate(&w).is_err());
    }
    #[tokio::test]
    async fn barrier_collects_results_without_repeating_completed_tasks() {
        let dir = tempfile::tempdir().unwrap();
        let app = crate::tests::app(dir.path());
        let w = graph();
        let mut r = create(&app, 1, w.clone(), String::new(), None)
            .await
            .unwrap();
        for id in ["a", "b"] {
            r.states.insert(id.into(), "completed".into());
            r.outputs.insert(
                id.into(),
                AgentResult {
                    status: "completed".into(),
                    summary: id.into(),
                    ..Default::default()
                },
            );
        }
        execute_parallel(&app, 1, &mut r, &w.nodes[0])
            .await
            .unwrap();
        assert_eq!(r.current, "join");
        assert!(r.attempts.is_empty());
        assert!(r.outputs["join"].summary.contains("a"));
        assert!(r.outputs["join"].summary.contains("b"));
    }
}

#[cfg(test)]
mod servicenow_approval_tests {
    use super::*;
    #[tokio::test]
    async fn exact_payload_is_reviewed_again_if_plan_changes() {
        let dir=tempfile::tempdir().unwrap();let app=crate::tests::app(dir.path());
        let w:Workflow=serde_json::from_value(serde_json::json!({
            "id":"native-sn-review","name":"ServiceNow review","description":"test","entry":"write",
            "nodes":[{"id":"source","kind":"Source","source_type":"example"},
                {"id":"write","kind":"Connection","provider":"servicenow","operation":"apply_ticket_plan","location":"plan.json"}],
            "edges":[{"from":"source","to":"write","kind":"context","outcome":""}]
        })).unwrap();
        let mut settings=app.store.settings(1).unwrap();settings.connections.insert("servicenow".into(),ConnectionConfig{
            base_url:"https://dev12345.service-now.com".into(),authentication:"basic".into(),username:"admin".into(),password:"secret-example".into(),..Default::default()});app.store.save_settings(1,&settings).unwrap();
        let r=create(&app,1,w,"demo".into(),None).await.unwrap();
        let path=Path::new(&r.workspace).join("plan.json");
        let mut p=serde_json::json!({"category":"Incident","action":"create_incident","request_id":"demo-1","short_description":"VPN failure","description":"Original description","answer":"KB-001"});
        std::fs::write(&path,p.to_string()).unwrap();
        drive(&app,1,&r.id).await.unwrap();let pending=app.store.run(1,&r.id).unwrap().pending.unwrap();
        assert_eq!(pending.kind,"effect");assert!(pending.questions[0].contains("Original description"));assert!(!pending.questions[0].contains("secret-example"));
        // The shape web/app.js reads the ServiceNow summary from, in English as the supervisor receives it.
        assert!(pending.questions[0].starts_with("Authorize apply_ticket_plan on servicenow? {")&&pending.questions[0].ends_with("} Check the target and the artifacts in the settings and in the run."),"{}",pending.questions[0]);
        decide(&app,&User{id:1,email:"test@local".into()},&r.id,"approve","").unwrap();
        p["description"]=serde_json::json!("Changed after approval");std::fs::write(path,p.to_string()).unwrap();
        drive(&app,1,&r.id).await.unwrap();let pending=app.store.run(1,&r.id).unwrap().pending.unwrap();
        assert_eq!(pending.kind,"effect");assert!(pending.questions[0].contains("Changed after approval"));
    }
}
