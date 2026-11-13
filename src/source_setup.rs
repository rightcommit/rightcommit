//! Optional preparation of a Source folder before the first node runs: rightcommit's
//! Spec Kit files, then the Source's own setup command (for OpenSpec or any other tool).
use crate::{App, agents, model::Node};
use anyhow::{Result, bail};
use std::path::{Path, PathBuf};

/// The Spec Kit files inside the bundled helpdesk example, for Claude Code, Codex and Pi.
const SPECKIT: &[&str] = &[
    ".specify/",
    ".claude/skills/speckit-",
    ".agents/skills/speckit-",
    ".pi/prompts/speckit.",
];
pub const SPECKIT_VERSION: &str = "1.2.2";

/// Copies the bundled Spec Kit into `folder` when it has no `.specify`, keeping every
/// existing file. Returns the number of files added.
pub fn add_speckit(assets: &Path, folder: &Path) -> Result<usize> {
    if folder.join(".specify").symlink_metadata().is_ok() {
        return Ok(0);
    }
    let example = assets.join("examples/helpdesk");
    let mut added = 0;
    for file in files(&example)? {
        let relative = file.strip_prefix(&example)?;
        if !SPECKIT.iter().any(|prefix| relative.to_string_lossy().starts_with(prefix)) {
            continue;
        }
        // The folder can come from any Git repository: never write through one of its links.
        let mut parent = folder.to_path_buf();
        for part in relative.parent().into_iter().flat_map(Path::components) {
            parent.push(part);
            match parent.symlink_metadata() {
                Ok(meta) if meta.is_dir() => {}
                Ok(_) => bail!("Spec Kit non aggiunto: {} non è una cartella", parent.display()),
                Err(_) => std::fs::create_dir(&parent)?,
            }
        }
        let target = folder.join(relative);
        if target.symlink_metadata().is_err() {
            std::fs::copy(&file, &target)?;
            added += 1;
        }
    }
    Ok(added)
}

fn files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut found = vec![];
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() {
            found.extend(files(&entry.path())?);
        } else if kind.is_file() {
            found.push(entry.path());
        }
    }
    Ok(found)
}

/// Prepares one Source folder as configured on its node. Returns lines for the Activity Log.
pub async fn prepare(app: &App, uid: i64, node: &Node, folder: &Path, run_command: bool) -> Result<Vec<String>> {
    let mut log = vec![];
    if node.speckit {
        let (assets, target) = (app.assets.clone(), folder.to_path_buf());
        let added = crate::artifacts::blocking(move || add_speckit(&assets, &target)).await?;
        if added > 0 {
            agents::own_tree_async(app, uid, folder).await?;
            log.push(format!("Spec Kit {SPECKIT_VERSION} aggiunto al Source {}: {added} file", node.title));
        } else {
            log.push(format!("Spec Kit già presente nel Source {}", node.title));
        }
    }
    if run_command && !node.setup_command.trim().is_empty() {
        let argv = crate::test_command::split_labeled("Il comando di preparazione", &node.setup_command)?;
        let shown = shlex::try_join(argv.iter().map(String::as_str)).unwrap_or_else(|_| argv.join(" "));
        let output = agents::project_tool(app, uid, folder, &argv, 600)
            .await
            .map_err(|e| anyhow::anyhow!("Source {}: comando di preparazione non riuscito: {e}", node.title))?;
        let output = output.trim();
        log.push(if output.is_empty() {
            format!("Comando di preparazione del Source {}: {shown}", node.title)
        } else {
            format!("Comando di preparazione del Source {}: {shown}\n{output}", node.title)
        });
    }
    Ok(log)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assets() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
    }

    #[test]
    fn speckit_is_added_for_every_agent_without_touching_project_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".claude/skills/mine")).unwrap();
        std::fs::write(dir.path().join(".claude/skills/mine/SKILL.md"), "mine").unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        let added = add_speckit(&assets(), dir.path()).unwrap();
        assert!(added > 40, "{added}");
        for runtime in ["claude", "codex", "pi"] {
            crate::agents::resolve_command("specify", runtime, dir.path()).unwrap();
        }
        assert_eq!(std::fs::read_to_string(dir.path().join(".claude/skills/mine/SKILL.md")).unwrap(), "mine");
        // Only Spec Kit: the example's own code and settings stay out.
        assert!(!dir.path().join("src").exists() && !dir.path().join("incident.json").exists());
        assert_eq!(std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap(), "[package]");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let script = dir.path().join(".specify/scripts/bash/setup-plan.sh");
            assert_ne!(std::fs::metadata(script).unwrap().permissions().mode() & 0o111, 0);
        }
    }

    #[test]
    fn existing_speckit_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".specify")).unwrap();
        assert_eq!(add_speckit(&assets(), dir.path()).unwrap(), 0);
        assert!(!dir.path().join(".claude").exists());
    }

    #[cfg(unix)]
    #[test]
    fn linked_folders_are_not_written_through() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join(".claude")).unwrap();
        assert!(add_speckit(&assets(), dir.path()).is_err());
        assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
    }
}
