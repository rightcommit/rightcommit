//! `rightcommit resetdb`: brings a data directory back to the state of a first installation, so the
//! next start asks for the administrator account again. What `rightcommit setup` installed is kept:
//! reinstalling the agents is slow and they hold no user data.
use anyhow::{Context, Result, bail, ensure};
use rusqlite::{Connection, ErrorCode, OpenFlags};
use std::{
    io::{BufRead, Write},
    path::{Path, PathBuf},
};

/// Entries of system/ that survive: the agents and tools installed by setup, the bundled resources
/// with the Pi extensions, downloaded RAG models (caches without account data) and earlier backups.
const KEEP: &[&str] = &["assets", "tools", "rag-models", "backups"];
/// Moved into the backup with the database: without the key its secrets cannot be read, and the
/// template catalog is administrator content that lives outside the database.
const BACKED_UP: &[&str] = &["master.key", "workflow-templates.json", "template-history"];
const DATABASE: &str = "nodo.sqlite";
const WORD: &str = "resetdb";

#[derive(Default)]
struct Options {
    yes: bool,
    no_backup: bool,
    terminal: bool,
}

pub fn main(args: &[String]) -> Result<()> {
    let mut options = Options::default();
    for arg in args {
        match arg.as_str() {
            "--yes" => options.yes = true,
            "--no-backup" => options.no_backup = true,
            _ => bail!("Argomento sconosciuto: {arg}; uso: rightcommit resetdb [--yes] [--no-backup]"),
        }
    }
    let data = PathBuf::from(std::env::var("NODO_DATA").unwrap_or_else(|_| "data".into()));
    // Never create it: a wrong start folder or NODO_DATA must not pass for an empty installation.
    let data = std::fs::canonicalize(&data).with_context(|| format!("Cartella dati non trovata: {}. Avvia il comando dalla cartella in cui avvii rightcommit, oppure imposta NODO_DATA.", data.display()))?;
    // Same resolution as the server, without creating the folder.
    let sources = std::env::var_os("NODO_SOURCES").map(PathBuf::from).unwrap_or_else(|| if Path::new("/sources").is_dir() { "/sources".into() } else { "sources".into() });
    use std::io::IsTerminal;
    options.terminal = std::io::stdin().is_terminal();
    reset(&data, &sources, &options, &mut std::io::stdin().lock(), &mut std::io::stdout())
}

