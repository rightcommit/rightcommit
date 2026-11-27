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

/// A passing run shows at most this much of its output in the Activity log; the report keeps all of it.
const PASSING_LINES: usize = 20;
const PASSING_CHARS: usize = 2000;
/// The Activity log message for a passing test command: the last lines of its output, which hold the
/// test runner's totals. The output is redacted like a failure's before it gets here.
pub fn passing_summary(output: &str) -> String {
    let lines: Vec<&str> = output.trim_end().lines().collect();
    let tail = lines[lines.len().saturating_sub(PASSING_LINES)..].join("\n");
    let count = tail.chars().count();
    let tail: String = tail.chars().skip(count.saturating_sub(PASSING_CHARS)).collect();
    if tail.trim().is_empty() {
        return "Test superati: il comando non ha scritto output.".into();
    }
    format!("Test superati. Ultime righe dell’output (completo nel report del passo):\n{}", tail.trim_start_matches('\n'))
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
    split_labeled("Il comando di test", command)
}

/// Words of a one-line command run without a shell. `label` names it in error messages.
pub fn split_labeled(label: &str, command: &str) -> Result<Vec<String>> {
    ensure!(!command.trim().is_empty(), "{label} è vuoto");
    ensure!(command.len() <= 4096, "{label} supera 4096 caratteri");
    ensure!(!command.contains(['\n', '\r', '\0']), "{label} deve stare su una riga");
    let Some(argv) = shlex::split(command) else {
        bail!("{label} non è valido: controlla virgolette e caratteri di escape");
    };
    ensure!(!argv.is_empty(), "{label} è vuoto");
    Ok(argv)
}

/// The Python the detected pytest command runs with, chosen in the run's workspace when the step
/// runs: the project's virtual environment, `.venv` then `venv`, or else the system `python3`.
/// A copied workspace has a virtual environment only if the run created one: the copy leaves the
/// Source's out.
const PYTHONS: &[&str] = &[".venv/bin/python", "venv/bin/python"];
fn python(workspace: &Path) -> String {
    // An executable file, following links: `.venv/bin/python` links to the interpreter.
    PYTHONS
        .iter()
        .find(|python| crate::agents::program_available(python, workspace, &[]))
        .map_or_else(|| "python3".into(), |python| python.to_string())
}

