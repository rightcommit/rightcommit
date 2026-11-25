//! The local commit step commits only what the run changed. Copied Sources start from a
//! fresh baseline commit, but a direct Source is the user's own folder and may already hold
//! staged, unstaged or untracked edits. When the run takes the workspace it records which
//! paths were already changed, with their index entry and content; the commit then takes
//! only the other paths and leaves the recorded ones, staged state included, as they were.
//! A file changed both before and during the run cannot be split without guessing, so the
//! commit stops and names it instead.
//!
//! With the node's confirmation on, the run first shows what the commit will contain and
//! waits for approval, like a Connection that writes to an external service. The preview
//! changes nothing in the repository, and the approval holds only for the exact content
//! that was shown: the commit checks it again just before committing.
use crate::{App, agents, model::Run};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsStr,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

/// Run context key with the paths already changed when the run took the workspace.
pub(crate) const START: &str = "git-start";
const MESSAGE: &str = "SDD: verified workflow delivery";
const UNREADABLE: &str = "unreadable";
/// Preview limits. They keep the approval readable, the saved run small and the supervisor
/// prompt bounded; every file is still counted and the digest covers all of the content.
const LISTED: usize = 1000;
const DIFF_FILES: usize = 200;
const FILE_LINES: usize = 400;
const FILE_BYTES: usize = 48 * 1024;
const TOTAL_BYTES: usize = 128 * 1024;
/// A new file larger than this is named with its size; reading it would not fit the limits.
const NEW_FILE_BYTES: u64 = 1024 * 1024;
const TEST_CHARS: usize = 8000;
const KEPT_LISTED: usize = 200;
const TEST_LINE: &str = "Comando di test: ";

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    File,
    /// Submodules, nested repositories and merge conflicts: committing them is not a file change.
    Unsupported,
}
struct Change {
    path: Vec<u8>,
    /// Index entry, so a change to the staged version counts as well.
    index: String,
    kind: Kind,
}

/// What the commit will contain, shown with the approval request.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Preview {
    /// The files the commit changes, in path order, at most `LISTED`.
    pub files: Vec<PreviewFile>,
    /// Files in the commit beyond the listed ones.
    pub more_files: usize,
    /// Edits that were there before the run and stay out of the commit.
    pub kept: Vec<String>,
    pub more_kept: usize,
    /// An approval was given, but the content changed before the commit.
    pub changed: bool,
    pub test: TestEvidence,
    pub limits: PreviewLimits,
}
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct PreviewFile {
    pub path: String,
    /// added, modified or deleted.
    pub status: String,
    pub binary: bool,
    /// Lines added and removed; absent when the file was not read.
    pub added: Option<u64>,
    pub removed: Option<u64>,
    /// Diff hunks, without git's file header.
    pub diff: String,
    /// The diff stops before its end.
    pub cut: bool,
    /// Why no diff is shown: too_large (a new file over the size limit) or limit (the
    /// preview already shows as much as it can).
    pub note: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TestEvidence {
    /// missing: the workflow has no local test step; not_run: none completed in this run; ran.
    pub state: String,
    pub node: String,
    /// The command as the Activity Log names it, with where it came from.
    pub command: String,
    /// The end of the test output.
    pub output: String,
    pub output_cut: bool,
    /// The tests ran on the files as they are now.
    pub current: bool,
}
/// The limits, sent along so the approval states them.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct PreviewLimits {
    pub file_lines: usize,
    pub file_bytes: usize,
    pub total_bytes: usize,
    pub diff_files: usize,
    pub new_file_bytes: u64,
    pub test_chars: usize,
}

fn os(bytes: &[u8]) -> &OsStr {
    OsStr::from_bytes(bytes)
}
fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}
fn records(out: &[u8]) -> impl Iterator<Item = &[u8]> {
    out.split(|b| *b == 0).filter(|r| !r.is_empty())
}

/// Git that only reads the repository: no optional index refresh written back by `status`,
/// no hook (`post-index-change` would run for the temporary index), no fsmonitor daemon
/// files and no split-index files in `.git`.
async fn git(app: &App, uid: i64, root: &Path, args: &[&OsStr], env: &[(&str, &OsStr)], timeout: u64) -> Result<Vec<u8>> {
    git_capped(app, uid, root, args, env, timeout, usize::MAX).await.map(|(out, _)| out)
}
async fn git_capped(app: &App, uid: i64, root: &Path, args: &[&OsStr], env: &[(&str, &OsStr)], timeout: u64, cap: usize) -> Result<(Vec<u8>, bool)> {
    let mut env = env.to_vec();
    env.push(("GIT_OPTIONAL_LOCKS", os(b"0")));
    let mut all = vec![os(b"-c"), os(b"core.hooksPath=/dev/null"), os(b"-c"), os(b"core.fsmonitor=false"), os(b"-c"), os(b"core.splitIndex=false")];
    all.extend_from_slice(args);
    agents::git_with(app, uid, root, &all, &env, timeout, cap).await
}

/// Repository root and the workspace's place in it: a direct Source may be a subfolder of a
/// larger repository, and only that folder belongs to the run.
async fn location(app: &App, uid: i64, workspace: &Path) -> Result<(PathBuf, PathBuf)> {
    let out = agents::git_bytes(app, uid, workspace, &[os(b"rev-parse"), os(b"--show-toplevel")], 30).await?;
    let top = PathBuf::from(os(out.strip_suffix(b"\n").unwrap_or(&out)));
    let (top, workspace) = (std::fs::canonicalize(&top)?, std::fs::canonicalize(workspace)?);
    let scope = workspace.strip_prefix(&top).context("Workspace fuori dal repository Git")?.to_path_buf();
    Ok((top, if scope.as_os_str().is_empty() { ".".into() } else { scope }))
}

async fn status(app: &App, uid: i64, root: &Path, scope: &Path) -> Result<Vec<Change>> {
    // Literal pathspecs: a folder name such as `*` or `:(top)` must not widen the scope.
    let out = git(app, uid, root, &[
        os(b"--literal-pathspecs"), os(b"status"), os(b"--porcelain=v2"), os(b"-z"),
        os(b"--no-renames"), os(b"--untracked-files=all"), os(b"--"), scope.as_os_str(),
    ], &[], 120).await?;
    let mut changes = vec![];
    let mut records = records(&out);
    while let Some(record) = records.next() {
        let field = |n: usize| record.splitn(n + 1, |b| *b == b' ').collect::<Vec<_>>();
        match record[0] {
            b'1' | b'2' => {
                let fields = field(if record[0] == b'1' { 8 } else { 9 });
                ensure!(fields.len() == if record[0] == b'1' { 9 } else { 10 }, "Stato Git non riconosciuto");
                let submodule = fields[2].starts_with(b"S") || fields[4] == b"160000" || fields[3] == b"160000";
                // Edits inside a submodule whose commit did not move are not a change here.
                if record[0] == b'1' && fields[2].starts_with(b"S") && fields[2].get(1) != Some(&b'C') && fields[1].starts_with(b".") {
                    continue;
                }
                let index = format!("{}:{}", String::from_utf8_lossy(fields[4]), String::from_utf8_lossy(fields[7]));
                let kind = if submodule { Kind::Unsupported } else { Kind::File };
                changes.push(Change { path: fields.last().unwrap().to_vec(), index: index.clone(), kind });
                // Renames are disabled; should one appear, its source path is a change too.
                if record[0] == b'2' {
                    let from = records.next().context("Stato Git non riconosciuto")?;
                    changes.push(Change { path: from.to_vec(), index, kind });
                }
            }
            b'u' => {
                let fields = field(10);
                ensure!(fields.len() == 11, "Stato Git non riconosciuto");
                changes.push(Change { path: fields[10].to_vec(), index: "unmerged".into(), kind: Kind::Unsupported });
            }
            b'?' => {
                let path = record.get(2..).context("Stato Git non riconosciuto")?;
                // With every untracked file listed, a directory is a nested repository.
                let kind = if path.ends_with(b"/") { Kind::Unsupported } else { Kind::File };
                changes.push(Change { path: path.to_vec(), index: "untracked".into(), kind });
            }
            _ => {}
        }
    }
    Ok(changes)
}

