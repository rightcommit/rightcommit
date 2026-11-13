//! Command for the local test Connection: the one set on the node, or one detected from
//! the project files in the run workspace. The command runs without a shell.
use crate::model::Node;
use anyhow::{Result, bail, ensure};
use std::path::Path;

#[derive(Debug, PartialEq, Eq)]
pub enum Origin {
    Configured,
    /// The file in the workspace root that selected the command.
    Detected(&'static str),
}

#[derive(Debug, PartialEq, Eq)]
pub struct TestCommand {
    pub argv: Vec<String>,
    pub origin: Origin,
}

impl TestCommand {
    /// The command as it would be typed, for logs and reports.
    pub fn display(&self) -> String {
        shlex::try_join(self.argv.iter().map(String::as_str)).unwrap_or_else(|_| self.argv.join(" "))
    }
    pub fn origin_text(&self) -> String {
        match self.origin {
            Origin::Configured => "configurato nel nodo".into(),
            Origin::Detected(file) => format!("rilevato da {file}"),
        }
    }
}

/// Order matters: a workspace with both Cargo.toml and package.json runs cargo test.
const DETECTION: &[(&[&str], &[&str])] = &[
    (&["Cargo.toml"], &["cargo", "test"]),
    (&["package.json"], &["npm", "test"]),
    (&["pyproject.toml", "pytest.ini", "setup.py"], &["python3", "-m", "pytest"]),
    (&["go.mod"], &["go", "test", "./..."]),
    (&["pom.xml"], &["mvn", "-q", "test"]),
    (&["build.gradle", "build.gradle.kts"], &["gradle", "test"]),
];
const MAKEFILES: &[&str] = &["GNUmakefile", "makefile", "Makefile"];

pub fn resolve(node: &Node, workspace: &Path) -> Result<TestCommand> {
    if !node.test_command.trim().is_empty() {
        return Ok(TestCommand { argv: split(&node.test_command)?, origin: Origin::Configured });
    }
    detect(workspace)
}

pub fn split(command: &str) -> Result<Vec<String>> {
    ensure!(!command.trim().is_empty(), "Il comando di test è vuoto");
    ensure!(command.len() <= 4096, "Il comando di test supera 4096 caratteri");
    ensure!(!command.contains(['\n', '\r', '\0']), "Il comando di test deve stare su una riga");
    let Some(argv) = shlex::split(command) else {
        bail!("Comando di test non valido: controlla virgolette e caratteri di escape");
    };
    ensure!(!argv.is_empty(), "Il comando di test è vuoto");
    Ok(argv)
}

pub fn detect(workspace: &Path) -> Result<TestCommand> {
    // Compare real entry names: on case-insensitive file systems `makefile` would also open `Makefile`.
    let files: std::collections::HashSet<String> = std::fs::read_dir(workspace)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    let file = |name: &str| files.contains(name);
    for (files, argv) in DETECTION {
        if let Some(found) = files.iter().find(|name| file(name)) {
            return Ok(TestCommand {
                argv: argv.iter().map(|s| s.to_string()).collect(),
                origin: Origin::Detected(found),
            });
        }
    }
    // make reads GNUmakefile, then makefile, then Makefile.
    if let Some(found) = MAKEFILES.iter().find(|name| file(name))
        && std::fs::read_to_string(workspace.join(found)).is_ok_and(|text| has_test_target(&text))
    {
        return Ok(TestCommand { argv: vec!["make".into(), "test".into()], origin: Origin::Detected(found) });
    }
    bail!(
        "Nessun comando di test riconosciuto nella cartella principale del workspace \
         (Cargo.toml, package.json, pyproject.toml, pytest.ini, setup.py, go.mod, pom.xml, \
         build.gradle, Makefile con target test). Imposta il comando di test nel nodo Connection."
    )
}

/// A rule whose targets include `test`, such as `test:` or `check test: build`.
fn has_test_target(makefile: &str) -> bool {
    makefile.lines().any(|line| {
        if line.starts_with(['\t', ' ', '#', '.']) {
            return false;
        }
        let Some((targets, rest)) = line.split_once(':') else { return false };
        // `test := value` and `test ::= value` are variable assignments, not rules.
        !rest.starts_with('=') && !rest.starts_with(":=")
            && targets.split_whitespace().any(|target| target == "test")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (name, text) in files {
            std::fs::write(dir.path().join(name), text).unwrap();
        }
        dir
    }
    fn detected(files: &[(&str, &str)]) -> Result<TestCommand> {
        detect(workspace(files).path())
    }
    fn argv(words: &[&str]) -> Vec<String> {
        words.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn cargo_wins_over_package_json() {
        let found = detected(&[("package.json", "{}"), ("Cargo.toml", "")]).unwrap();
        assert_eq!(found, TestCommand { argv: argv(&["cargo", "test"]), origin: Origin::Detected("Cargo.toml") });
    }

    #[test]
    fn each_ecosystem_is_detected() {
        for (file, expected) in [
            ("Cargo.toml", &["cargo", "test"][..]),
            ("package.json", &["npm", "test"]),
            ("pyproject.toml", &["python3", "-m", "pytest"]),
            ("pytest.ini", &["python3", "-m", "pytest"]),
            ("setup.py", &["python3", "-m", "pytest"]),
            ("go.mod", &["go", "test", "./..."]),
            ("pom.xml", &["mvn", "-q", "test"]),
            ("build.gradle", &["gradle", "test"]),
            ("build.gradle.kts", &["gradle", "test"]),
        ] {
            let found = detected(&[(file, "")]).unwrap();
            assert_eq!(found.argv, argv(expected), "{file}");
            assert_eq!(found.origin, Origin::Detected(file));
        }
    }

    #[test]
    fn makefile_needs_a_test_target() {
        let found = detected(&[("Makefile", ".PHONY: test\nbuild:\n\tcc main.c\ntest: build\n\t./run\n")]).unwrap();
        assert_eq!(found, TestCommand { argv: argv(&["make", "test"]), origin: Origin::Detected("Makefile") });
        assert_eq!(detected(&[("Makefile", "check test:\n\t./run\n")]).unwrap().argv, argv(&["make", "test"]));
        for text in ["build:\n\tcc main.c\n", "test := unit\nall:\n", "testing:\n\t./run\n", "\ttest: x\n", "# test:\n"] {
            assert!(detected(&[("Makefile", text)]).is_err(), "{text:?}");
        }
    }

    #[test]
    fn nothing_detected_asks_to_set_the_command() {
        let message = detected(&[("README.md", "# project")]).unwrap_err().to_string();
        assert!(message.contains("Imposta il comando di test nel nodo Connection"), "{message}");
    }

    #[test]
    fn symlinked_project_file_is_ignored() {
        let dir = workspace(&[]);
        std::os::unix::fs::symlink("/etc/hosts", dir.path().join("package.json")).unwrap();
        assert!(detect(dir.path()).is_err());
    }

    #[test]
    fn configured_command_wins_over_detection() {
        let dir = workspace(&[("Cargo.toml", "")]);
        let node = Node { test_command: "node --test test/".into(), ..Default::default() };
        let found = resolve(&node, dir.path()).unwrap();
        assert_eq!(found, TestCommand { argv: argv(&["node", "--test", "test/"]), origin: Origin::Configured });
        assert_eq!(found.display(), "node --test test/");
        let blank = Node { test_command: "   ".into(), ..Default::default() };
        assert_eq!(resolve(&blank, dir.path()).unwrap().origin, Origin::Detected("Cargo.toml"));
    }

    #[test]
    fn splitter_handles_quotes_and_whitespace() {
        assert_eq!(split("  pytest   -k 'slow and not db'  \"tests/a b\" ").unwrap(), argv(&["pytest", "-k", "slow and not db", "tests/a b"]));
        assert_eq!(split(r#"go test -run "Test\"Quoted\"""#).unwrap(), argv(&["go", "test", "-run", "Test\"Quoted\""]));
        // No shell: operators stay literal arguments.
        assert_eq!(split("npm test && rm -rf /").unwrap(), argv(&["npm", "test", "&&", "rm", "-rf", "/"]));
        for bad in ["", "   ", "\t", "npm 'test", "npm test\nrm -rf /"] {
            assert!(split(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn display_quotes_arguments_with_spaces() {
        let command = TestCommand { argv: argv(&["pytest", "-k", "a b"]), origin: Origin::Configured };
        assert_eq!(command.display(), "pytest -k 'a b'");
    }
}