/// `data` is canonical, as main resolves it, so that it compares with the canonical sources path.
fn reset(data: &Path, sources: &Path, options: &Options, input: &mut dyn BufRead, out: &mut dyn Write) -> Result<()> {
    let system = data.join("system");
    ensure!(system.is_dir(), "{} non è una cartella dati di rightcommit: manca system. Nessun file è stato modificato.", data.display());
    // Held until the end, so a server cannot start between the question and the reset.
    let mut db = match system.join(DATABASE).exists() { true => Some(lock(&system.join(DATABASE))?), false => None };
    let tables = match &db { Some(db) => tables(db)?, None => Vec::new() };
    let mut removed = Vec::new();
    if data.join("users").exists() { removed.push(data.join("users")); }
    for entry in std::fs::read_dir(&system)? {
        let name = entry?.file_name();
        let name = name.to_string_lossy();
        if !KEEP.contains(&name.as_ref()) && !name.starts_with(DATABASE) { removed.push(system.join(name.as_ref())); }
    }
    removed.sort();
    if tables.is_empty() && removed.is_empty() {
        writeln!(out, "Niente da azzerare in {}: è già come una prima installazione.", data.display())?;
        return Ok(());
    }
    if let Ok(sources) = std::fs::canonicalize(sources) {
        for path in &removed {
            ensure!(!sources.starts_with(path), "La cartella dei progetti {} si trova dentro {}: spostala prima di azzerare. Nessun file è stato modificato.", sources.display(), path.display());
        }
    }
    let stamp = jiff::Timestamp::now().strftime("%Y%m%dT%H%M%SZ").to_string();
    let backup = (!options.no_backup && !tables.is_empty()).then(|| system.join("backups").join(format!("resetdb-{stamp}")));
    out.write_all(describe(data, !tables.is_empty(), &removed, backup.as_deref()).as_bytes())?;
    if !options.yes {
        ensure!(options.terminal, "Conferma mancante: per eseguire senza terminale aggiungi --yes. Nessun file è stato modificato.");
        write!(out, "Scrivi {WORD} per confermare: ")?;
        out.flush()?;
        let mut answer = String::new();
        input.read_line(&mut answer)?;
        ensure!(answer.trim() == WORD, "Annullato. Nessun file è stato modificato.");
    }
    if let (Some(dir), Some(db)) = (&backup, &db) {
        std::fs::create_dir_all(system.join("backups"))?;
        crate::store::private(&system.join("backups"), true)?;
        std::fs::create_dir(dir)?;
        crate::store::private(dir, true)?;
        // A consistent copy that includes what is still only in the WAL file.
        db.execute("VACUUM INTO ?1", [dir.join(DATABASE).to_str().context("Percorso della copia non valido")?])?;
        crate::store::private(&dir.join(DATABASE), false)?;
    }
    // Everything leaves its place with a rename before the database is emptied: a folder that cannot
    // be deleted must never stay where a new user with the same id would find it.
    let trash = system.join(format!("resetdb-{stamp}.trash"));
    std::fs::create_dir(&trash)?;
    crate::store::private(&trash, true)?;
    let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
    let result = (|| -> Result<()> {
        for path in &removed {
            let name = path.file_name().context("Percorso non valido")?;
            let dest = match &backup {
                Some(dir) if path.parent() == Some(system.as_path()) && BACKED_UP.iter().any(|b| name == *b) => dir.join(name),
                _ => trash.join(name),
            };
            std::fs::rename(path, &dest).with_context(|| format!("Impossibile spostare {}", path.display()))?;
            moved.push((path.clone(), dest));
        }
        if let Some(db) = &mut db { empty(db, &tables)?; }
        Ok(())
    })();
    if let Err(error) = result {
        for (original, staged) in moved.iter().rev() {
            std::fs::rename(staged, original)?;
        }
        let _ = std::fs::remove_dir(&trash);
        return Err(error.context("Azzeramento non eseguito; i file sono stati rimessi al loro posto"));
    }
    // Free the database first: from here the folder is a first installation and may be started.
    if let Some(db) = db.take() {
        db.execute_batch("VACUUM")?;
        db.close().map_err(|(_, e)| e)?;
    }
    writeln!(out, "Azzeramento completato. Al prossimo avvio il primo accesso crea l'account amministratore.")?;
    if let Some(dir) = &backup { writeln!(out, "Copia di sicurezza: {}", dir.display())?; }
    if let Err(error) = remove(&trash) {
        writeln!(out, "Spazio non ancora liberato: elimina a mano {} ({error}).", trash.display())?;
    }
    Ok(())
}

/// The server keeps a shared lock on the database file for as long as it runs (SQLite WAL mode).
/// An exclusive lock therefore fails while a server, or another resetdb, uses this data directory.
fn lock(path: &Path) -> Result<Connection> {
    let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
    db.busy_timeout(std::time::Duration::ZERO)?;
    match db.execute_batch("PRAGMA locking_mode=EXCLUSIVE; BEGIN EXCLUSIVE; COMMIT;") {
        Err(rusqlite::Error::SqliteFailure(e, _)) if matches!(e.code, ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked) => {
            bail!("Il database è in uso: arresta il server rightcommit che usa questa cartella dati (Ctrl+C nel suo terminale, oppure docker compose stop) e riprova. Nessun file è stato modificato.")
        }
        result => result.context("Database non leggibile. Nessun file è stato modificato.")?,
    }
    Ok(db)
}