/// The failure of a test command whose Python has no pytest gets a first line that says so and
/// how to fix it; the output follows as written. The Activity log translates the first line.
pub fn explain(command: &TestCommand, error: anyhow::Error) -> anyhow::Error {
    let text = error.to_string();
    if !text.contains("No module named pytest") {
        return error;
    }
    anyhow::anyhow!(
        "pytest is not installed for {}. Install it with the project's dependencies in a virtual \
         environment in the project folder (python3 -m venv .venv, then .venv/bin/pip install pytest), \
         which the detected command uses, or set a test command in the node.\n{text}",
        command.argv[0]
    )
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
            let mut argv: Vec<String> = argv.iter().map(|s| s.to_string()).collect();
            if argv[0] == "python3" {
                argv[0] = python(workspace);
            }
            return Ok(TestCommand { argv, origin: Origin::Detected(found) });
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
    fn passing_output_is_summarized_by_its_last_lines() {
        let unittest = "test_add (test_calculator.CalculatorTest.test_add) ... ok\n\n----------------------------------------------------------------------\nRan 1 test in 0.000s\n\nOK\n";
        let summary = passing_summary(unittest);
        assert!(summary.starts_with("Test superati. Ultime righe dell’output (completo nel report del passo):\ntest_add"));
        assert!(summary.ends_with("Ran 1 test in 0.000s\n\nOK"));
        let long: String = (1..=100).map(|i| format!("line {i}\n")).collect();
        let summary = passing_summary(&long);
        assert!(!summary.contains("line 80\n") && summary.contains("line 81\n") && summary.ends_with("line 100"));
        let wide = format!("{}\nRan 3 tests\nOK", "x".repeat(10_000));
        let summary = passing_summary(&wide);
        assert!(summary.chars().count() < PASSING_CHARS + 100 && summary.ends_with("Ran 3 tests\nOK"));
        assert_eq!(passing_summary(" \n\n"), "Test superati: il comando non ha scritto output.");
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

    fn executable(path: &Path, text: &str) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn python_tests_prefer_the_project_virtual_environment() {
        for file in ["pyproject.toml", "pytest.ini", "setup.py"] {
            let dir = workspace(&[(file, "")]);
            let found = || detect(dir.path()).unwrap();
            assert_eq!(found().argv, argv(&["python3", "-m", "pytest"]), "{file}");
            // Not executable, or a link to nothing: not a Python to run.
            std::fs::create_dir_all(dir.path().join("venv/bin")).unwrap();
            std::fs::write(dir.path().join("venv/bin/python"), "").unwrap();
            std::fs::create_dir_all(dir.path().join(".venv/bin")).unwrap();
            std::os::unix::fs::symlink(dir.path().join("missing"), dir.path().join(".venv/bin/python")).unwrap();
            assert_eq!(found().argv, argv(&["python3", "-m", "pytest"]), "{file}");
            executable(&dir.path().join("venv/bin/python"), "#!/bin/sh\n");
            assert_eq!(found(), TestCommand { argv: argv(&["venv/bin/python", "-m", "pytest"]), origin: Origin::Detected(file) });
            // .venv wins over venv; python -m venv makes bin/python a link to the interpreter.
            std::fs::remove_file(dir.path().join(".venv/bin/python")).unwrap();
            executable(&dir.path().join("interpreter"), "#!/bin/sh\n");
            std::os::unix::fs::symlink(dir.path().join("interpreter"), dir.path().join(".venv/bin/python")).unwrap();
            assert_eq!(found().argv, argv(&[".venv/bin/python", "-m", "pytest"]), "{file}");
            assert_eq!(found().display(), ".venv/bin/python -m pytest");
        }
        // Other ecosystems and a command set in the node are never changed.
        let dir = workspace(&[("Cargo.toml", ""), ("pyproject.toml", "")]);
        executable(&dir.path().join(".venv/bin/python"), "#!/bin/sh\n");
        assert_eq!(detect(dir.path()).unwrap().argv, argv(&["cargo", "test"]));
        let node = Node { test_command: "python3 -m pytest -q".into(), ..Default::default() };
        assert_eq!(resolve(&node, dir.path()).unwrap().argv, argv(&["python3", "-m", "pytest", "-q"]));
    }

    #[test]
    fn a_missing_pytest_says_how_to_install_it() {
        let command = TestCommand { argv: argv(&[".venv/bin/python", "-m", "pytest"]), origin: Origin::Detected("pyproject.toml") };
        let output = ".venv/bin/python: /workspace/.venv/bin/python: No module named pytest";
        let message = explain(&command, anyhow::anyhow!("{output}")).to_string();
        let (first, rest) = message.split_once('\n').unwrap();
        assert_eq!(first, "pytest is not installed for .venv/bin/python. Install it with the project's dependencies in a virtual environment in the project folder (python3 -m venv .venv, then .venv/bin/pip install pytest), which the detected command uses, or set a test command in the node.");
        assert_eq!(rest, output);
        // Any other failure is left as it was.
        let failed = "python3: 1 failed, 2 passed";
        assert_eq!(explain(&command, anyhow::anyhow!("{failed}")).to_string(), failed);
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
    fn gnumakefile_takes_precedence_over_makefile() {
        let found = detected(&[
            ("GNUmakefile", "test:\n\t./run-gnu\n"),
            ("Makefile", "test:\n\t./run\n"),
        ])
        .unwrap();
        assert_eq!(found.argv, argv(&["make", "test"]));
        assert_eq!(found.origin, Origin::Detected("GNUmakefile"));
    }

    #[test]
    fn gnumakefile_without_test_target_shadows_makefile() {
        let found = detected(&[
            ("GNUmakefile", "build:\n\t./build\n"),
            ("Makefile", "test:\n\t./run\n"),
        ]);
        assert!(found.is_err());
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
