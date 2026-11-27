use crate::{
    App,
    auth::{ApiResult, bad},
    model::{Node, User, Workflow},
};
use axum::{
    Json,
    body::Bytes,
    extract::{Query, State},
    http::{StatusCode, header},
    response::IntoResponse,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::{Component, Path, PathBuf};
#[derive(Deserialize)]
pub struct Location {
    #[serde(default)]
    path: String,
    #[serde(default)]
    directory: bool,
    #[serde(default = "default_root")]
    root: String,
}
fn default_root() -> String {
    "sources".into()
}
fn root(app: &App, name: &str) -> anyhow::Result<PathBuf> {
    let path = match name {
        "sources" => app.sources.clone(),
        "examples" => app.assets.join("examples"),
        _ => anyhow::bail!("Archivio non valido"),
    };
    Ok(std::fs::canonicalize(path)?)
}
pub(crate) fn source_folder(app: &App, name: &str, location: &str) -> anyhow::Result<PathBuf> {
    let base = root(app, name)?;
    let folder = resolve(&base, if location == "." { "" } else { location })?;
    anyhow::ensure!(folder.is_dir(), "Seleziona una cartella");
    Ok(folder)
}
/// What the folder of a Source of type "Folder in /sources" is on disk.
enum FolderState {
    Ready(PathBuf),
    Missing,
    /// The relative path of the first component that is not a folder.
    NotFolder(String),
}
fn folder_state(base: &Path, location: &str) -> anyhow::Result<FolderState> {
    let rel = relative(if location == "." { "" } else { location })?;
    let mut cursor = base.to_path_buf();
    let mut shown = PathBuf::new();
    for c in rel.components() {
        cursor.push(c);
        shown.push(c);
        match std::fs::symlink_metadata(&cursor) {
            Ok(m) if m.file_type().is_symlink() => anyhow::bail!("Link simbolici non supportati"),
            Ok(m) if m.is_dir() => {}
            Ok(_) => return Ok(FolderState::NotFolder(shown.to_string_lossy().into_owned())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(FolderState::Missing),
            Err(e) => return Err(e.into()),
        }
    }
    Ok(FolderState::Ready(cursor))
}
/// A Source whose folder does not exist in /sources: node ID, the title errors show and the folder.
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct MissingFolder {
    pub node: String,
    pub title: String,
    pub folder: String,
}
/// Run start refused because Source folders do not exist. The API answers with the code
/// source_folders_missing and the list, so the interface can offer to create them.
#[derive(Debug)]
pub(crate) struct MissingSourceFolders(pub Vec<MissingFolder>);
impl std::fmt::Display for MissingSourceFolders {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0.as_slice() {
            [one] => write!(f, "Source {}: la cartella {} non esiste in /sources. Creala o scegli un’altra cartella", one.title, one.folder),
            many => write!(
                f,
                "Le cartelle di questi Source non esistono in /sources: {}. Creale o scegli altre cartelle",
                many.iter().map(|m| format!("{} ({})", m.title, m.folder)).collect::<Vec<_>>().join(", ")
            ),
        }
    }
}
impl std::error::Error for MissingSourceFolders {}
fn node_label(n: &Node) -> &str {
    if n.title.trim().is_empty() { &n.id } else { &n.title }
}
fn missing(n: &Node) -> MissingFolder {
    MissingFolder { node: n.id.clone(), title: node_label(n).into(), folder: n.location.clone() }
}
fn not_folder(n: &Node, shown: &str) -> anyhow::Error {
    anyhow::anyhow!("Source {}: {} in /sources è un file, non una cartella. Scegli una cartella", node_label(n), shown)
}
/// The folder of a Source in /sources, with errors that name the Source and its folder.
pub(crate) fn node_source_folder(app: &App, n: &Node) -> anyhow::Result<PathBuf> {
    match folder_state(&root(app, "sources")?, &n.location)? {
        FolderState::Ready(path) => Ok(path),
        FolderState::Missing => Err(MissingSourceFolders(vec![missing(n)]).into()),
        FolderState::NotFolder(shown) => Err(not_folder(n, &shown)),
    }
}
/// Checks the folders of the Sources of type "Folder in /sources" a new run reads (the first
/// Source is skipped when the run starts from an earlier run). Missing folders refuse the start
/// with MissingSourceFolders; with `create`, given only when the person confirmed it, they are
/// created inside /sources first. Returns the folders created.
pub(crate) fn check_source_folders(app: &App, w: &Workflow, base_run: bool, create: bool) -> anyhow::Result<Vec<PathBuf>> {
    let checked: Vec<_> = w.nodes.iter().filter(|n| n.kind == "Source").enumerate()
        .filter(|(i, n)| n.source_type == "folder" && !(*i == 0 && base_run)).map(|(_, n)| n).collect();
    if checked.is_empty() {
        return Ok(vec![]);
    }
    let base = root(app, "sources")?;
    let mut absent = vec![];
    for n in checked {
        match folder_state(&base, &n.location)? {
            FolderState::Ready(_) => {}
            FolderState::Missing => absent.push(n),
            FolderState::NotFolder(shown) => return Err(not_folder(n, &shown)),
        }
    }
    if absent.is_empty() {
        return Ok(vec![]);
    }
    if !create {
        return Err(MissingSourceFolders(absent.iter().map(|n| missing(n)).collect()).into());
    }
    let mut created = vec![];
    for n in absent {
        create_folder(&base, n, &mut created)?;
        node_source_folder(app, n)?;
    }
    Ok(created)
}
/// Creates the missing tail of a Source folder inside /sources one folder at a time, with the
/// server's default permissions, like a folder made with New folder in Storage: existing
/// components must be real folders, never links or files.
fn create_folder(base: &Path, n: &Node, created: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    let mut cursor = base.to_path_buf();
    let mut shown = PathBuf::new();
    for c in relative(&n.location)?.components() {
        cursor.push(c);
        shown.push(c);
        match std::fs::create_dir(&cursor) {
            Ok(()) => created.push(cursor.clone()),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
        let m = std::fs::symlink_metadata(&cursor)?;
        anyhow::ensure!(!m.file_type().is_symlink(), "Link simbolici non supportati");
        if !m.is_dir() {
            return Err(not_folder(n, &shown.to_string_lossy()));
        }
    }
    Ok(())
}
pub async fn source_size(
    State(app): State<App>,
    user: User,
    Query(q): Query<Location>,
) -> ApiResult<Json<Value>> {
    if q.root != "sources" {
        return Err(bad("Dimensione copia disponibile solo per sources"));
    }
    let threshold = app.store.settings(user.id)?.source_copy_limit_bytes()?;
    // Nothing to copy from a folder that does not exist yet, or from a file in its place: the run
    // start refuses them with an error naming the Source, and offers to create a missing folder.
    let folder = match folder_state(&root(&app, "sources")?, &q.path)? {
        FolderState::Ready(folder) => folder,
        FolderState::Missing => {
            return Ok(Json(json!({"bytes":0,"threshold_bytes":threshold,"copy_error":null,"missing":true})));
        }
        FolderState::NotFolder(shown) => {
            return Ok(Json(json!({"bytes":0,"threshold_bytes":threshold,"copy_error":null,"not_folder":shown})));
        }
    };
    let (bytes, copy_error) = crate::artifacts::blocking(move || {
        crate::agents::source_tree_info(&folder, Some(threshold))
    }).await?;
    Ok(Json(
        json!({"bytes":bytes,"threshold_bytes":threshold,"copy_error":copy_error}),
    ))
}

fn relative(s: &str) -> anyhow::Result<&Path> {
    let p = Path::new(s);
    anyhow::ensure!(
        !p.is_absolute() && p.components().all(|c| matches!(c, Component::Normal(_))),
        "Percorso non valido"
    );
    Ok(p)
}
fn resolve(base: &Path, s: &str) -> anyhow::Result<PathBuf> {
    let p = base.join(relative(s)?);
    let mut cursor = base.to_path_buf();
    for c in relative(s)?.components() {
        cursor.push(c);
        anyhow::ensure!(
            !std::fs::symlink_metadata(&cursor)?.file_type().is_symlink(),
            "Link simbolici non supportati"
        );
    }
    Ok(p)
}
/// Run workspaces hide and export neither `.git`, `target` nor dependency and cache folders, at
/// any depth: see `project_tree::omitted`. /sources and examples show every real folder.
/// A path component that does not exist yet counts as a folder.
fn workspace_omitted(base: &Path, rest: &str) -> bool {
    let mut cursor = base.to_path_buf();
    Path::new(rest).components().any(|c| {
        cursor.push(c);
        let is_file = std::fs::symlink_metadata(&cursor).is_ok_and(|m| m.is_file());
        crate::project_tree::omitted(c.as_os_str(), is_file)
    })
}
fn workspace_path(app: &App, uid: i64, path: &str) -> anyhow::Result<PathBuf> {
    relative(path)?;
    let (id, rest) = path.split_once('/').unwrap_or((path, ""));
    anyhow::ensure!(!id.is_empty(), "Seleziona un'esecuzione");
    let run = app.store.run(uid, id)?;
    let root = crate::subworkflows::workspace(app, uid, &run)?;
    let base = resolve(&root, "")?;
    anyhow::ensure!(
        std::fs::canonicalize(&base)? == std::fs::canonicalize(&run.workspace)?,
        "Workspace non valido"
    );
    anyhow::ensure!(
        !workspace_omitted(&base, rest),
        "Cartella interna esclusa dall'esportazione"
    );
    resolve(&base, rest)
}
fn storage_path(app: &App, uid: i64, q: &Location) -> anyhow::Result<PathBuf> {
    if q.root == "workspaces" {
        workspace_path(app, uid, &q.path)
    } else {
        resolve(&root(app, &q.root)?, &q.path)
    }
}
fn reports(
    app: &App,
    uid: i64,
    id: &str,
) -> anyhow::Result<std::collections::BTreeMap<String, String>> {
    app.store.run(uid, id)?;
    let mut latest = std::collections::BTreeMap::new();
    let mut artifacts = app.store.artifacts(uid, id)?;
    artifacts.sort_by_key(|a| a.version);
    for a in artifacts {
        if crate::source_access::report_path(&a.path) && relative(&a.path).is_ok() {
            latest.insert(a.path, a.content);
        }
    }
    Ok(latest)
}
fn report_view(app: &App, uid: i64, path: &str) -> anyhow::Result<Option<Value>> {
    relative(path)?;
    let (id, rest) = path.split_once('/').unwrap_or((path, ""));
    let dir = rest.split('/').next().unwrap_or("");
    if !crate::source_access::internal_dir(dir) {
        return Ok(None);
    }
    let report_dir = format!("{dir}/reports");
    if rest != dir && rest != report_dir && !crate::source_access::report_path(rest) {
        return Ok(None);
    }
    let reports = reports(app, uid, id)?;
    if let Some(content) = reports.get(rest) {
        let settings=app.store.settings(uid)?;
        let limit=settings.file_preview_limit_bytes()?;
        let preview=(content.len() as u64<=limit && !content.contains('\0')).then_some(content);
        let reason=if preview.is_some(){None}else{Some(if content.len() as u64>limit {"size"}else{"format"})};
        return Ok(Some(json!({"path":path,"file":true,"preview":preview,"html":preview.map(|s|crate::api::render_markdown(s)),"size":content.len(),"preview_reason":reason,"preview_limit_mb":settings.file_preview_limit_mb})));
    }
    if rest == report_dir {
        return Ok(Some(
            json!({"path":path,"entries":reports.keys().filter(|p|p.starts_with(&format!("{report_dir}/"))).map(|p|json!({"name":p.trim_start_matches(&format!("{report_dir}/")),"directory":false,"size":reports[p].len()})).collect::<Vec<_>>()}),
        ));
    }
    if rest == dir && workspace_path(app, uid, path).is_err() {
        return Ok(Some(
            json!({"path":path,"entries":[{"name":"reports","directory":true,"size":0}]}),
        ));
    }
    Ok(None)
}
/// Logical byte totals, never file contents or symlink targets. Runs on a
/// blocking worker; directory listings remain usable while totals are computed.
fn folder_bytes(path: &Path, omit_build: bool) -> anyhow::Result<u64> {
    let meta = std::fs::symlink_metadata(path)?;
    anyhow::ensure!(!meta.is_symlink(), "Link simbolico non supportato");
    if meta.is_file() {
        return Ok(meta.len());
    }
    let mut total = 0u64;
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_symlink() || (omit_build && crate::project_tree::omitted(&entry.file_name(), kind.is_file())) {
            continue;
        }
        total = total.saturating_add(folder_bytes(&entry.path(), omit_build)?);
    }
    Ok(total)
}
#[cfg(test)]
fn logical_bytes(app: &App, uid: i64, q: &Location) -> anyhow::Result<u64> {
    logical_bytes_cached(app, uid, q, &mut std::collections::HashMap::new())
}
fn logical_bytes_cached(
    app: &App,
    uid: i64,
    q: &Location,
    cache: &mut std::collections::HashMap<PathBuf, u64>,
) -> anyhow::Result<u64> {
    let physical = storage_path(app, uid, q).ok();
    // Several progressive runs can share one workspace. Measure it once per request.
    let mut total = if let Some(path) = &physical {
        if let Some(bytes) = cache.get(path) {
            *bytes
        } else {
            let bytes = folder_bytes(path, q.root == "workspaces")?;
            cache.insert(path.clone(), bytes);
            bytes
        }
    } else {
        0
    };
    let mut has_report = false;
    if q.root == "workspaces" {
        let (id, rest) = q.path.split_once('/').unwrap_or((&q.path, ""));
        let prefix = if rest.is_empty() {
            String::new()
        } else {
            format!("{rest}/")
        };
        for (report, content) in reports(app, uid, id)? {
            if let Some(relative) = report.strip_prefix(&prefix) {
                has_report = true;
                if let Some(path) = &physical {
                    let existing = path.join(relative);
                    if let Ok(m) = existing.symlink_metadata()
                        && m.is_file()
                    {
                        total = total.saturating_sub(m.len());
                    }
                }
                total = total.saturating_add(content.len() as u64);
            }
        }
    }
    anyhow::ensure!(physical.is_some() || has_report, "Cartella non disponibile");
    Ok(total)
}
pub async fn entry_sizes(
    State(app): State<App>,
    user: User,
    Query(q): Query<Location>,
) -> ApiResult<Json<Value>> {
    let entries = list(
        State(app.clone()),
        user.clone(),
        Query(Location {
            path: q.path.clone(),
            root: q.root.clone(),
            directory: false,
        }),
    )
    .await?
    .0;
    let names: Vec<String> = entries["entries"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|e| e["directory"] == true)
        .filter_map(|e| e["name"].as_str().map(str::to_owned))
        .collect();
    let sizes = crate::artifacts::blocking(move || {
        let mut sizes = serde_json::Map::new();
        let mut cache = std::collections::HashMap::new();
        for name in names {
            let path = if q.path.is_empty() {
                name.clone()
            } else {
                format!("{}/{name}", q.path)
            };
            let size = logical_bytes_cached(
                &app,
                user.id,
                &Location {
                    root: q.root.clone(),
                    path,
                    directory: false,
                },
                &mut cache,
            )
            .ok();
            sizes.insert(name, json!(size));
        }
        Ok(sizes)
    })
    .await?;
    Ok(Json(json!({"sizes":sizes})))
}

pub async fn list(
    State(app): State<App>,
    u: User,
    Query(q): Query<Location>,
) -> ApiResult<Json<Value>> {
    if q.root == "workspaces" && q.path.is_empty() {
        let names = app.store.workflow_names(u.id)?;
        let entries: Vec<_> = app.store.runs(u.id)?.iter().map(|r| json!({"name":r.id,"label":format!("{} · {}",crate::api::run_name(&names, r),r.id.chars().take(8).collect::<String>()),"directory":true,"size":0,"status":r.status,"source_mode":if r.context.contains_key("direct-primary") {"direct"} else {"copy"}})).collect();
        return Ok(Json(json!({"path":"","entries":entries})));
    }
    if q.root == "workspaces"
        && let Some(view) = report_view(&app, u.id, &q.path)?
    {
        return Ok(Json(view));
    }
    let p = storage_path(&app, u.id, &q)?;
    if p.is_file() {
        let size = p.metadata()?.len();
        let settings=app.store.settings(u.id)?;
        let limit=settings.file_preview_limit_bytes()?;
        let decoded=crate::artifacts::read_text(&p,limit)?;
        let encoding=decoded.as_ref().and_then(|d|d.encoding);
        let preview=decoded.map(|d|d.text);
        let reason=if preview.is_some(){None}else{Some(if size>limit {"size"}else{"format"})};
        let view=crate::api::inline_view(&q.path).map(|(kind,_)|kind).filter(|kind|size<=limit&&(*kind!="html"||preview.is_some()));
        let reason=if view.is_some(){None}else{reason};
        return Ok(Json(json!({"path":q.path,"file":true,"html":if q.path.ends_with(".md") { preview.as_ref().map(|text|crate::api::render_markdown(text)) } else { None },"preview":preview,"view":view,"encoding":encoding,"size":size,"preview_reason":reason,"preview_limit_mb":settings.file_preview_limit_mb})));
    }
    let mut entries = Vec::new();
    for e in std::fs::read_dir(p)?.take(10001) {
        let e = e?;
        let m = e.path().symlink_metadata()?;
        if m.file_type().is_symlink()
            || (q.root == "workspaces" && crate::project_tree::omitted(&e.file_name(), m.is_file()))
        {
            continue;
        }
        entries.push(
            json!({"name":e.file_name().to_string_lossy(),"directory":m.is_dir(),"size":m.len()}),
        );
    }
    if q.root == "workspaces" {
        let (id, rest) = q.path.split_once('/').unwrap_or((&q.path, ""));
        let report_files = reports(&app, u.id, id)?;
        for dir in [".rightcommit", ".nodo"] {
            let name = if rest.is_empty() {
                Some(dir)
            } else if rest == dir {
                Some("reports")
            } else {
                None
            };
            if let Some(name) = name
                && report_files.keys().any(|p|p.starts_with(&format!("{dir}/reports/")))
                && !entries.iter().any(|e| e["name"] == name)
            {
                entries.push(json!({"name":name,"directory":true,"size":0}));
            }
        }
    }
    if entries.len() > 10000 {
        return Err(bad("Cartella troppo grande"));
    }
    entries.sort_by_key(|e| {
        (
            !e["directory"].as_bool().unwrap_or(false),
            e["name"].as_str().unwrap_or("").to_lowercase(),
        )
    });
    Ok(Json(json!({"path":q.path,"entries":entries})))
}
/// A Storage image or HTML file for the in-app viewer; see `api::inline_response`.
pub async fn view(
    State(app): State<App>,
    u: User,
    Query(q): Query<Location>,
) -> ApiResult<axum::response::Response> {
    let (_, content_type) = crate::api::inline_view(&q.path)
        .ok_or_else(|| bad("Anteprima non disponibile per questo formato"))?;
    let path = storage_path(&app, u.id, &q)?;
    let limit = app.store.settings(u.id)?.file_preview_limit_bytes()?;
    let meta = path.symlink_metadata()?;
    if !meta.is_file() {
        return Err(bad("File non trovato"));
    }
    if meta.len() > limit {
        return Err(bad("Il file supera il limite di anteprima"));
    }
    let bytes = tokio::fs::read(path).await.map_err(|e| bad(e.to_string()))?;
    Ok(crate::api::inline_response(crate::api::inline_bytes(bytes, content_type), content_type))
}
pub async fn upload(
    State(app): State<App>,
    _u: User,
    Query(q): Query<Location>,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    if ["examples", "workspaces"].contains(&q.root.as_str()) {
        return Err(crate::auth::ApiError(
            StatusCode::FORBIDDEN,
            "Archivio di sola lettura".into(),
        ));
    }
    let base = root(&app, &q.root)?;
    let rel = relative(&q.path)?;
    if q.path.is_empty() {
        return Err(bad("Nome file mancante"));
    }
    let mut parent = base.clone();
    if let Some(dirs) = rel.parent() {
        for c in dirs.components() {
            parent.push(c);
            match std::fs::create_dir(&parent) {
                Ok(()) => (),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
                Err(e) => return Err(e.into()),
            }
            let m = parent.symlink_metadata()?;
            if !m.is_dir() || m.file_type().is_symlink() {
                return Err(bad("Percorso cartella non valido"));
            }
        }
    }
    let dest = parent.join(rel.file_name().ok_or_else(|| bad("Nome mancante"))?);
    if q.directory {
        match std::fs::create_dir(&dest) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let m = dest.symlink_metadata()?;
                if !m.is_dir() || m.file_type().is_symlink() {
                    return Err(bad("Conflitto con un file esistente"));
                }
            }
            Err(e) => return Err(e.into()),
        }
        return Ok(Json(json!({"ok":true,"path":q.path,"directory":true})));
    }
    let mut f = match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&dest)
    {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(crate::auth::ApiError(
                StatusCode::CONFLICT,
                "Esiste già un file con questo nome".into(),
            ));
        }
        Err(e) => return Err(e.into()),
    };
    use std::io::Write;
    if let Err(e) = f.write_all(&body) {
        drop(f);
        let _ = std::fs::remove_file(dest);
        return Err(e.into());
    }
    Ok(Json(json!({"ok":true,"path":q.path,"size":body.len()})))
}
pub async fn download(
    State(app): State<App>,
    u: User,
    Query(q): Query<Location>,
) -> ApiResult<impl IntoResponse> {
    let mut report_files = std::collections::BTreeMap::new();
    let mut selected = String::new();
    let p = if q.root == "workspaces" {
        relative(&q.path)?;
        let (id, rest) = q.path.split_once('/').unwrap_or((&q.path, ""));
        report_files = reports(&app, u.id, id)?;
        selected = rest.to_string();
        if crate::source_access::internal_dir(rest) || [".nodo/reports", ".rightcommit/reports"].contains(&rest) || report_files.contains_key(rest) {
            workspace_path(&app, u.id, id)?.join(rest)
        } else {
            workspace_path(&app, u.id, &q.path)?
        }
    } else {
        storage_path(&app, u.id, &q)?
    };
    let omit = if q.root == "workspaces" { crate::project_tree::export_rules().to_string() } else { "{}".into() };
    let bytes=tokio::task::spawn_blocking(move||->anyhow::Result<Vec<u8>>{
 let mut child=std::process::Command::new("python3").args(["-c",r#"
import sys,os,zipfile,io
from pathlib import Path
p=Path(sys.argv[1]); files=[]
rules=__import__('json').loads(sys.argv[2]); any_type=set(rules.get('any',[])); folders=set(rules.get('folders',[]))
# A file named like a dependency folder is content; a link with that name is left out unread.
def skip(parent,n):
 f=Path(parent)/n
 return n in any_type or (n in folders and (f.is_symlink() or not f.is_file()))
if p.is_dir():
 for parent,dirs,names in os.walk(p,followlinks=False):
  dirs[:]=sorted(d for d in dirs if not skip(parent,d))
  files.extend(Path(parent)/d for d in dirs)
  files.extend(Path(parent)/n for n in sorted(names) if not skip(parent,n))
  if len(files)>10000: raise ValueError('Limite di 10000 elementi superato')
else: files=[p] if p.exists() else []
reports=__import__('json').load(sys.stdin); selected=Path(sys.argv[3]) if sys.argv[3] else None
total=0; out=io.BytesIO()
with zipfile.ZipFile(out,'w',zipfile.ZIP_DEFLATED) as z:
 for f in files:
  if f.is_symlink() or any(a.is_symlink() for a in f.parents): raise ValueError('Link simbolico non supportato')
  total+=f.stat().st_size
  if total>200*1024*1024: raise ValueError('Limite ZIP di 200 MB superato')
  z.write(f, str(Path(p.name)/f.relative_to(p)) if p.is_dir() else f.name)
 for name,content in reports.items():
  rp=Path(name)
  if selected is None: rel=Path(p.name)/rp
  elif rp==selected: rel=Path(rp.name)
  elif selected in rp.parents: rel=Path(p.name)/rp.relative_to(selected)
  else: continue
  total+=len(content.encode())
  if total>200*1024*1024 or len(z.namelist())>=10000: raise ValueError('Limite ZIP superato')
  if str(rel) not in z.namelist(): z.writestr(str(rel),content)
 if not z.namelist() and not p.is_dir(): raise ValueError('File non trovato')
sys.stdout.buffer.write(out.getvalue())
"#]).arg(p).arg(omit).arg(selected).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn()?;
 use std::io::Write;
 let mut stdin=child.stdin.take().ok_or_else(||anyhow::anyhow!("Input ZIP non disponibile"))?;
 stdin.write_all(&serde_json::to_vec(&report_files)?)?;drop(stdin);
 let output=child.wait_with_output()?;anyhow::ensure!(output.status.success(),"Download ZIP non disponibile: limite 200 MB / 10000 elementi o link simbolici");Ok(output.stdout)
 }).await.map_err(|_|bad("Download interrotto"))??;
    Ok((
        [
            (header::CONTENT_TYPE, "application/zip".to_string()),
            (header::CONTENT_DISPOSITION, attachment(&download_name(&app, u.id, &q))),
        ],
        bytes,
    ))
}
/// The ZIP is named after the file or folder it holds. A whole root keeps the root's name,
/// and a whole run is named after its workflow and short id instead of its folder id.
fn download_name(app: &App, uid: i64, q: &Location) -> String {
    let parts: Vec<&str> = q.path.split('/').filter(|part| !part.is_empty()).collect();
    let name = match parts.as_slice() {
        [] => q.root.clone(),
        [id] if q.root == "workspaces" => match (app.store.run(uid, id), app.store.workflow_names(uid)) {
            (Ok(run), Ok(names)) => format!("{} {}", crate::api::run_name(&names, &run), id.chars().take(8).collect::<String>()),
            _ => "result".into(),
        },
        [.., last] => (*last).to_string(),
    };
    format!("{name}.zip")
}
/// `Content-Disposition` for a download: an ASCII name for old clients and the exact UTF-8 name.
fn attachment(name: &str) -> String {
    let ascii: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || "-_. ".contains(c) { c } else { '_' })
        .collect();
    let encoded: String = name
        .bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b"-_.".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect();
    format!("attachment; filename=\"{ascii}\"; filename*=UTF-8''{encoded}")
}
impl From<std::io::Error> for crate::auth::ApiError {
    fn from(e: std::io::Error) -> Self {
        bad(e.to_string())
    }
}

pub(crate) fn rag_source(app: &App, uid: i64, root: &str, path: &str) -> anyhow::Result<PathBuf> {
    anyhow::ensure!(
        !path.split('/').any(crate::source_access::internal_dir),
        "I report virtuali non sono sorgenti indicizzabili"
    );
    storage_path(
        app,
        uid,
        &Location {
            root: root.into(),
            path: path.into(),
            directory: false,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn folder_totals_use_metadata_and_skip_links() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir(d.path().join("nested")).unwrap();
        std::fs::write(d.path().join("a.txt"), b"abc").unwrap();
        let large = std::fs::File::create(d.path().join("nested/large.bin")).unwrap();
        large.set_len(4 * 1024 * 1024 * 1024).unwrap();
        std::fs::create_dir(d.path().join("target")).unwrap();
        std::fs::write(d.path().join("target/cache"), b"cache").unwrap();
        std::os::unix::fs::symlink(d.path(), d.path().join("loop")).unwrap();
        assert_eq!(folder_bytes(d.path(), false).unwrap(), 4 * 1024 * 1024 * 1024 + 8);
        assert_eq!(folder_bytes(d.path(), true).unwrap(), 4 * 1024 * 1024 * 1024 + 3);
    }
    #[tokio::test]
    async fn workspace_access_is_owned_and_excludes_internal_paths() {
        let d = tempfile::tempdir().unwrap();
        let app = crate::tests::app(d.path());
        let run = crate::engine::create(
            &app,
            1,
            crate::presets::all().remove(0),
            "storage test".into(),
            None,
        )
        .await
        .unwrap();
        let path = format!("{}/src/lib.rs", run.id);
        assert!(workspace_path(&app, 1, &path).unwrap().is_file());
        assert!(workspace_path(&app, 2, &path).is_err());
        assert!(workspace_path(&app, 1, &format!("{}/../..", run.id)).is_err());
        assert!(workspace_path(&app, 1, &format!("{}/.git/config", run.id)).is_err());
        assert!(workspace_path(&app, 1, &format!("{}/target/file", run.id)).is_err());
        assert!(workspace_path(&app, 1, "").is_err());
        std::os::unix::fs::symlink("/etc", Path::new(&run.workspace).join("escape")).unwrap();
        assert!(workspace_path(&app, 1, &format!("{}/escape/passwd", run.id)).is_err());
    }
    #[tokio::test]
    async fn downloads_are_named_after_the_file_folder_or_run() {
        let d = tempfile::tempdir().unwrap();
        let app = crate::tests::app(d.path());
        let run = crate::engine::create(&app, 1, crate::presets::all().remove(0), "storage test".into(), None)
            .await
            .unwrap();
        let name = |root: &str, path: &str| download_name(&app, 1, &Location { path: path.into(), directory: false, root: root.into() });
        assert_eq!(name("sources", "project/src"), "src.zip");
        assert_eq!(name("sources", "project/Relazione finale.md"), "Relazione finale.md.zip");
        assert_eq!(name("examples", "demo/"), "demo.zip");
        // A whole root keeps its name; a whole run uses its workflow name, not its folder id.
        assert_eq!(name("sources", ""), "sources.zip");
        assert_eq!(name("examples", ""), "examples.zip");
        let short: String = run.id.chars().take(8).collect();
        assert_eq!(name("workspaces", &run.id), format!("{} {short}.zip", run.workflow.name));
        assert_eq!(name("workspaces", &format!("{}/src", run.id)), "src.zip");
        assert_eq!(name("workspaces", "unknown-run"), "result.zip");
        assert_eq!(
            attachment("Attività \"2026\".zip"),
            "attachment; filename=\"Attivit_ _2026_.zip\"; filename*=UTF-8''Attivit%C3%A0%20%222026%22.zip"
        );
    }
    #[tokio::test]
    async fn reports_support_current_and_legacy_namespaces() {
        let d = tempfile::tempdir().unwrap();
        let app = crate::tests::app(d.path());
        let run = crate::engine::create(&app, 1, crate::presets::all().remove(0), "reports".into(), None).await.unwrap();
        for dir in [".rightcommit", ".nodo"] {
            let path = format!("{dir}/reports/task.md");
            let artifact: crate::model::Artifact = serde_json::from_value(json!({
                "id":crate::model::id(), "run":run.id, "node":"task", "path":path,
                "version":1, "hash":"test", "content":dir, "session":"", "turn":0, "created":0
            })).unwrap();
            app.store.add_artifact(1, &artifact).unwrap();
            let view = report_view(&app, 1, &format!("{}/{path}",run.id)).unwrap().unwrap();
            assert_eq!(view["preview"], dir);
            let q = Location {root:"workspaces".into(), path:format!("{}/{dir}/reports",run.id), directory:false};
            assert_eq!(logical_bytes(&app, 1, &q).unwrap(), dir.len() as u64);
            assert!(logical_bytes(&app, 2, &q).is_err());
            std::fs::create_dir_all(Path::new(&run.workspace).join(format!("{dir}/reports"))).unwrap();
            std::fs::write(Path::new(&run.workspace).join(&path), "outdated physical report").unwrap();
            assert_eq!(logical_bytes(&app, 1, &q).unwrap(), dir.len() as u64);

            let listing = report_view(&app, 1, &format!("{}/{dir}/reports",run.id)).unwrap().unwrap();
            assert_eq!(listing["entries"].as_array().unwrap().len(), 1);
            assert_eq!(listing["entries"][0]["name"], "task.md");
            assert!(report_view(&app, 2, &format!("{}/{path}",run.id)).is_err());
            assert!(rag_source(&app, 1, "workspaces", &format!("{}/{path}",run.id)).is_err());
        }
    }
    #[tokio::test]
    async fn workspaces_hide_and_export_without_dependency_folders() {
        use axum::response::IntoResponse;
        let d = tempfile::tempdir().unwrap();
        // The ZIP refuses a link anywhere above its files, and the macOS temp folder sits behind one.
        let base = std::fs::canonicalize(d.path()).unwrap();
        let app = crate::tests::app(&base);
        let run = crate::engine::create(&app, 1, crate::presets::all().remove(0), "storage test".into(), None).await.unwrap();
        let ws = Path::new(&run.workspace);
        crate::source_access::tests::venv(&ws.join(".venv"));
        crate::source_access::tests::venv(&ws.join("services/api/venv"));
        std::fs::create_dir_all(ws.join("web/node_modules/.bin")).unwrap();
        std::os::unix::fs::symlink("/etc", ws.join("web/node_modules/.bin/tool")).unwrap();
        std::fs::create_dir_all(ws.join("app/.dart_tool")).unwrap();
        std::fs::write(ws.join("app/.dart_tool/state"), "cache").unwrap();
        std::fs::write(ws.join("services/venv"), "a file, not a folder").unwrap();
        let user = || User { id: 1, email: String::new() };
        let query = |root: &str, path: String| Query(Location { root: root.into(), path, directory: false });
        let names = |listing: Value| listing["entries"].as_array().unwrap().iter().map(|e| e["name"].as_str().unwrap().to_owned()).collect::<Vec<_>>();
        let top = names(list(State(app.clone()), user(), query("workspaces", run.id.clone())).await.unwrap().0);
        assert!(!top.contains(&".venv".to_string()) && top.contains(&"services".to_string()), "{top:?}");
        let services = names(list(State(app.clone()), user(), query("workspaces", format!("{}/services", run.id))).await.unwrap().0);
        assert!(services.contains(&"venv".to_string()), "a file named venv is shown: {services:?}");
        let api = names(list(State(app.clone()), user(), query("workspaces", format!("{}/services/api", run.id))).await.unwrap().0);
        assert!(api.is_empty(), "{api:?}");
        assert!(workspace_path(&app, 1, &format!("{}/services/venv", run.id)).unwrap().is_file());
        assert!(workspace_path(&app, 1, &format!("{}/services/api/venv/pyvenv.cfg", run.id)).is_err());
        assert!(workspace_path(&app, 1, &format!("{}/app/.dart_tool/state", run.id)).is_err());
        let q = Location { root: "workspaces".into(), path: format!("{}/services", run.id), directory: false };
        assert_eq!(logical_bytes(&app, 1, &q).unwrap(), "a file, not a folder".len() as u64);
        // The result ZIP used to fail on the links inside .venv; now it leaves the folders out.
        let response = download(State(app.clone()), user(), query("workspaces", run.id.clone())).await.unwrap().into_response();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let zip = d.path().join("result.zip");
        std::fs::write(&zip, &bytes).unwrap();
        let listed = std::process::Command::new("python3")
            .args(["-c", "import sys,zipfile;print('\\n'.join(zipfile.ZipFile(sys.argv[1]).namelist()))"])
            .arg(&zip).output().unwrap();
        let listed = String::from_utf8(listed.stdout).unwrap();
        assert!(listed.lines().any(|l| l.ends_with("services/venv")), "{listed}");
        assert!(listed.lines().any(|l| l.ends_with("src/lib.rs")), "{listed}");
        for hidden in [".venv", "api/venv", "node_modules", ".dart_tool", ".git/"] {
            assert!(!listed.contains(hidden), "{hidden} in ZIP:\n{listed}");
        }
        // /sources shows every real folder, as before.
        let source = app.sources.join("project");
        std::fs::create_dir_all(source.join(".venv/lib")).unwrap();
        assert_eq!(names(list(State(app.clone()), user(), query("sources", "project".into())).await.unwrap().0), [".venv"]);
    }
    #[test]
    fn rejects_escape_and_symlinks() {
        let t = tempfile::tempdir().unwrap();
        assert!(relative("../secret").is_err());
        assert!(relative("/etc/passwd").is_err());
        assert!(relative("foo/bar").is_ok());
        std::os::unix::fs::symlink("/etc", t.path().join("link")).unwrap();
        assert!(resolve(t.path(), "link/passwd").is_err());
        assert!(resolve(t.path(), "").is_ok());
    }
}