fn tables(db: &Connection) -> Result<Vec<String>> {
    let mut q = db.prepare("SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")?;
    Ok(q.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?)
}

/// Emptied in place rather than deleted: the lock stays on the same file until the end, and SQLite
/// keeps its WAL consistent. Store::open recreates the tables at the next start.
fn empty(db: &mut Connection, tables: &[String]) -> Result<()> {
    let tx = db.transaction()?;
    for table in tables {
        tx.execute_batch(&format!("DROP TABLE \"{}\"", table.replace('"', "\"\"")))?;
    }
    // A first installation numbers users from 1 again.
    if tx.query_row("SELECT count(*) FROM sqlite_schema WHERE name='sqlite_sequence'", [], |r| r.get::<_, i64>(0))? > 0 {
        tx.execute("DELETE FROM sqlite_sequence", [])?;
    }
    tx.commit()?;
    Ok(())
}

fn describe(data: &Path, database: bool, removed: &[PathBuf], backup: Option<&Path>) -> String {
    let mut text = format!("Azzeramento dei dati di rightcommit, come in una prima installazione.\nCartella dati: {}\n\nVerranno eliminati:\n", data.display());
    if database {
        text += "  - system/nodo.sqlite: account, impostazioni e credenziali salvate, workflow, esecuzioni, pianificazioni, chat del Project Manager, artefatti e storico (il database viene svuotato)\n";
    }
    for path in removed {
        let name = path.strip_prefix(data).unwrap_or(path).display().to_string();
        let what = match name.as_str() {
            "users" => "home degli utenti con le credenziali salvate degli agenti, workspace delle esecuzioni e spazi del Project Manager",
            "system/master.key" => "chiave di cifratura delle credenziali; al prossimo avvio ne viene creata una nuova",
            "system/workflow-templates.json" => "catalogo dei template di workflow; al prossimo avvio torna quello predefinito",
            "system/template-history" => "versioni precedenti del catalogo dei template",
            "system/settings-history" => "versioni precedenti delle impostazioni",
            "system/a2a-history" => "versioni precedenti delle connessioni A2A",
            "system/artifact-files" => "copie dei file degli artefatti",
            "system/rag" => "indici RAG",
            "system/itsm-items" | "system/servicenow-effects" => "operazioni ServiceNow registrate",
            "system/deletions" => "file di workflow eliminati in attesa di pulizia",
            _ => "dati di rightcommit",
        };
        text += &format!("  - {name}: {what}\n");
    }
    text += "\nRestano: agenti e strumenti installati da rightcommit setup (system/tools), risorse ed estensioni Pi (system/assets), modelli RAG scaricati (system/rag-models), copie di sicurezza precedenti (system/backups), i progetti in sources e il file .env.\n";
    text += &match backup {
        Some(dir) => {
            let saved: String = removed.iter().filter_map(|p| p.strip_prefix(data.join("system")).ok()?.to_str()).filter(|n| BACKED_UP.contains(n)).map(|n| format!(", {n}")).collect();
            format!("Copia di sicurezza: {} con {DATABASE}{saved}. Contiene le credenziali cifrate e la chiave per leggerle; per non crearla usa --no-backup.\n", dir.display())
        }
        None if database => "Nessuna copia di sicurezza (--no-backup): l'operazione non si può annullare.\n".into(),
        None => String::new(),
    };
    text += "Il server non deve essere in esecuzione su questa cartella dati: fino al termine non può avviarsi.\n\n";
    text
}