/// The commit the branch points to, or `None` on a branch that has no commit yet.
async fn head(app: &App, uid: i64, root: &Path) -> Result<Option<String>> {
    match git(app, uid, root, &[os(b"rev-parse"), os(b"--verify"), os(b"--quiet"), os(b"HEAD^{commit}")], &[], 30).await {
        Ok(out) => Ok(Some(text(&out).trim().to_owned())),
        // `--quiet` on an unborn branch: exit 1 and nothing on stderr.
        Err(e) if e.to_string().trim() == "git:" => Ok(None),
        Err(e) => Err(e),
    }
}

/// Content of a working-tree path, never following links or opening special files.
fn content(path: &Path) -> String {
    use sha2::{Digest, Sha256};
    use std::os::unix::fs::PermissionsExt;
    let meta = match std::fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return "absent".into(),
        Err(_) => return UNREADABLE.into(),
    };
    if meta.is_symlink() {
        return std::fs::read_link(path)
            .map(|target| format!("link:{}", hex::encode(Sha256::digest(target.as_os_str().as_bytes()))))
            .unwrap_or_else(|_| UNREADABLE.into());
    }
    if meta.is_dir() {
        return "directory".into();
    }
    if !meta.is_file() {
        return "special".into();
    }
    let mut hasher = Sha256::new();
    match std::fs::File::open(path).and_then(|mut f| std::io::copy(&mut f, &mut hasher)) {
        Ok(_) => format!("file:{}:{}", meta.permissions().mode() & 0o111 != 0, hex::encode(hasher.finalize())),
        Err(_) => UNREADABLE.into(),
    }
}

fn fingerprint(root: &Path, path: &[u8], index: &str) -> String {
    format!("{index}|{}", content(&root.join(os(path))))
}

/// The paths already changed in the workspace, recorded when the run takes it. `None` when
/// the workspace is not in a Git repository: there is nothing a local commit could carry.
pub(crate) async fn start(app: &App, uid: i64, workspace: &Path) -> Result<Option<String>> {
    let Ok((root, scope)) = location(app, uid, workspace).await else { return Ok(None) };
    let changes = status(app, uid, &root, &scope).await?;
    let paths = crate::artifacts::blocking(move || {
        Ok(changes.iter().map(|c| (hex::encode(&c.path), fingerprint(&root, &c.path, &c.index))).collect::<BTreeMap<_, _>>())
    }).await?;
    Ok(Some(serde_json::to_string(&paths)?))
}

fn shown(paths: &[Vec<u8>]) -> String {
    let mut text = paths.iter().take(20).map(|p| String::from_utf8_lossy(p).into_owned()).collect::<Vec<_>>().join(", ");
    if paths.len() > 20 {
        text.push_str(", …");
    }
    text
}

/// Whether the workspace is one of the copies rightcommit makes under the user's home; checked
/// on the path too, so an unexpected context never lets the user's own folder pass as a copy.
fn copied(app: &App, uid: i64, workspace: &Path) -> bool {
    let Ok(copies) = agents::user_home(app, uid).and_then(|home| Ok(std::fs::canonicalize(home.join("workspaces"))?)) else { return false };
    std::fs::canonicalize(workspace).is_ok_and(|w| w != copies && w.starts_with(&copies))
}

/// What the commit takes, worked out without changing the repository. Every reason to stop
/// is found here, so the preview stops with the same message the commit would.
struct Plan {
    root: PathBuf,
    run: Vec<Vec<u8>>,
    kept: Vec<Vec<u8>>,
    head: Option<String>,
    /// The starting commit, the content of every path the commit takes and the edits left
    /// out: any change to them gives another value, so an approval cannot cover it.
    digest: String,
}

async fn plan(app: &App, uid: i64, r: &Run) -> Result<Plan> {
    // A child workflow shares the workspace its root run took.
    let lineage = crate::subworkflows::ancestors(app, uid, r)?;
    let first = lineage.last().unwrap_or(r);
    let recorded: BTreeMap<String, String> = match first.context.get(START) {
        Some(raw) => serde_json::from_str(raw)?,
        // Runs started before the record: a copy began from its own baseline commit, so all of
        // it is the run's. Only a direct Source, the user's own folder, can hold earlier edits.
        None if !first.context.contains_key("direct-primary") && copied(app, uid, Path::new(&r.workspace)) => BTreeMap::new(),
        None => bail!("Commit locale fermato: lo stato Git del workspace all’avvio della run non è registrato, quindi le modifiche della run non si distinguono da quelle già presenti. Fai il commit a mano oppure avvia una nuova run"),
    };
    let (root, scope) = location(app, uid, Path::new(&r.workspace)).await?;
    let changes = status(app, uid, &root, &scope).await?;
    let head = head(app, uid, &root).await?;
    let (top, plan, start) = (root.clone(), recorded, head.clone());
    let (run, kept, review, unsupported, digest) = crate::artifacts::blocking(move || {
        let current: BTreeMap<&[u8], &Change> = changes.iter().map(|c| (c.path.as_slice(), c)).collect();
        let (mut kept, mut review) = (vec![], vec![]);
        for (key, before) in &plan {
            let path = hex::decode(key)?;
            let index = current.get(path.as_slice()).map_or("clean", |c| c.index.as_str());
            // An edit that was there before the run must still be exactly as it was; one that
            // could not be read then cannot be shown unchanged now.
            if before.ends_with(UNREADABLE) || fingerprint(&top, &path, index) != *before {
                review.push(path);
            } else {
                kept.push(path);
            }
        }
        let (mut run, mut unsupported) = (vec![], vec![]);
        for c in changes.iter().filter(|c| !plan.contains_key(&hex::encode(&c.path))) {
            if c.kind == Kind::File { run.push(c.path.clone()) } else { unsupported.push(c.path.clone()) }
        }
        // The commit takes these paths from the working tree, whatever their index entry says.
        let taken: Vec<_> = run.iter().map(|p| (hex::encode(p), content(&top.join(os(p))))).collect();
        let left: Vec<_> = kept.iter().map(|p| (hex::encode(p), &plan[&hex::encode(p)])).collect();
        let digest = crate::store::hash(serde_json::to_vec(&serde_json::json!({"head": start, "run": taken, "kept": left}))?);
        Ok((run, kept, review, unsupported, digest))
    }).await?;
    ensure!(
        review.is_empty(),
        "Commit locale fermato: questi file avevano già modifiche prima della run e sono cambiati anche durante la run, quindi le due modifiche non si separano con certezza: {}. Controllali e fai il commit a mano",
        shown(&review)
    );
    ensure!(
        unsupported.is_empty(),
        "Commit locale fermato: la run ha cambiato sottomoduli, repository Git annidati o file in conflitto, che il commit locale non gestisce: {}. Controllali e fai il commit a mano",
        shown(&unsupported)
    );
    Ok(Plan { root, run, kept, head, digest })
}

