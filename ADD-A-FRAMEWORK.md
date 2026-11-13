# Using a framework in your project

Frameworks such as [Spec Kit](https://github.com/github/spec-kit) and
[OpenSpec](https://github.com/Fission-AI/openspec) give the agents skills (or commands) for a
way of working: specify, plan, implement, review. rightcommit works with any framework that
installs its skills in your project, and the same workflow can run with Claude Code, Codex or Pi.

The agents read a framework's skills from the run workspace, which is a copy of your
**Source** (or your folder itself, if the Source edits it permanently). There are three ways to
get them there.

| Way | When to use it |
|---|---|
| **Add Spec Kit if missing** on the Source node | Spec Kit, nothing to install |
| **Setup command** on the Source node | Any framework that has an install command, such as OpenSpec |
| **Commit the framework to your project** | You want it in your repository for good |

## Spec Kit

rightcommit ships Spec Kit 1.2.2 for Claude Code, Codex and Pi.

1. Select the **Source** node.
2. Turn on **Add Spec Kit if missing**.

When a run starts and the Source has no `.specify` folder, rightcommit adds Spec Kit to the
workspace. It never changes files that are already there. The Spec Kit workflow templates have
this option on, so you can point their Source at your own project and run them.

The added files are changes made by the run: if the workflow ends with a commit or a pull
request, Spec Kit goes into your repository with it. With a Source that edits your folder
permanently, the files are added to the folder itself.

If your project already has Spec Kit, the option does nothing. A project set up for one
agent only (for example `specify init --ai claude`) runs with that agent; to use the others,
add their Spec Kit files to the project with the Spec Kit CLI.

## Any framework with a setup command

The **Setup command** of a Source runs in the Source folder when a run starts, before the first
node. For OpenSpec:

```text
openspec init --tools claude,codex,pi
```

The command:

- runs without a shell: pipes, redirects and `&&` are not supported. For more steps, call a
  script in your repository, such as `sh scripts/setup-agents.sh`;
- runs with the same isolation as the agents, without their credentials;
- must be safe to run again, because it runs at every run. It is skipped when a run continues
  from an earlier run, whose workspace is already prepared;
- stops the run if it fails, with its output in the error.

Its program must be installed where rightcommit runs:

- **With Docker or Podman:** add it to your own image, as in
  [Tools for your project's tests](INSTALL.md#tools-for-your-projects-tests). For OpenSpec, the
  `Dockerfile.tools` line is:

  ```dockerfile
  RUN npm install -g @fission-ai/openspec@1.16.0 && npm cache clean --force
  ```

- **With a local installation:** install it on your computer, for example
  `npm install -g @fission-ai/openspec`.

When you import a workflow, rightcommit lists the setup and test commands it would run on the
server, so you can check them first.

## Commit the framework to your project

Install the framework in your project on your computer and commit its files. For OpenSpec:

```sh
openspec init --tools claude,codex,pi
git add .
git commit -m "Add OpenSpec"
```

Any skill can be added the same way: see [ADD_A_SKILL.md](ADD_A_SKILL.md).

## Use the skills in a workflow

In a **Task** node, write the skill name in the **Skill** field, or name the skill in the
prompt. The Skill field takes:

- Spec Kit commands by their short name: `specify`, `clarify`, `plan`, `tasks`, `analyze`,
  `implement`, `checklist`, `constitution`, `converge`;
- any other skill by its name, for example `openspec-propose`, `openspec-apply-change` or
  `openspec-archive-change`;
- a Claude Code command with folders, for example `opsx:propose`.

When the run starts, rightcommit checks that every skill in a Skill field exists for the
Agent's runtime, and stops with the missing name before any agent works. It looks in these
folders of the workspace:

| Agent | Folders |
|---|---|
| Claude Code | `.claude/skills/<name>/SKILL.md`, `.claude/commands/<name>.md` |
| Codex | `.agents/skills/<name>/SKILL.md` |
| Pi | `.pi/prompts/<name>.md`, `.pi/skills/<name>/SKILL.md`, `.agents/skills/<name>/SKILL.md` |

A skill that only the agent's own setup provides (outside the project) is not checked: leave
the Skill field empty and name the skill in the prompt.

To add tools or skills to Pi for one account instead, see [ADD-PI-PACKAGES.md](ADD-PI-PACKAGES.md).
