//! Notifications invalidate candidate hashes; metadata reconciliation is mandatory
//! at checkpoints, including Docker bind mounts that can silently lose events.
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::{Arc, Mutex},
};
#[derive(Default)]
pub(crate) struct Changes {
    pub paths: BTreeSet<PathBuf>,
    pub uncertain: bool,
}
pub(crate) struct Monitor {
    _watcher: RecommendedWatcher,
    changes: Arc<Mutex<Changes>>,
    roots: Vec<PathBuf>,
}
impl Monitor {
    pub fn start(roots: Vec<PathBuf>) -> anyhow::Result<Self> {
        let changes = Arc::new(Mutex::new(Changes::default()));
        let events = changes.clone();
        let watched = roots.clone();
        // Never follow links: on Linux the default would add watches through every link the
        // agent creates, such as .venv/lib64 or a link to /.
        let config = notify::Config::default().with_follow_symlinks(false);
        let mut watcher = RecommendedWatcher::new(move |result: notify::Result<Event>| {
            let mut changes = events.lock().unwrap();
            match result {
                Ok(event) if !matches!(event.kind, EventKind::Access(_)) => {
                    if event.need_rescan() || event.paths.is_empty() {
                        changes.uncertain = true;
                    }
                    for path in event.paths {
                        // Inside a root, .git and dependency folders (npm or pip installs make
                        // thousands of events) are never artifacts.
                        let inside = watched.iter().find_map(|root| path.strip_prefix(root).ok()).unwrap_or(&path);
                        let folders = inside.parent().unwrap_or(std::path::Path::new(""));
                        if !inside.components().any(|c| c.as_os_str() == ".git")
                            && !folders.components().any(|c| crate::project_tree::dependency_dir(c.as_os_str(), false))
                        {
                            changes.paths.insert(path);
                        }
                    }
                    // Bound event storms. Reconciliation will recover the state.
                    if changes.paths.len() > 20000 {
                        changes.paths.clear();
                        changes.uncertain = true;
                    }
                }
                Err(_) => changes.uncertain = true,
                _ => {}
            }
        }, config)?;
        for root in &roots {
            watcher.watch(root, RecursiveMode::Recursive)?;
        }
        Ok(Self {
            _watcher: watcher,
            changes,
            roots,
        })
    }
    pub fn matches(&self, roots: &[PathBuf]) -> bool {
        self.roots == roots
    }
    pub fn take(&self) -> Changes {
        std::mem::take(&mut *self.changes.lock().unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn receives_creation_and_rename_events() {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let monitor = Monitor::start(vec![root.clone()]).unwrap();
        std::fs::write(root.join("first.txt"), "content").unwrap();
        std::fs::rename(root.join("first.txt"), root.join("second.txt")).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let changes = monitor.take();
            if changes.uncertain || changes.paths.iter().any(|p| p.starts_with(&root)) {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "No native events received"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }
}