/// Commit the run's own changes and return the step's report. With an approved digest the
/// commit happens only if the content is still the one approved; otherwise `None`.
pub(crate) async fn commit(app: &App, uid: i64, r: &Run, approved: Option<&str>) -> Result<Option<String>> {
    let Plan { root, run, kept, digest, .. } = plan(app, uid, r).await?;
    if approved.is_some_and(|approved| approved != digest) {
        return Ok(None);
    }
    if !run.is_empty() {
        // `--only` builds the commit from these paths alone; other staged entries stay staged.
        let mut add = vec![os(b"--literal-pathspecs"), os(b"add"), os(b"--all"), os(b"--")];
        add.extend(run.iter().map(|p| os(p)));
        agents::git_bytes(app, uid, &root, &add, 60).await?;
        let mut commit = vec![
            os(b"--literal-pathspecs"), os(b"-c"), os(b"core.hooksPath=/dev/null"), os(b"commit"),
            os(b"--only"), os(b"-m"), OsStr::new(MESSAGE), os(b"--"),
        ];
        commit.extend(run.iter().map(|p| os(p)));
        agents::git_bytes(app, uid, &root, &commit, 60).await?;
    }
    let head = agents::tool(app, uid, &root, "git", &["rev-parse", "HEAD"], 30).await?;
    // One sentence per line, each a message the interface translates whole; the hash and the
    // file names inside it are shown as written.
    let mut lines = vec![
        format!("Commit locale verificato: {}.", head.trim()),
        if run.is_empty() { "File della run nel commit: nessuno.".into() } else { format!("File della run nel commit: {}.", shown(&run)) },
    ];
    if !kept.is_empty() {
        lines.push(format!("Modifiche già presenti prima della run, lasciate fuori dal commit come erano: {}.", shown(&kept)));
    }
    lines.push("Nessun push, merge o deploy eseguito.".into());
    Ok(Some(lines.join("\n")))
}

/// What the commit would contain, with the digest an approval must match. Nothing in the
/// repository changes: git reads it with a temporary index outside it, and no command here
/// writes objects, refs, the index or the working tree.
pub(crate) async fn preview(app: &App, uid: i64, r: &Run) -> Result<(Preview, String)> {
    let plan = plan(app, uid, r).await?;
    let mut preview = Preview {
        kept: plan.kept.iter().take(KEPT_LISTED).map(|p| text(p)).collect(),
        more_kept: plan.kept.len().saturating_sub(KEPT_LISTED),
        test: tests(app, uid, r).await?,
        limits: PreviewLimits { file_lines: FILE_LINES, file_bytes: FILE_BYTES, total_bytes: TOTAL_BYTES, diff_files: DIFF_FILES, new_file_bytes: NEW_FILE_BYTES, test_chars: TEST_CHARS },
        ..Default::default()
    };
    if !plan.run.is_empty() {
        // Under the user's rightcommit home, which git can write when it runs as that user.
        let index = agents::user_home(app, uid)?.join("tmp").join(format!("commit-preview-{}.index", crate::model::id()));
        let result = files(app, uid, &plan, &index, &mut preview).await;
        let mut lock = index.clone().into_os_string();
        lock.push(".lock");
        let _ = std::fs::remove_file(&index);
        let _ = std::fs::remove_file(lock);
        result?;
    }
    Ok((preview, plan.digest))
}

async fn files(app: &App, uid: i64, plan: &Plan, index: &Path, preview: &mut Preview) -> Result<()> {
    let root = &plan.root;
    // The temporary index holds just the starting commit, so a diff against it shows what
    // `commit --only` takes from the working tree, whatever the user's own index holds.
    let env = [("GIT_INDEX_FILE", index.as_os_str())];
    git(app, uid, root, &[os(b"read-tree"), if plan.head.is_some() { os(b"HEAD") } else { os(b"--empty") }], &env, 60).await?;
    let paths = |args: &[&'static str]| -> Vec<&OsStr> {
        let mut all: Vec<&OsStr> = args.iter().map(|a| OsStr::new(*a)).collect();
        all.push(os(b"--"));
        all.extend(plan.run.iter().map(|p| os(p)));
        all
    };
    let tracked: BTreeSet<Vec<u8>> = records(&git(app, uid, root, &paths(&["--literal-pathspecs", "ls-files", "-z"]), &env, 120).await?).map(<[u8]>::to_vec).collect();
    // Lines added and removed per path; `-` for both marks a binary file.
    let mut counts = BTreeMap::new();
    if plan.head.is_some() && !tracked.is_empty() {
        let out = git(app, uid, root, &paths(&["--literal-pathspecs", "diff", "--numstat", "-z", "--no-renames", "--no-ext-diff", "--no-textconv", "HEAD"]), &env, 300).await?;
        for record in records(&out) {
            let parts: Vec<_> = record.splitn(3, |b| *b == b'\t').collect();
            ensure!(parts.len() == 3, "Differenze Git non riconosciute");
            let number = |n: &[u8]| std::str::from_utf8(n).ok().and_then(|n| n.parse::<u64>().ok());
            counts.insert(parts[2].to_vec(), (number(parts[0]), number(parts[1])));
        }
    }
    let (mut budget, mut diffs) = (TOTAL_BYTES, 0);
    // Path order, as a diff lists them; git status puts untracked files last.
    let mut run: Vec<&Vec<u8>> = plan.run.iter().collect();
    run.sort();
    for path in run {
        let present = std::fs::symlink_metadata(root.join(os(path)));
        let mut file = PreviewFile { path: text(path), ..Default::default() };
        if tracked.contains(path) {
            // A tracked path missing from the counts is unchanged in the commit.
            let Some(&(added, removed)) = counts.get(path) else { continue };
            file.status = if present.is_ok() { "modified" } else { "deleted" }.into();
            (file.added, file.removed, file.binary) = (added, removed, added.is_none() && removed.is_none());
        } else if let Ok(meta) = &present {
            file.status = "added".into();
            file.removed = Some(0);
            if meta.is_file() && meta.len() > NEW_FILE_BYTES {
                file.note = "too_large".into();
            }
        } else {
            continue;
        }
        if preview.files.len() == LISTED {
            preview.more_files += 1;
            continue;
        }
        if file.binary || !file.note.is_empty() {
            preview.files.push(file);
            continue;
        }
        if diffs == DIFF_FILES || budget == 0 {
            // Counted, but beyond what one approval can show: a new file is still read for its size.
            if file.status == "added" {
                (file.binary, file.added) = added(&root.join(os(path)), 0).map_or((false, None), |(binary, lines, _, _)| (binary, Some(lines)));
            }
            if !file.binary {
                file.note = "limit".into();
            }
            preview.files.push(file);
            continue;
        }
        let cap = FILE_BYTES.min(budget);
        if file.status == "added" {
            let (binary, lines, diff, cut) = added(&root.join(os(path)), cap)?;
            (file.binary, file.added, file.diff, file.cut) = (binary, Some(lines), diff, cut);
        } else {
            let (out, cut) = git_capped(app, uid, root, &[
                os(b"--literal-pathspecs"), os(b"-c"), os(b"core.quotePath=false"), os(b"diff"), os(b"--no-ext-diff"), os(b"--no-textconv"),
                os(b"--no-color"), os(b"--no-renames"), os(b"--src-prefix=a/"), os(b"--dst-prefix=b/"), os(b"HEAD"), os(b"--"), os(path),
            ], &env, 120, cap).await?;
            (file.diff, file.cut) = hunks(&out, cut);
        }
        budget = budget.saturating_sub(file.diff.len());
        diffs += 1;
        preview.files.push(file);
    }
    Ok(())
}

