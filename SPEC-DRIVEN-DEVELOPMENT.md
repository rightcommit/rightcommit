# Spec-driven development with rightcommit

In spec-driven development (SDD) the specification comes first: the agent writes a spec, a plan
and a task list, a person approves them, and only then is code written and checked against the
spec. Frameworks such as [Spec Kit](https://github.com/github/spec-kit) and
[OpenSpec](https://github.com/Fission-AI/openspec) give the agents the skills for each step.

A framework tells the agent what to do at each step. It does not stop the agent from skipping
a step, starting to code before the plan is approved, or reporting tests it never ran.
rightcommit runs the SDD steps as a workflow on the server, so the order, the approvals and the
test results are enforced by the workflow, not left to the agent.

## What rightcommit adds to an SDD framework

| SDD needs | In rightcommit |
|---|---|
| The spec is approved before any code is written | A **Review** node with human approval and a **Gate** node sit between the spec steps and the implementation step |
| The code matches the spec | A **Converge** task compares the implementation with the acceptance criteria and tasks, and a Gate sends the work back if gaps remain |
| Tests really pass | The **Local CI** step runs the project's tests on the server. The result comes from the test command, not from the agent's report |
| The work stays traceable | Each run keeps versions of tracked files with SHA-256 fingerprints and diffs, and the Activity Log shows every step |
| Any coding agent | The same workflow runs with Claude Code, Codex or Pi. You can use one agent to write and another to review |
| Specs start from real requests | Connections read the request from GitHub, Jira, Bitbucket, Azure DevOps or ServiceNow |

## Worked example: a feature from spec to verified commit

The **Feature → SDD → Delivery** template runs a complete Spec Kit cycle on Northstar Helpdesk,
a small Rust project in [`examples/helpdesk`](examples/helpdesk). The feature is a ticket
deduplication service. It needs no external service: only one configured agent.

1. Press **+** and choose **Feature → SDD → Delivery** in the section
   **Develop and evolve with SDD**.
2. Run it. The steps are:

   | Phase | Steps | What happens |
   |---|---|---|
   | Specification | Constitution, Specify, Clarify, Plan, Checklist, Tasks, Analyze | The agent runs the Spec Kit commands in order. If a business decision is missing, Clarify stops and asks instead of assuming an answer |
   | Approval | Design review, **Ready for code** gate | An agent review looks for inconsistencies across spec, plan and tasks, then a person approves. The gate lets the run continue only if no blocking gap remains |
   | Delivery | Implement, Converge, **Convergence verified** gate | The agent implements the tasks, then checks the code against the spec. The gate sends the work back if tasks are not really closed |
   | Verification | Local CI · tests, Code review, **Quality gate** | The server runs `cargo test`. The review reads the code and the real test results. The gate passes only with passing tests |
   | Commit | Verified commit | The run stops and shows what the commit will contain: the files, the diff, the edits left out and the test output. After you approve, it creates a local commit. This node does not push, merge or deploy |

3. Open the run to read the spec, plan and tasks the agent wrote, the diffs of each file, and
   the test output.

A failed gate routes the work back: **Ready for code** to Plan, the other two to Implement. In
this template each step can be reworked up to two times.

To run the same workflow on your own project, change the **Source** node to your folder or Git
repository, and rewrite the prompts of **Specify**, **Clarify** and **Checklist**: they describe
the Northstar feature, so with only a new Source the agent would specify the wrong feature. Give
goals and constraints when you start the run. The template has
**Add Spec Kit if missing** turned on, so a project without a `.specify` folder gets Spec Kit
in the run workspace. See [ADD-A-FRAMEWORK.md](ADD-A-FRAMEWORK.md).

## When the requirement changes: the living spec

The **Incident → Living spec → Delivery** template continues the same example. A follow-up
request in [`incident.json`](examples/helpdesk/incident.json) asks for a different business
rule. The workflow reads the request and decides whether it is a code defect or a change to a
documented rule. For a rule change, a person approves the new rule, the agent updates the
existing specification first, and then the same plan, implement, converge and test steps run
again with a regression test. The resulting spec is in
[`examples/living-spec`](examples/living-spec).

## SDD workflow templates

| Template | Use it to |
|---|---|
| Application overview → Evidence and functional catalog | Analyze an existing codebase into an evidence report and a functional catalog. One agent writes, a second agent verifies independently, a person approves |
| Existing software → SDD baseline | Reconstruct current behavior, rules and tests from source, as verifiable material for Spec Kit |
| Idea → Approved specification | Turn a request into a spec, plan and tasks, without writing code |
| Feature → SDD → Delivery | Run the full cycle from spec to a verified local commit |
| Incident → Living spec → Delivery | Update the spec first when an incident changes a requirement, then deliver the change |
| Evolution → Impact → SDD | Assess a planned change and prepare spec changes, compatibility notes and regression tests |
| Jira + GitHub → SDD, Jira + Bitbucket → SDD | Read a ticket and its repository and prepare the spec, plan, tasks and test strategy |
| ServiceNow · Software requirement to Spec Kit | Read a ServiceNow request and generate a Spec Kit specification with source references |

Every template is an editable workflow: add a review, change the agent on a step, or move a
gate on the canvas.

## Frameworks

- **Spec Kit** ships with rightcommit for Claude Code, Codex and Pi. Nothing to install.
- **OpenSpec** works through a setup command on the Source node. See
  [ADD-OPENSPEC.md](ADD-OPENSPEC.md).
- **Any other framework** that installs its skills in your project works the same way. See
  [ADD-A-FRAMEWORK.md](ADD-A-FRAMEWORK.md).

## Limits

- The commit node creates a local commit with only the files the run changed. Edits that were
  already in the project folder when the run started stay out of it, staged or not. If the run
  changed one of those files again, the commit stops and names the files to review. To push or
  open a pull request, add a Connection.
- With **Confirm before commit** on (the default for new commit nodes and in the templates), the
  commit waits for your approval and is created only if the files are still the ones shown. If
  they changed, the run asks again with the new content. Rejecting stops the run without a
  commit. Long diffs are cut: the preview shows up to 400 lines or 48 KiB per file and 128 KiB
  in total, and says where it cut. An unattended run commits without asking and says so in the
  step report. Workflows saved before this option commit without asking, as before.
- The Local CI step needs the project's test tools installed where rightcommit runs. See
  [Tools for your project's tests](INSTALL.md#tools-for-your-projects-tests).
- A gate can require closed tasks or fresh passing tests, as in this template. The rest of its
  decision is an agent judgment: use a Review node where a person must decide.