/// Toolchains such as Go leave read-only folders in an agent's home; make them writable and retry.
fn remove(path: &Path) -> std::io::Result<()> {
    fn writable(dir: &Path) -> std::io::Result<()> {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() { writable(&entry.path())?; }
        }
        Ok(())
    }
    std::fs::remove_dir_all(path).or_else(|_| { writable(path)?; std::fs::remove_dir_all(path) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Settings, Workflow};

    fn yes() -> Options { Options { yes: true, ..Default::default() } }
    fn run(data: &Path, options: &Options, answer: &str) -> Result<String> {
        let mut out = Vec::new();
        reset(data, &data.join("sources"), options, &mut answer.as_bytes(), &mut out)?;
        Ok(String::from_utf8(out).unwrap())
    }
    /// An installation that has been used: one user with settings, a workflow, a run, agent
    /// credentials in the home, plus the tools installed by setup.
    async fn used(data: &Path) {
        let app = crate::tests::app(data);
        app.store.db.lock().unwrap().execute("INSERT INTO users(email,password) VALUES('old@example.com','x')", []).unwrap();
        let settings = Settings { default_runtime: "claude".into(), ..Default::default() };
        app.store.save_settings(1, &settings).unwrap();
        let workflow: Workflow = crate::presets::all().remove(0);
        app.store.save_workflow(1, &workflow).unwrap();
        let run = crate::engine::create(&app, 1, workflow, "resetdb test".into(), None).await.unwrap();
        assert!(Path::new(&run.workspace).starts_with(data.join("users/1")));
        let home = crate::agents::user_home(&app, 1).unwrap();
        std::fs::write(home.join(".codex/auth.json"), "old-user-secret").unwrap();
        // Read-only like a Go module cache.
        std::fs::create_dir_all(home.join("go/pkg/mod/m")).unwrap();
        std::fs::write(home.join("go/pkg/mod/m/f"), "x").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(home.join("go/pkg/mod/m"), std::fs::Permissions::from_mode(0o555)).unwrap();
        for dir in ["system/tools/bin", "system/assets/bundle/pi-extensions/node_modules", "system/settings-history/1"] {
            std::fs::create_dir_all(data.join(dir)).unwrap();
        }
        std::fs::write(data.join("system/tools/bin/codex"), "#!/bin/sh").unwrap();
        std::fs::write(data.join("system/settings-history/1/old.enc"), "old").unwrap();
        std::fs::write(data.join("system/workflow-templates.json"), "{}").unwrap();
        assert_eq!(app.store.workflows(1).unwrap().len(), 1);
    }
    fn users(data: &Path) -> i64 {
        crate::tests::app(data).store.db.lock().unwrap().query_row("SELECT count(*) FROM users", [], |r| r.get(0)).unwrap()
    }

    #[tokio::test]
    async fn reset_restores_a_first_installation_and_keeps_setup_tools() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path();
        used(data).await;
        let old_key = std::fs::read(data.join("system/master.key")).unwrap();
        let text = run(data, &yes(), "").unwrap();
        assert!(text.contains("users: home degli utenti") && text.contains("Azzeramento completato"), "{text}");
        // A second reset finds nothing to do and needs no confirmation.
        assert!(run(data, &Options::default(), "").unwrap().contains("Niente da azzerare"));
        // VACUUM rewrote the file: the old rows are not left behind in free pages.
        for file in ["nodo.sqlite", "nodo.sqlite-wal"] {
            let bytes = std::fs::read(data.join("system").join(file)).unwrap_or_default();
            assert!(!bytes.windows(15).any(|w| w == b"old@example.com"), "{file}");
        }
        let app = crate::tests::app(data);
        // What /api/auth/status reports as setup_required.
        assert_eq!(app.store.db.lock().unwrap().query_row("SELECT count(*) FROM users", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
        assert!(app.store.workflows(1).unwrap().is_empty() && app.store.runs(1).unwrap().is_empty());
        assert_eq!(app.store.settings(1).unwrap().default_runtime, Settings::default().default_runtime);
        assert_ne!(std::fs::read(data.join("system/master.key")).unwrap(), old_key);
        assert!(data.join("system/tools/bin/codex").is_file());
        assert!(data.join("system/assets/bundle/pi-extensions/node_modules").is_dir());
        assert!(!data.join("system/settings-history").exists() && !data.join("system/workflow-templates.json").exists());
        assert!(!std::fs::read_dir(data.join("system")).unwrap().any(|e| e.unwrap().file_name().to_string_lossy().ends_with(".trash")));
        // The next first user gets id 1 again and an empty home, not the old user's credentials.
        let uid = app.store.db.lock().unwrap().query_row("INSERT INTO users(email,password) VALUES('new@example.com','x') RETURNING id", [], |r| r.get::<_, i64>(0)).unwrap();
        assert_eq!(uid, 1);
        let home = crate::agents::user_home(&app, uid).unwrap();
        assert!(!home.join(".codex/auth.json").exists() && !home.join("go").exists());
        // The backup holds the old database and the key to read its secrets.
        let backups: Vec<_> = std::fs::read_dir(data.join("system/backups")).unwrap().map(|e| e.unwrap().path()).collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(std::fs::read(backups[0].join("master.key")).unwrap(), old_key);
        assert!(backups[0].join("workflow-templates.json").is_file());
        let old = Connection::open(backups[0].join(DATABASE)).unwrap();
        assert_eq!(old.query_row("SELECT email FROM users", [], |r| r.get::<_, String>(0)).unwrap(), "old@example.com");
    }

    #[tokio::test]
    async fn reset_without_backup_leaves_no_copy() {
        let dir = tempfile::tempdir().unwrap();
        used(dir.path()).await;
        run(dir.path(), &Options { yes: true, no_backup: true, ..Default::default() }, "").unwrap();
        assert!(!dir.path().join("system/backups").exists());
        assert_eq!(users(dir.path()), 0);
    }

    #[tokio::test]
    async fn nothing_changes_without_confirmation() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path();
        used(data).await;
        let snapshot = || (users(data), data.join("users/1/.codex/auth.json").is_file(), data.join("system/master.key").is_file(), data.join("system/backups").exists());
        let before = snapshot();
        assert_eq!(before, (1, true, true, false));
        // Not a terminal and no --yes: refused before any question.
        let error = run(data, &Options::default(), "resetdb\n").unwrap_err().to_string();
        assert!(error.contains("--yes"), "{error}");
        assert_eq!(snapshot(), before);
        // A terminal: Enter or another word cancels.
        for answer in ["\n", "y\n", "RESETDB\n", ""] {
            let error = run(data, &Options { terminal: true, ..Default::default() }, answer).unwrap_err().to_string();
            assert!(error.contains("Annullato"), "{error}");
            assert_eq!(snapshot(), before);
        }
        assert!(run(data, &Options { terminal: true, ..Default::default() }, "resetdb\n").unwrap().contains("Scrivi resetdb per confermare"));
        assert_eq!(users(data), 0);
    }

    #[tokio::test]
    async fn refuses_while_a_server_holds_the_database() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path();
        used(data).await;
        let server = crate::tests::app(data);
        let error = run(data, &yes(), "").unwrap_err().to_string();
        assert!(error.contains("arresta il server"), "{error}");
        assert!(data.join("users/1/.codex/auth.json").is_file() && !data.join("system/backups").exists());
        assert_eq!(server.store.workflows(1).unwrap().len(), 1);
        drop(server);
        run(data, &yes(), "").unwrap();
    }

    #[tokio::test]
    async fn refuses_a_folder_that_is_not_a_data_directory_and_protects_sources() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("users/1")).unwrap();
        assert!(run(dir.path(), &yes(), "").unwrap_err().to_string().contains("manca system"));
        assert!(dir.path().join("users/1").is_dir());
        used(dir.path()).await;
        let data = std::fs::canonicalize(dir.path()).unwrap();
        let error = reset(&data, &data.join("users/1"), &yes(), &mut "".as_bytes(), &mut Vec::new()).unwrap_err().to_string();
        assert!(error.contains("cartella dei progetti"), "{error}");
        assert_eq!(users(dir.path()), 1);
    }
}