/// The hunks of one file's git diff, at most `FILE_LINES` lines; mode changes stay, the
/// rest of the file header goes.
fn hunks(out: &[u8], cut: bool) -> (String, bool) {
    let mut lines: Vec<&[u8]> = out.split(|b| *b == b'\n').collect();
    // A cut output ends inside a line; a complete one ends with a newline.
    let mut cut = cut;
    if cut || lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    let start = lines.iter().position(|l| l.starts_with(b"@@")).unwrap_or(lines.len());
    let mut shown: Vec<&[u8]> = lines[..start].iter().copied().filter(|l| l.starts_with(b"old mode") || l.starts_with(b"new mode")).collect();
    let body = &lines[start..];
    cut |= body.len() > FILE_LINES;
    shown.extend(body.iter().take(FILE_LINES));
    let mut diff = shown.iter().map(|l| text(l)).collect::<Vec<_>>().join("\n");
    if !diff.is_empty() {
        diff.push('\n');
    }
    (diff, cut)
}

/// A new file as diff lines, read from the working tree without writing it to Git. Returns
/// whether it is binary (a NUL byte in its first 8000 bytes, as git decides), its line count,
/// up to `cap` bytes of diff and whether that diff stops early.
fn added(path: &Path, cap: usize) -> Result<(bool, u64, String, bool)> {
    let meta = std::fs::symlink_metadata(path)?;
    let bytes = if meta.is_symlink() {
        std::fs::read_link(path)?.as_os_str().as_bytes().to_vec()
    } else if meta.is_file() {
        std::fs::read(path)?
    } else {
        vec![]
    };
    if bytes[..bytes.len().min(8000)].contains(&0) {
        return Ok((true, 0, String::new(), false));
    }
    let complete = bytes.last().is_none_or(|b| *b == b'\n') && !meta.is_symlink();
    let lines: Vec<&[u8]> = bytes.strip_suffix(b"\n").unwrap_or(&bytes).split(|b| *b == b'\n').filter(|_| !bytes.is_empty()).collect();
    let mut diff = if lines.is_empty() { String::new() } else { format!("@@ -0,0 +1,{} @@\n", lines.len()) };
    let mut cut = false;
    for (i, line) in lines.iter().enumerate() {
        let line = format!("+{}\n", text(line));
        if i == FILE_LINES || diff.len() + line.len() > cap {
            cut = true;
            break;
        }
        diff.push_str(&line);
    }
    if !complete && !cut && !lines.is_empty() {
        diff.push_str("\\ No newline at end of file\n");
    }
    Ok((false, lines.len() as u64, diff, cut))
}

/// The local test step that last ran in this run: the command from the Activity Log, the
/// output from the step's result, and whether the files are still the ones it tested.
async fn tests(app: &App, uid: i64, r: &Run) -> Result<TestEvidence> {
    let steps: Vec<_> = r.workflow.nodes.iter().filter(|n| n.kind == "Connection" && n.provider == "local" && n.operation == "test").collect();
    if steps.is_empty() {
        return Ok(TestEvidence { state: "missing".into(), ..Default::default() });
    }
    let events = app.store.events(uid, &r.id, 0)?;
    let last = events.iter().rev().filter(|e| e.kind == "output").find_map(|e| {
        let command = e.text.strip_prefix(TEST_LINE)?;
        let step = steps.iter().find(|n| n.id == e.node)?;
        let out = r.outputs.get(&step.id).filter(|o| o.status == "completed" && r.states.get(&step.id).is_some_and(|s| s == "completed"))?;
        Some((step.id.clone(), command.to_owned(), out.summary.clone()))
    });
    let Some((node, command, output)) = last else { return Ok(TestEvidence { state: "not_run".into(), ..Default::default() }) };
    let proof = r.context.get(&format!("test-proof:{node}")).cloned();
    let workspace = PathBuf::from(&r.workspace);
    let current = crate::artifacts::blocking(move || Ok(proof.is_some_and(|proof| agents::workspace_fingerprint(&workspace).is_ok_and(|now| now == proof)))).await?;
    let total = output.chars().count();
    let output_cut = total > TEST_CHARS;
    let output = if output_cut { output.chars().skip(total - TEST_CHARS).collect() } else { output };
    Ok(TestEvidence { state: "ran".into(), node, command, output, output_cut, current })
}

#[cfg(test)]
mod tests {
    use crate::{App, engine, model::*};
    use std::path::{Path, PathBuf};

