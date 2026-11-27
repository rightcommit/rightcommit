//! Which entries of a Source or run workspace the server leaves out when it walks the tree.
//!
//! Every walker reads entry types without following links (`symlink_metadata`,
//! `DirEntry::file_type`) and opens the files it reads with `O_NOFOLLOW`. These rules only
//! decide which entries are not walked at all. They apply at any depth (`packages/x/node_modules`,
//! `services/y/.venv`) and never to the root the walk starts from.
use std::ffi::OsStr;

/// Installed dependencies, caches and generated build folders that each run recreates in its own
/// workspace, as `npm install` does with `node_modules` and `python3 -m venv .venv` with a virtual
/// environment. Many hold symbolic links (`node_modules/.bin`, `.venv/bin/python3`, `.venv/lib64`)
/// and they are not project content: a copy leaves them out and does not count them toward its
/// limit, a permanent Source is not refused for the links inside them, and they are not exported,
/// indexed, part of the restart and test proofs, or captured as artifacts. A project that commits
/// one of them does not find it in a copied workspace; a permanent Source still has it.
///
/// Only names that by convention are generated, never committed, and mean nothing else. Generic
/// names that are often real sources (`build`, `dist`, `out`, `bin`, `obj`, `vendor`, `deps`,
/// `env`, `coverage`, `Pods`, `.yarn`, `.idea`, `.vscode`) stay out, and so does `.bundle`: Ruby
/// projects often commit `.bundle/config`.
///
/// Keep `artifactCacheDirectories` in web/app.js a superset of this list: it hides the files that
/// runs captured before these folders were skipped (a test checks it).
pub const DEPENDENCY_DIRS: &[&str] = &[
    // Python
    ".venv", "venv", "__pycache__", ".pytest_cache", ".tox", ".nox", ".mypy_cache", ".ruff_cache",
    ".ipynb_checkpoints",
    // JavaScript and TypeScript
    "node_modules", "bower_components", ".next", ".nuxt", ".svelte-kit", ".turbo", ".parcel-cache",
    ".angular", ".pnpm-store", ".nyc_output",
    // Java, Kotlin and Android
    ".gradle",
    // Elixir
    "_build",
    // Swift and Xcode
    ".build", "DerivedData",
    // Dart and Flutter
    ".dart_tool",
    // Terraform
    ".terraform",
    // Haskell
    ".stack-work", "dist-newstyle",
    // Zig
    "zig-cache", ".zig-cache", "zig-out",
    // C and C++
    "CMakeFiles", ".ccls-cache",
    // Several tools
    ".cache", ".direnv",
];

/// Git's repository data. Left out whatever its type: in a worktree or a submodule `.git` is a file.
/// The Artifacts view and a permanent Source's link check keep their own `.git` rules.
pub const GIT: &str = ".git";

/// Rust build output, rebuilt by every run: not copied, exported, indexed or part of the restart
/// and test proofs. Unlike `DEPENDENCY_DIRS` it is still captured for the Artifacts view, where a
/// built program can be a result (the view folds `target/debug` by default), and a permanent
/// Source is still checked inside it.
pub const BUILD: &str = "target";

/// A dependency or cache folder from `DEPENDENCY_DIRS`. A regular file with one of these names is
/// project content and is not skipped. A link with one of these names is skipped without being
/// followed: it is neither copied nor refused. Storage hides these folders in /sources and
/// examples, and the Project Manager does not suggest them as Source folders.
pub fn dependency_dir(name: &OsStr, is_file: bool) -> bool {
    !is_file && DEPENDENCY_DIRS.iter().any(|dir| name == *dir)
}

/// Left out of the copy of a Source or earlier workspace into a run workspace and of its size
/// check, of the restart proof, of the workspace ZIP export and Storage listing, and of the RAG
/// index: `.git`, `target` and `DEPENDENCY_DIRS`.
pub fn omitted(name: &OsStr, is_file: bool) -> bool {
    name == GIT || (!is_file && name == BUILD) || dependency_dir(name, is_file)
}

/// The same rule for the ZIP export script: names left out whatever their type, and names left
/// out unless they are regular files.
pub fn export_rules() -> serde_json::Value {
    let mut folders = vec![BUILD];
    folders.extend(DEPENDENCY_DIRS);
    serde_json::json!({"any": [GIT], "folders": folders})
}

/// The rule for the ZIP of a folder in /sources or examples: only `DEPENDENCY_DIRS`, whose links
/// would stop the export. `.git` and `target` there are the user's own and stay in the ZIP.
pub fn source_export_rules() -> serde_json::Value {
    serde_json::json!({"folders": DEPENDENCY_DIRS})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_named_like_folders_are_project_content() {
        for name in DEPENDENCY_DIRS {
            assert!(dependency_dir(OsStr::new(name), false));
            assert!(!dependency_dir(OsStr::new(name), true));
            assert!(omitted(OsStr::new(name), false));
            assert!(!omitted(OsStr::new(name), true));
        }
        assert!(omitted(OsStr::new(".git"), true) && omitted(OsStr::new(".git"), false));
        assert!(omitted(OsStr::new("target"), false) && !omitted(OsStr::new("target"), true));
        assert!(!dependency_dir(OsStr::new("target"), false) && !dependency_dir(OsStr::new(".git"), false));
        for name in ["src", "venvs", ".venv2", "node_modules.txt", "lib", "build", "dist", "out", "bin", "obj",
            "vendor", "deps", "env", "coverage", "Pods", ".yarn", ".idea", ".vscode", ".bundle"] {
            assert!(!omitted(OsStr::new(name), false), "{name}");
        }
    }

    #[test]
    fn the_artifacts_view_hides_every_dependency_folder() {
        let app = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/web/app.js")).unwrap();
        let line = app.lines().find(|l| l.starts_with("const artifactCacheDirectories=")).unwrap();
        for name in DEPENDENCY_DIRS {
            assert!(line.contains(&format!("'{name}'")), "web/app.js does not hide {name}");
        }
    }
}
