# Configuring Local CI tests

The **Local CI** step (a Connection node with provider **Local Git / CI** and operation
**test**) runs your project's tests in the run workspace. The result comes from the test
command, not from an agent's report, so a Gate can require passing tests before the workflow
continues.

## Choosing the test command

Set **Test command** on the node, or leave it empty to detect it from the files in the
workspace root, in this order:

| File | Command |
|---|---|
| `Cargo.toml` | `cargo test` |
| `package.json` | `npm test` |
| `pyproject.toml`, `pytest.ini` or `setup.py` | `python3 -m pytest` |
| `go.mod` | `go test ./...` |
| `pom.xml` | `mvn -q test` |
| `build.gradle` or `build.gradle.kts` | `gradle test` |
| `GNUmakefile`, `makefile` or `Makefile` with a `test` target | `make test` |

If several makefiles exist, rightcommit reads the first one in the order make uses:
`GNUmakefile`, then `makefile`, then `Makefile`.

The command runs without a shell, so pipes, redirects and `&&` don't work: put several steps
in a script in your repository and call it. The Activity Log shows the command and whether it
was set on the node or detected.

## Tools

The tool must be installed where rightcommit runs. The Docker image includes Rust, Node.js
with npm, Python 3 and make. The
[Docker install guide](../INSTALL.md#tools-for-your-projects-tests) explains how to add others.

## Try it

The [Local CI example](../examples/local-ci/README.md) imports a workflow so you can inspect a
passing run and an intentional test failure.
