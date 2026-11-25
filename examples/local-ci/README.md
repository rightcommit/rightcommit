# Local CI example

Watch RightCommit run a local test, then inspect an intentional failure. The workflow uses a bundled calculator project and Python's standard library.

## Run the passing test

1. Start RightCommit and sign in. Python 3 must be available where RightCommit runs. The Docker image already includes it.
2. Press **+** (**New workflow**), choose **Local CI example** under **Get started** and press **Save**. If your list does not include it, choose **Import workflow** instead, select [`workflow.json`](workflow.json) and press **Import**. The Source node selects `local-ci/project` from the bundled examples. RightCommit copies it into the run workspace.
3. The new workflow opens. Select **Run workflow**, then **Run now**, and press **Start**. No connectors, model API key, or run input are needed.
4. In the run view, select **Local CI tests**. The Activity Log shows the configured command and the last lines of the passing output, including `Ran 1 test` and `OK`. For the full output, open **Report** under **Artifacts** in the side panel and select `.rightcommit/reports/tests.md`.

## Run a failing test

1. Press **Back to design** above the canvas to return to the workflow. Select **Local CI tests** and replace its **Test command** with `python3 -m unittest -v demo_failure`. Save the workflow.
2. Start another run. The test step fails because it deliberately expects `2 + 3` to equal `6`.
3. Select the failed step in the run view and open the Activity Log. Find `Intentional failure for the Local CI example` in the test output.

Restore `python3 -m unittest -v test_calculator` on the node to run the passing test again.
