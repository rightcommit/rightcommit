# Local CI example

Watch RightCommit run a local test, then inspect an intentional failure. The workflow uses a bundled calculator project and Python's standard library.

## Run the passing test

1. Start RightCommit and sign in. Python 3 must be available where RightCommit runs. The Docker image already includes it.
2. Open **Import workflow** and choose [`workflow.json`](workflow.json). The Source node selects `local-ci/project` from the bundled examples. RightCommit copies it into the run workspace.
3. Open the **Local CI example** workflow. Select **Run workflow** and confirm. No connectors, model API key, or run input are needed.
4. In the run view, select **Local CI tests**. The Activity Log shows the configured command and the passing output, including `Ran 1 test` and `OK`.

## Run a failing test

1. Return to the workflow canvas. Select **Local CI tests** and replace its **Test command** with `python3 -m unittest -v demo_failure`. Save the workflow.
2. Start another run. The test step fails because it deliberately expects `2 + 3` to equal `6`.
3. Select the failed step in the run view and open the Activity Log. Find `Intentional failure for the Local CI example` in the test output.

Restore `python3 -m unittest -v test_calculator` on the node to run the passing test again.