    fn git(dir: &Path, args: &[&str]) -> String {
        // No detached auto-maintenance after a commit: its lock file in .git can vanish while
        // the next run checks the Source tree, and the test would fail for that alone.
        let out = std::process::Command::new("git")
            .args(["-c", "user.name=test", "-c", "user.email=test@localhost", "-c", "commit.gpgSign=false", "-c", "core.hooksPath=/dev/null", "-c", "maintenance.auto=false"])
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).into_owned()
    }
    fn lines(text: String) -> Vec<String> {
        let mut lines: Vec<_> = text.lines().map(str::to_owned).collect();
        lines.sort();
        lines
    }
    fn write(dir: &Path, path: &str, text: &str) {
        std::fs::create_dir_all(dir.join(path).parent().unwrap()).unwrap();
        std::fs::write(dir.join(path), text).unwrap();
    }
    /// The SDD preset on the user's own Git project, used in place or as a copy.
    async fn project(a: &App, mode: &str, files: &[(&str, &str)], before_run: impl FnOnce(&Path)) -> (Run, Node, PathBuf) {
        project_at(a, mode, "project", files, before_run).await
    }
    /// `location` is the Source folder inside the repository created at `project`.
    async fn project_at(a: &App, mode: &str, location: &str, files: &[(&str, &str)], before_run: impl FnOnce(&Path)) -> (Run, Node, PathBuf) {
        let dir = a.sources.join("project");
        std::fs::create_dir_all(&dir).unwrap();
        for (path, text) in files {
            write(&dir, path, text);
        }
        if mode == "direct" {
            git(&dir, &["init", "-q", "-b", "main"]);
            git(&dir, &["add", "--all"]);
            git(&dir, &["commit", "-q", "-m", "base"]);
        }
        before_run(&dir);
        let mut w = crate::presets::all().remove(0);
        let source = w.nodes.iter_mut().find(|n| n.kind == "Source").unwrap();
        (source.source_type, source.source_mode, source.location, source.speckit) = ("folder".into(), mode.into(), location.into(), false);
        for task in w.nodes.iter_mut().filter(|n| n.kind == "Task") {
            task.command.clear();
        }
        let r = engine::create(a, 1, w, "commit".into(), None).await.unwrap();
        let commit = r.workflow.node("commit").unwrap().clone();
        let workspace = PathBuf::from(&r.workspace);
        (r, commit, workspace)
    }
    fn committed(dir: &Path) -> Vec<String> {
        lines(git(dir, &["show", "--name-only", "--format=", "HEAD"]))
    }

    #[tokio::test]
    async fn clean_workspace_commits_everything_the_run_changed() {
        let d = tempfile::tempdir().unwrap();
        let a = crate::tests::app(d.path());
        let files = [("README.md", "# Project\n"), ("old.txt", "old\n"), ("src/lib.rs", "pub fn a() {}\n")];
        let (r, commit, ws) = project(&a, "copy", &files, |_| {}).await;
        write(&ws, "src/lib.rs", "pub fn a() -> u8 { 1 }\n");
        write(&ws, "docs/new file.md", "spaces\n");
        write(&ws, "-dash.txt", "leading dash\n");
        std::fs::remove_file(ws.join("old.txt")).unwrap();
        let out = crate::connectors::execute(&a, 1, &r, &commit).await.unwrap();
        assert!(out.summary.starts_with("Commit locale verificato") && !out.summary.contains("lasciate fuori"), "{}", out.summary);
        assert_eq!(committed(&ws), ["-dash.txt", "docs/new file.md", "old.txt", "src/lib.rs"]);
        assert_eq!(git(&ws, &["status", "--porcelain", "--untracked-files=all"]), "");
        assert_eq!(git(&ws, &["log", "-1", "--format=%s"]).trim(), "SDD: verified workflow delivery");
    }

    #[tokio::test]
    async fn edits_made_before_the_run_stay_out_of_the_commit_as_they_were() {
        let d = tempfile::tempdir().unwrap();
        let a = crate::tests::app(d.path());
        let files = [("a.txt", "a\n"), ("b.txt", "b\n"), ("staged.txt", "s\n")];
        let (r, commit, ws) = project(&a, "direct", &files, |dir| {
            write(dir, "b.txt", "b\nuser edit\n");
            write(dir, "staged.txt", "s\nuser staged\n");
            git(dir, &["add", "staged.txt"]);
            write(dir, "staged.txt", "s\nuser staged\nand unstaged\n");
            write(dir, "notes/user notes.txt", "untracked\n");
        }).await;
        let base = git(&ws, &["rev-parse", "HEAD"]);
        write(&ws, "a.txt", "a\nrun edit\n");
        write(&ws, "new.txt", "run file\n");
        let out = crate::connectors::execute(&a, 1, &r, &commit).await.unwrap();
        // One sentence per line: the interface translates each line as a whole.
        let head = git(&ws, &["rev-parse", "HEAD"]);
        assert_eq!(out.summary.lines().collect::<Vec<_>>(), [
            format!("Commit locale verificato: {}.", head.trim()).as_str(),
            "File della run nel commit: a.txt, new.txt.",
            "Modifiche già presenti prima della run, lasciate fuori dal commit come erano: b.txt, notes/user notes.txt, staged.txt.",
            "Nessun push, merge o deploy eseguito.",
        ]);
        assert_eq!(git(&ws, &["rev-parse", "HEAD~1"]), base);
        assert_eq!(committed(&ws), ["a.txt", "new.txt"]);
        // The staged part is still staged, the unstaged parts are still unstaged and untracked.
        assert_eq!(git(&ws, &["diff", "--cached"]).lines().filter(|l| l.starts_with('+') && !l.starts_with("+++")).collect::<Vec<_>>(), ["+user staged"]);
        assert_eq!(lines(git(&ws, &["diff", "--name-only"])), ["b.txt", "staged.txt"]);
        assert_eq!(git(&ws, &["ls-files", "--others", "--exclude-standard"]), "notes/user notes.txt\n");
        assert_eq!(std::fs::read_to_string(ws.join("b.txt")).unwrap(), "b\nuser edit\n");
        assert_eq!(std::fs::read_to_string(ws.join("staged.txt")).unwrap(), "s\nuser staged\nand unstaged\n");
        // A second commit in the same run finds nothing new and still leaves the edits alone.
        let out = crate::connectors::execute(&a, 1, &r, &commit).await.unwrap();
        assert!(out.summary.lines().any(|l| l == "File della run nel commit: nessuno."), "{}", out.summary);
        assert_eq!(committed(&ws), ["a.txt", "new.txt"]);
    }

    #[tokio::test]
    async fn a_file_changed_before_and_during_the_run_stops_the_commit() {
        for (case, pre, during) in [
            ("separate lines", "1\nuser\n3\n4\n5\n", "1\nuser\n3\n4\nrun\n"),
            ("same line", "1\nuser\n3\n4\n5\n", "1\nrun\n3\n4\n5\n"),
        ] {
            for staged in [false, true] {
                let d = tempfile::tempdir().unwrap();
                let a = crate::tests::app(d.path());
                let files = [("shared.txt", "1\n2\n3\n4\n5\n"), ("other.txt", "o\n")];
                let (r, commit, ws) = project(&a, "direct", &files, |dir| {
                    write(dir, "shared.txt", pre);
                    if staged {
                        git(dir, &["add", "shared.txt"]);
                    }
                }).await;
                let (head, index) = (git(&ws, &["rev-parse", "HEAD"]), git(&ws, &["diff", "--cached"]));
                write(&ws, "shared.txt", during);
                write(&ws, "other.txt", "run\n");
                let error = crate::connectors::execute(&a, 1, &r, &commit).await.unwrap_err().to_string();
                assert!(error.starts_with("Commit locale fermato: questi file avevano già modifiche prima della run") && error.contains(": shared.txt. Controllali"), "{case}, staged {staged}: {error}");
                // Nothing was committed or staged: the user finds the folder as the run left it.
                assert_eq!(git(&ws, &["rev-parse", "HEAD"]), head);
                assert_eq!(git(&ws, &["diff", "--cached"]), index);
                assert_eq!(std::fs::read_to_string(ws.join("shared.txt")).unwrap(), during);
            }
        }
    }

    #[tokio::test]
    async fn untracked_files_from_before_the_run_are_watched_too() {
        let d = tempfile::tempdir().unwrap();
        let a = crate::tests::app(d.path());
        let (r, commit, ws) = project(&a, "direct", &[("a.txt", "a\n")], |dir| write(dir, "draft.txt", "user draft\n")).await;
        write(&ws, "draft.txt", "user draft\nrun addition\n");
        let error = crate::connectors::execute(&a, 1, &r, &commit).await.unwrap_err().to_string();
        assert!(error.contains(": draft.txt. Controllali"), "{error}");
        // A file the run deletes counts as changed as well.
        std::fs::remove_file(ws.join("draft.txt")).unwrap();
        let error = crate::connectors::execute(&a, 1, &r, &commit).await.unwrap_err().to_string();
        assert!(error.contains(": draft.txt. Controllali"), "{error}");
        assert_eq!(git(&ws, &["log", "--format=%s"]).trim(), "base");
    }

    #[tokio::test]
    async fn runs_without_a_recorded_start_do_not_guess() {
        let d = tempfile::tempdir().unwrap();
        let a = crate::tests::app(d.path());
        let (mut r, commit, ws) = project(&a, "direct", &[("a.txt", "a\n")], |_| {}).await;
        assert!(r.context.contains_key(super::START));
        r.context.remove(super::START);
        write(&ws, "a.txt", "run\n");
        let error = crate::connectors::execute(&a, 1, &r, &commit).await.unwrap_err().to_string();
        assert!(error.contains("lo stato Git del workspace all’avvio della run non è registrato"), "{error}");
        assert_eq!(git(&ws, &["log", "--format=%s"]).trim(), "base");
    }

    #[tokio::test]
    async fn copies_started_without_a_record_commit_everything_as_before() {
        let d = tempfile::tempdir().unwrap();
        let a = crate::tests::app(d.path());
        let (mut r, commit, ws) = project(&a, "copy", &[("a.txt", "a\n")], |_| {}).await;
        r.context.remove(super::START);
        write(&ws, "a.txt", "run\n");
        write(&ws, "new.txt", "run file\n");
        crate::connectors::execute(&a, 1, &r, &commit).await.unwrap();
        assert_eq!(committed(&ws), ["a.txt", "new.txt"]);
        // A copy path is not enough when the run says its workspace is the user's folder.
        r.context.insert("direct-primary".into(), "source".into());
        write(&ws, "a.txt", "again\n");
        let error = crate::connectors::execute(&a, 1, &r, &commit).await.unwrap_err().to_string();
        assert!(error.contains("non è registrato"), "{error}");
    }

    #[tokio::test]
    async fn a_source_inside_a_larger_repository_commits_only_its_own_folder() {
        let d = tempfile::tempdir().unwrap();
        let a = crate::tests::app(d.path());
        let files = [("app/a.txt", "a\n"), ("outside.txt", "o\n")];
        let (r, commit, ws) = project_at(&a, "direct", "project/app", &files, |dir| write(dir, "outside.txt", "user edit\n")).await;
        write(&ws, "a.txt", "run\n");
        // Not the run's folder: a direct run must not reach it, and the commit leaves it alone.
        write(&ws, "../elsewhere.txt", "not in the Source\n");
        let out = crate::connectors::execute(&a, 1, &r, &commit).await.unwrap();
        assert!(!out.summary.contains("outside.txt"), "{}", out.summary);
        assert_eq!(committed(&ws), ["app/a.txt"]);
        assert_eq!(lines(git(&ws, &["status", "--porcelain", "--untracked-files=all"])), [" M outside.txt", "?? elsewhere.txt"]);
    }

    #[tokio::test]
    async fn nested_repositories_created_by_the_run_stop_the_commit() {
        let d = tempfile::tempdir().unwrap();
        let a = crate::tests::app(d.path());
        let (r, commit, ws) = project(&a, "direct", &[("a.txt", "a\n")], |_| {}).await;
        write(&ws, "vendor/lib/x.txt", "x\n");
        git(&ws.join("vendor/lib"), &["init", "-q"]);
        write(&ws, "a.txt", "run\n");
        let error = crate::connectors::execute(&a, 1, &r, &commit).await.unwrap_err().to_string();
        assert!(error.starts_with("Commit locale fermato: la run ha cambiato sottomoduli") && error.contains(": vendor/lib/. Controllali"), "{error}");
        assert_eq!(git(&ws, &["log", "--format=%s"]).trim(), "base");
    }

    /// Every file under `.git`, with its content: any write to the index, objects or refs shows.
    fn repository(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        let (mut files, mut folders) = (vec![], vec![dir.join(".git")]);
        while let Some(folder) = folders.pop() {
            for entry in std::fs::read_dir(folder).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() { folders.push(path) } else { files.push((path.clone(), std::fs::read(&path).unwrap())) }
            }
        }
        files.sort();
        files
    }
    /// The run as it reaches the commit step after the quality gate.
    fn at_commit(a: &App, mut r: Run, mode: ExecutionMode, confirm: bool) -> Run {
        (r.current, r.last_node, r.status, r.execution.mode) = ("commit".into(), Some("quality".into()), "paused".into(), mode);
        r.workflow.nodes.iter_mut().find(|n| n.id == "commit").unwrap().confirm_commit = confirm;
        a.store.save_run(1, &r).unwrap();
        r
    }
    fn reviewer() -> User {
        User { id: 1, email: "reviewer@test.local".into() }
    }
    fn waiting(a: &App, r: &Run) -> super::Preview {
        let run = a.store.run(1, &r.id).unwrap();
        assert_eq!(run.status, "waiting_review");
        let pending = run.pending.unwrap();
        assert_eq!((pending.kind.as_str(), pending.node.as_str()), ("effect", "commit"));
        pending.commit.unwrap()
    }
    fn paths(preview: &super::Preview) -> Vec<&str> {
        preview.files.iter().map(|f| f.path.as_str()).collect()
    }

    #[tokio::test]
    async fn the_preview_shows_the_commit_and_writes_nothing_in_the_repository() {
        let d = tempfile::tempdir().unwrap();
        let a = crate::tests::app(d.path());
        let long: String = (0..600).map(|i| format!("old {i}\n")).collect();
        let files = [("a.txt", "1\n2\n3\n"), ("gone.txt", "bye\n"), ("kept.txt", "k\n"), ("staged.txt", "s\n"), ("long.txt", long.as_str())];
        let (r, _, ws) = project(&a, "direct", &files, |dir| write(dir, "kept.txt", "k\nuser edit\n")).await;
        write(&ws, "a.txt", "1\ntwo\n3\n");
        std::fs::remove_file(ws.join("gone.txt")).unwrap();
        write(&ws, "dir/new file.txt", "hello\nworld");
        std::fs::write(ws.join("blob.bin"), [0u8, 1, 2, 3]).unwrap();
        // Staged by the run: the commit takes the working tree, and so does the preview.
        write(&ws, "staged.txt", "s\nstaged\n");
        git(&ws, &["add", "staged.txt"]);
        write(&ws, "staged.txt", "s\nstaged\nand more\n");
        write(&ws, "long.txt", &(0..600).map(|i| format!("new {i}\n")).collect::<String>());
        write(&ws, "big.txt", &(0..1000).map(|i| format!("line {i}\n")).collect::<String>());
        let (status, head) = (git(&ws, &["status", "--porcelain=v2", "--untracked-files=all"]), git(&ws, &["rev-parse", "HEAD"]));
        // A hook that would run whenever an index is written, the temporary one included.
        let hook = ws.join(".git/hooks/post-index-change");
        std::fs::write(&hook, "#!/bin/sh\ntouch \"$(git rev-parse --git-dir)/hook-ran\"\n").unwrap();
        std::fs::set_permissions(&hook, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        let before = repository(&ws);
        let (preview, digest) = super::preview(&a, 1, &r).await.unwrap();
        // Index, objects, refs and HEAD are byte for byte as they were.
        assert!(repository(&ws) == before, "the preview wrote in .git");
        assert_eq!(git(&ws, &["status", "--porcelain=v2", "--untracked-files=all"]), status);
        assert_eq!(git(&ws, &["rev-parse", "HEAD"]), head);
        let tmp = crate::agents::user_home(&a, 1).unwrap().join("tmp");
        assert!(!std::fs::read_dir(tmp).unwrap().any(|e| e.unwrap().file_name().to_string_lossy().starts_with("commit-preview")));
        assert_eq!(paths(&preview), ["a.txt", "big.txt", "blob.bin", "dir/new file.txt", "gone.txt", "long.txt", "staged.txt"]);
        let file = |path: &str| preview.files.iter().find(|f| f.path == path).unwrap();
        let a_txt = file("a.txt");
        assert_eq!((a_txt.status.as_str(), a_txt.added, a_txt.removed, a_txt.cut), ("modified", Some(1), Some(1), false));
        assert!(a_txt.diff.starts_with("@@ -1,3 +1,3 @@\n") && a_txt.diff.contains("\n-2\n+two\n") && !a_txt.diff.contains("diff --git"), "{}", a_txt.diff);
        assert_eq!((file("gone.txt").status.as_str(), file("gone.txt").diff.contains("\n-bye\n")), ("deleted", true));
        assert_eq!(file("dir/new file.txt").diff, "@@ -0,0 +1,2 @@\n+hello\n+world\n\\ No newline at end of file\n");
        assert_eq!((file("blob.bin").status.as_str(), file("blob.bin").binary, file("blob.bin").diff.as_str()), ("added", true, ""));
        assert!(file("staged.txt").diff.contains("+staged\n+and more\n"), "{}", file("staged.txt").diff);
        // Long diffs stop at the line limit and say so; the counts still cover the whole file.
        let big = file("big.txt");
        assert_eq!((big.added, big.cut, big.diff.lines().count()), (Some(1000), true, 1 + super::FILE_LINES));
        let long = file("long.txt");
        assert_eq!((long.added, long.removed, long.cut, long.diff.lines().count()), (Some(600), Some(600), true, super::FILE_LINES));
        assert_eq!(preview.kept, ["kept.txt"]);
        assert_eq!(preview.test.state, "not_run");
        // The digest covers content: one more byte anywhere in the commit gives another one.
        write(&ws, "big.txt", "changed\n");
        assert_ne!(super::preview(&a, 1, &r).await.unwrap().1, digest);
    }

    #[tokio::test]
    async fn approving_commits_exactly_what_was_shown() {
        let d = tempfile::tempdir().unwrap();
        let a = crate::tests::app(d.path());
        let (r, _, ws) = project(&a, "direct", &[("a.txt", "a\n")], |dir| write(dir, "mine.txt", "user notes\n")).await;
        write(&ws, "a.txt", "a\nrun\n");
        write(&ws, "new.txt", "new\n");
        let r = at_commit(&a, r, ExecutionMode::Interactive, true);
        engine::drive(&a, 1, &r.id).await.unwrap();
        let preview = waiting(&a, &r);
        assert!(!preview.changed);
        assert_eq!((paths(&preview), preview.kept.clone()), (vec!["a.txt", "new.txt"], vec!["mine.txt".to_string()]));
        assert_eq!(git(&ws, &["log", "-1", "--format=%s"]).trim(), "base");
        // A restart interrupts running steps only: the request and its digest are kept.
        a.store.interrupt_running().unwrap();
        waiting(&a, &r);
        engine::decide(&a, &reviewer(), &r.id, "approve", "Checked the diff").unwrap();
        engine::drive(&a, 1, &r.id).await.unwrap();
        let done = a.store.run(1, &r.id).unwrap();
        assert_eq!(done.status, "completed");
        assert_eq!(committed(&ws), ["a.txt", "new.txt"]);
        assert_eq!(git(&ws, &["show", "HEAD:a.txt"]), "a\nrun\n");
        let report = &done.outputs["commit"].summary;
        assert!(report.contains(git(&ws, &["rev-parse", "HEAD"]).trim()) && !report.contains("senza conferma"), "{report}");
        assert_eq!(git(&ws, &["ls-files", "--others", "--exclude-standard"]), "mine.txt\n");
        let decisions = a.store.decisions(1, &r.id).unwrap();
        assert_eq!((decisions[0].actor.as_str(), decisions[0].decision.as_str(), decisions[0].node.as_str()), ("human", "approve", "commit"));
    }

    #[tokio::test]
    async fn a_change_after_approval_asks_again() {
        let d = tempfile::tempdir().unwrap();
        let a = crate::tests::app(d.path());
        let (r, _, ws) = project(&a, "direct", &[("a.txt", "a\n")], |_| {}).await;
        write(&ws, "a.txt", "approved\n");
        let r = at_commit(&a, r, ExecutionMode::Interactive, true);
        engine::drive(&a, 1, &r.id).await.unwrap();
        engine::decide(&a, &reviewer(), &r.id, "approve", "").unwrap();
        write(&ws, "a.txt", "changed after approval\n");
        engine::drive(&a, 1, &r.id).await.unwrap();
        let again = waiting(&a, &r);
        assert!(again.changed && again.files[0].diff.contains("+changed after approval"), "{again:?}");
        assert!(a.store.run(1, &r.id).unwrap().pending.unwrap().questions[0].starts_with("I file sono cambiati dopo l’approvazione"));
        assert_eq!(git(&ws, &["log", "-1", "--format=%s"]).trim(), "base");
        // The commit checks the digest itself too, for a change between the check and the commit.
        let run = a.store.run(1, &r.id).unwrap();
        assert!(super::commit(&a, 1, &run, Some("an older approval")).await.unwrap().is_none());
        assert_eq!(git(&ws, &["log", "-1", "--format=%s"]).trim(), "base");
        engine::decide(&a, &reviewer(), &r.id, "approve", "").unwrap();
        engine::drive(&a, 1, &r.id).await.unwrap();
        assert_eq!(a.store.run(1, &r.id).unwrap().status, "completed");
        assert_eq!(git(&ws, &["show", "HEAD:a.txt"]), "changed after approval\n");
    }

    #[tokio::test]
    async fn rejecting_fails_the_run_without_committing() {
        let d = tempfile::tempdir().unwrap();
        let a = crate::tests::app(d.path());
        let (r, _, ws) = project(&a, "direct", &[("a.txt", "a\n")], |_| {}).await;
        write(&ws, "a.txt", "run\n");
        let r = at_commit(&a, r, ExecutionMode::Interactive, true);
        engine::drive(&a, 1, &r.id).await.unwrap();
        let failed = engine::decide(&a, &reviewer(), &r.id, "reject", "Not this").unwrap();
        assert_eq!(failed.status, "failed");
        assert!(failed.error.unwrap().starts_with("Commit locale rifiutato: nessun commit creato"));
        assert_eq!(git(&ws, &["log", "-1", "--format=%s"]).trim(), "base");
        assert_eq!(std::fs::read_to_string(ws.join("a.txt")).unwrap(), "run\n");
    }

    #[tokio::test]
    async fn without_confirmation_the_commit_runs_as_before() {
        let d = tempfile::tempdir().unwrap();
        let a = crate::tests::app(d.path());
        let (r, _, ws) = project(&a, "direct", &[("a.txt", "a\n")], |_| {}).await;
        write(&ws, "a.txt", "run\n");
        let r = at_commit(&a, r, ExecutionMode::Interactive, false);
        engine::drive(&a, 1, &r.id).await.unwrap();
        let done = a.store.run(1, &r.id).unwrap();
        assert_eq!((done.status.as_str(), done.pending.is_none()), ("completed", true));
        assert_eq!(committed(&ws), ["a.txt"]);
        assert!(done.outputs["commit"].summary.starts_with("Commit locale verificato") && !done.outputs["commit"].summary.contains("conferma"));
        assert!(a.store.decisions(1, &r.id).unwrap().is_empty());
        // Workflows saved before the option have no field: they keep committing without a pause.
        let mut saved = serde_json::to_value(&r.workflow).unwrap();
        for node in saved["nodes"].as_array_mut().unwrap() {
            node.as_object_mut().unwrap().remove("confirm_commit");
        }
        let old: Workflow = serde_json::from_value(saved).unwrap();
        assert!(!old.node("commit").unwrap().confirm_commit);
        // New ones have it on: the presets and every commit step in the template catalog.
        assert!(crate::presets::all().iter().flat_map(|w| &w.nodes).filter(|n| n.operation == "commit").all(|n| n.confirm_commit));
        let catalog: Vec<_> = crate::presets::catalog().into_iter().flat_map(|t| t.workflow.nodes).filter(|n| n.operation == "commit").collect();
        assert!(catalog.len() == 2 && catalog.iter().all(crate::connectors::confirms_commit));
    }

    #[tokio::test]
    async fn unattended_runs_commit_without_waiting_and_say_so() {
        let d = tempfile::tempdir().unwrap();
        let a = crate::tests::app(d.path());
        let (r, _, ws) = project(&a, "direct", &[("a.txt", "a\n")], |_| {}).await;
        write(&ws, "a.txt", "run\n");
        let r = at_commit(&a, r, ExecutionMode::Unattended, true);
        engine::drive(&a, 1, &r.id).await.unwrap();
        let done = a.store.run(1, &r.id).unwrap();
        assert_eq!((done.status.as_str(), done.pending.is_none()), ("completed", true));
        assert_eq!(committed(&ws), ["a.txt"]);
        assert_eq!(done.outputs["commit"].summary.lines().last(), Some("Commit eseguito senza conferma: la run è in modalità non presidiata, quindi nessuno poteva approvarlo."), "{}", done.outputs["commit"].summary);
        assert!(!done.sessions.contains_key(crate::supervision::BINDING));
    }

    #[tokio::test]
    async fn the_safety_checks_stop_the_run_before_asking() {
        let d = tempfile::tempdir().unwrap();
        let a = crate::tests::app(d.path());
        let (r, _, ws) = project(&a, "direct", &[("shared.txt", "1\n"), ("b.txt", "b\n")], |dir| write(dir, "shared.txt", "1\nuser\n")).await;
        write(&ws, "shared.txt", "1\nuser\nrun\n");
        write(&ws, "b.txt", "run\n");
        let r = at_commit(&a, r, ExecutionMode::Interactive, true);
        let error = engine::drive(&a, 1, &r.id).await.unwrap_err().to_string();
        assert!(error.starts_with("Commit locale fermato: questi file avevano già modifiche prima della run") && error.contains(": shared.txt. Controllali"), "{error}");
        assert!(a.store.run(1, &r.id).unwrap().pending.is_none());
        assert_eq!(git(&ws, &["log", "-1", "--format=%s"]).trim(), "base");
        // A run without its start record stops the same way, before any approval is asked.
        let d = tempfile::tempdir().unwrap();
        let a = crate::tests::app(d.path());
        let (mut r, _, _) = project(&a, "direct", &[("a.txt", "a\n")], |_| {}).await;
        r.context.remove(super::START);
        let r = at_commit(&a, r, ExecutionMode::Interactive, true);
        let error = engine::drive(&a, 1, &r.id).await.unwrap_err().to_string();
        assert!(error.contains("lo stato Git del workspace all’avvio della run non è registrato"), "{error}");
    }

    #[tokio::test]
    async fn nothing_to_commit_needs_no_approval() {
        let d = tempfile::tempdir().unwrap();
        let a = crate::tests::app(d.path());
        let (r, _, _) = project(&a, "direct", &[("a.txt", "a\n")], |dir| write(dir, "mine.txt", "user\n")).await;
        let r = at_commit(&a, r, ExecutionMode::Interactive, true);
        engine::drive(&a, 1, &r.id).await.unwrap();
        let done = a.store.run(1, &r.id).unwrap();
        assert_eq!(done.status, "completed");
        assert!(done.outputs["commit"].summary.contains("nel commit: nessuno"));
    }

    #[tokio::test]
    async fn the_preview_carries_the_last_local_test() {
        let d = tempfile::tempdir().unwrap();
        let a = crate::tests::app(d.path());
        let (mut r, _, ws) = project(&a, "direct", &[("a.txt", "a\n")], |_| {}).await;
        write(&ws, "a.txt", "run\n");
        crate::agents::emit(&a, 1, &r.id, "output", "tests", "Comando di test: cargo test (rilevato da Cargo.toml)");
        r.outputs.insert("tests".into(), AgentResult { status: "completed".into(), decision: "pass".into(), summary: format!("{}test result: ok", "x".repeat(super::TEST_CHARS)), ..Default::default() });
        r.states.insert("tests".into(), "completed".into());
        r.context.insert("test-proof:tests".into(), crate::agents::workspace_fingerprint(&ws).unwrap());
        let test = super::preview(&a, 1, &r).await.unwrap().0.test;
        assert_eq!((test.state.as_str(), test.node.as_str(), test.command.as_str(), test.current), ("ran", "tests", "cargo test (rilevato da Cargo.toml)", true));
        assert!(test.output_cut && test.output.ends_with("test result: ok") && test.output.chars().count() == super::TEST_CHARS);
        write(&ws, "a.txt", "changed after the tests\n");
        assert!(!super::preview(&a, 1, &r).await.unwrap().0.test.current);
        r.workflow.nodes.retain(|n| n.operation != "test");
        assert_eq!(super::preview(&a, 1, &r).await.unwrap().0.test.state, "missing");
    }
}
