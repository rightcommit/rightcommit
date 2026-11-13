# Using OpenSpec in rightcommit

[OpenSpec](https://github.com/Fission-AI/openspec) is a spec-driven workflow for AI agents:
propose a change, implement it, then archive it. Its skills work with Claude Code, Codex and Pi.

For the general rules, and for other frameworks, see [ADD-A-FRAMEWORK.md](ADD-A-FRAMEWORK.md).

## Step 1 — Install the `openspec` command where rightcommit runs

The agents run `openspec` while they work, and the setup command below uses it too.

- **With Docker or Podman:** add this line to your `Dockerfile.tools`, as explained in
  [Tools for your project's tests](INSTALL.md#tools-for-your-projects-tests), then rebuild:

  ```dockerfile
  RUN npm install -g @fission-ai/openspec@1.16.0 && npm cache clean --force
  ```

- **With a local installation:** install it on your computer (Node.js 22.12 or newer):

  ```sh
  npm install -g @fission-ai/openspec@latest
  ```

## Step 2 — Add OpenSpec to your project

Choose one:

- **At every run, without changing your repository:** in the workflow's **Source** node, set
  **Setup command** to:

  ```text
  openspec init --tools claude,codex,pi
  ```

- **Once, in your repository:** in your project folder, run the same command and commit:

  ```sh
  openspec init --tools claude,codex,pi
  git add .
  git commit -m "Add OpenSpec"
  ```

Both add the OpenSpec skills for the three agents (`.claude/`, `.agents/skills/`, `.pi/`)
and an `openspec/` folder for your specs.

## Step 3 — Use it in a workflow

1. Make your project available to rightcommit: with Docker or Podman, put it in the `sources`
   folder of rightcommit; or push it to a Git repository.
2. In a workflow, add a **Source** node that points to it (**Folder in /sources** or
   **HTTPS Git repository**).
3. Add an **Agent** node with the runtime you want: Claude Code, Codex or Pi.
4. Add **Task** nodes that use that agent:

   | Task | Skill | Prompt |
   |---|---|---|
   | Propose | `openspec-propose` | `Propose this change: <your idea>` |
   | Implement | `openspec-apply-change` | `Implement the change.` |
   | Archive | `openspec-archive-change` | `Archive the change.` |

   When the run starts, rightcommit checks that these skills exist for the Agent's runtime.
5. Optionally, add a **Review** node after "Propose" to approve the plan before any code is written.

The skill names are the same for Claude Code, Codex and Pi, so the same workflow works with
any of the three.
