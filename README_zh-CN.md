<p align="center">
  <a href="https://rightcommit.com">
    <img src=".github/assets/rightcommit-icon.png" alt="rightcommit 图标" width="128" height="128">
  </a>
</p>

<h1 align="center">
  <a href="https://rightcommit.com">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="https://github.com/rightcommit/rightcommit/raw/HEAD/.github/assets/rightcommit-wordmark-dark.svg?sanitize=true">
      <img src=".github/assets/rightcommit-wordmark-light.svg" alt="rightcommit" width="360" height="72">
    </picture>
  </a>
</h1>

[English](README.md) | 简体中文

<!-- 更新 README.md 时，请同步更新本文件。 -->

<p align="center">
  <strong>面向 AI 编程智能体的开源工作流引擎。</strong>
</p>

<p align="center">
  描述要做的事，即可在可视化画布上获得可编辑的工作流。让 Claude Code、Codex 和 Pi 按你设定的流程执行任务，并经过你指定的测试、审查和审批。适用于任何方法或框架。支持自托管。
</p>

<p align="center">
  <a href="https://rightcommit.com"><strong>官网</strong></a> ·
  <a href="#quickstart"><strong>快速开始</strong></a> ·
  <a href="#documentation"><strong>文档</strong></a> ·
  <a href="#contributing"><strong>参与贡献</strong></a> ·
  <a href="https://github.com/rightcommit/rightcommit/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22"><strong>入门议题</strong></a> ·
  <a href="#license"><strong>许可证</strong></a>
</p>

<p align="center">
  <img src=".github/assets/rightcommit-walkthrough.gif" alt="rightcommit 演示：Project Manager 生成工作流草稿，随后执行、测试、审查、本地提交并查看 FinOps" width="780">
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-6254e8" alt="许可证：Apache-2.0"></a>
  <img src="https://img.shields.io/badge/built%20with-Rust-b7410e" alt="使用 Rust 构建">
  <a href="https://github.com/rightcommit/rightcommit/actions/workflows/docker-check.yml"><img src="https://github.com/rightcommit/rightcommit/actions/workflows/docker-check.yml/badge.svg?branch=main" alt="Docker 镜像检查"></a>
  <a href="https://github.com/rightcommit/rightcommit/actions/workflows/package-check.yml"><img src="https://github.com/rightcommit/rightcommit/actions/workflows/package-check.yml/badge.svg?branch=main" alt="安装包检查"></a>
  <a href="https://github.com/rightcommit/rightcommit/stargazers"><img src="https://img.shields.io/github/stars/rightcommit/rightcommit?style=social" alt="GitHub Star 数"></a>
</p>

---

## 可以用它构建什么

可以从 20 多个现成的工作流开始，也可以向 Project Manager 描述新的工作流。

- **任何智能体配置。** 在画布上组合智能体、审查、门禁和连接。并行运行步骤，调用其他工作流，按计划运行，或通过 A2A 发布工作流。
- **你偏好的 \*-driven 开发方式。** 规范驱动、测试驱动、行为驱动或你自己的方法：每个阶段是一个步骤，由门禁决定何时进入下一阶段。内置 Spec Kit；OpenSpec 或其他任何框架都可以通过 setup 命令安装。详见 [规范驱动开发](SPEC-DRIVEN-DEVELOPMENT.md)。
- **从工单到经过测试的代码。** 把 GitHub、Jira 或 ServiceNow 工单变成需求、规范和测试，再交付新功能、变更和事件修复，并附带回归测试。
- **遗留代码分析。** 根据源代码重建行为、规则和测试，由第二个智能体核查，作为后续工作的基线。
- **安全审查。** 在服务器上运行扫描器，让智能体分析并修复发现的问题；在扫描结果干净或有人接受风险之前，阻止提交。
- **发布、审查与 IT 服务管理。** 发布说明和回滚计划、拉取请求审查文档、Confluence 简报，以及借助本地知识库对 ServiceNow 工单进行分类。

<a name="quickstart"></a>

## 快速开始

### macOS：Homebrew

```sh
brew install rightcommit/tap/rightcommit
mkdir -p ~/rightcommit && cd ~/rightcommit
rightcommit setup
rightcommit
```

在搭载 Apple 芯片且运行 macOS 15 或更高版本的 Mac 上，Homebrew 会在几秒内下载可直接使用的 rightcommit；在 Intel Mac 和较旧的 macOS 版本上，Homebrew 会从源码编译，需要几分钟。首次运行 `rightcommit setup` 以安装智能体。以后从同一目录启动 `rightcommit`，即可继续使用已有数据。参见 [macOS 安装指南](INSTALL_MAC.md)。如需清空数据、恢复到首次安装时的状态，请先停止应用，再运行 `rightcommit resetdb`；它会删除哪些数据、如何恢复它留下的备份，参见[重置与恢复指南](HOW_TO_RESET_RESTORE_RIGHTCOMMIT.md)。

### Windows、Linux 或任意系统：Docker

Docker 在 macOS、Windows 和 Linux 上运行包含所有依赖的 rightcommit，并让每个账户以独立的系统用户运行。启动 Docker，然后运行以下命令克隆并启动 rightcommit。

```sh
git clone https://github.com/rightcommit/rightcommit.git && cd rightcommit && docker compose up -d
```

首次启动会下载最新发布版本的预构建镜像（约 1 GB），所需时间取决于网络速度，通常为几分钟。下载 ZIP、查看分步说明或从源码构建镜像（首次需要 10–30 分钟），请参阅 [Docker 安装指南](INSTALL.md)。如何选择安装方式，请参阅[选择哪种安装方式](INSTALL.md#which-install-to-choose)。

<!-- TODO: link Gitee
如果在中国大陆访问 GitHub 较慢，可在 Gitee 镜像上线后使用该镜像。
-->

### 运行第一个工作流

1. 打开 <http://localhost:8080>。首个账户将成为管理员，此后默认关闭公开注册。
2. **无需 API 密钥，先检查安装是否正常。** 点击 **+**，在 **Get started** 下选择 **Local CI example**，然后点击 **Save**。依次点击 **Run workflow** → **Run now** → **Start**。服务端会运行一个内置的小型 Python 项目的测试：选中 **Local CI tests**，Activity log 会显示输出的最后几行，可以看到 `Ran 1 test` 和 `OK`；在 **Artifacts** 下打开它的报告，可以查看完整输出。
3. 在 **Personal settings → Agents** 中，为至少一个智能体配置 API 密钥或已获授权的服务提供方，并选择模型。参见[配置智能体](#configuring-the-agents)。
4. **在内置示例上运行智能体。** 点击 **+**，选择 **Feature → SDD → Delivery**。它使用 rightcommit 内置的小型 Rust 项目 Northstar Helpdesk：智能体为一个功能编写规格说明和代码，服务端运行 `cargo test`，最后在本地提交，不会推送。按第 2 步的方式启动。运行暂停等待你审批时，点击 **Review required**。

在 **+** 列表中，标记为 **Local** 的工作流不需要任何外部账户。如果卡片上还列出了某个智能体（例如 **Local · Codex**），也需要先配置该智能体：如果工作流用到的智能体尚未配置，启动时会被拒绝，并在提示中列出这些智能体。标记为 GitHub、Jira、Bitbucket、Confluence 或 ServiceNow 的工作流需要先连接对应服务，建议稍后再用。你也可以向 **Project Manager** 描述所需的工作流。完整步骤、每次运行的前提条件以及结果的查看位置，请参阅 [Your first run](INSTALL.md#your-first-run)。

**使用你自己的代码。** 将项目复制或克隆到 `sources` 目录：使用 Docker 或 Podman 时是 `compose.yaml` 旁边的目录，使用 Homebrew 时是 `~/rightcommit/sources`。在工作流中选中 **Source** 节点，将 **Source type** 设为 **Folder in /sources**，然后选择你的项目。参见 [Your own project folders](INSTALL.md#your-own-project-folders)。

如果觉得有用，欢迎[给 rightcommit 点 Star](https://github.com/rightcommit/rightcommit)。
你也可以[报告问题或提出建议](https://github.com/rightcommit/rightcommit/issues)，或阅读[贡献指南](CONTRIBUTING.md)。

<a name="为什么选择-rightcommit"></a>
<a name="主要功能"></a>

## 如何掌控工作流

### AI 智能体说“完成了”，rightcommit 来验证。

- **分配智能体和工具。** 为每个需要智能体的步骤选择 Claude Code、Codex 或 Pi。可以直接在画布上搭建流程，也可以编辑 Project Manager 生成的草稿。
- **设置质量门禁。** 检查任务是否已关闭，[在服务端运行项目测试](guides/CONFIGURING_LOCAL_CI.md)，文件变更后要求重新测试。还可以加入智能体审查，并在创建本地提交前审批确切的文件和差异。
- **查看每个产出文件。** 每个步骤都会记录它创建或修改的文件，包括版本、SHA-256 指纹，以及对支持的文本文件的逐行比对。可在运行视图中筛选、比较和下载这些文件。检查失败时可退回修改，并限制重试次数。
- **从任意步骤重新开始。** 从你选择的节点继续已停止的运行。之前的结果、文件和智能体会话都会保留，只有该节点及其后续节点重新运行，已完成的工作不必再付一次费用。也可以修改工作流后在同一工作区中继续。
- **决定何时介入。** 可选择交互、辅助或无人值守模式，暂停等待人工决定，再从保存的位置继续。
- **追踪用量与成本。** 在 FinOps 中按步骤、运行记录、工作流、智能体和模型查看已记录的 Token 用量和可用的成本数据。智能体报告的成本和估算值分别显示。

## 连接与集成

- **智能体：** 支持 Claude Code、Codex 和 Pi。可使用 API 密钥或已获授权的网关，例如 Anthropic、OpenAI、OpenRouter、AWS Bedrock、Google Vertex 和 Microsoft Foundry。
- **服务：** 支持 GitHub、Azure DevOps、ServiceNow，以及通过 Atlassian CLI 接入的 Jira、Confluence 和 Bitbucket。默认情况下，内置集成在向外部系统写入前会要求确认具体写入内容。
- **开放标准：** 使用 MCP 为智能体接入 MCP 服务器上的工具，使用 A2A 发布可供其他智能体调用的工作流。

## 适用人群

rightcommit 支持个人使用，也支持在组织内部部署并使用独立账户。请阅读[服务提供方条款与许可用途](PROVIDER_TERMS.md)和[预期用途与第三方服务提供方义务](INTENDED_USE.md)。服务提供方名称用于标识集成，不代表存在关联或获得背书。

- **开发者：** 在自己的笔记本电脑上运行智能体，使用自己的密钥。
- **小型团队：** 自托管部署，各用户使用独立的账户、密钥和工作区。

## 其他安装方式

| 指南 | 适用场景 |
|---|---|
| [INSTALL_MAC.md](INSTALL_MAC.md) | 在 macOS 上无容器安装：Homebrew、crates.io 或源码 |
| [INSTALL.md](INSTALL.md) | 在 macOS、Windows 和 Linux 上使用 Docker，含分步说明 |
| [INSTALL_LINUX.md](INSTALL_LINUX.md) | 在 Linux 和 WSL2 上无容器安装：crates.io 或源码 |
| [INSTALL_PODMAN.md](INSTALL_PODMAN.md) | 使用 Podman 替代 Docker |

### 管理 Docker 安装

| 操作 | 命令 |
|---|---|
| 停止应用 | `docker compose down` |
| 获取新版本后更新 | 先运行 `docker compose pull`，再运行 `docker compose up -d` |
| 从源码构建镜像 | `docker compose -f compose.yaml -f compose.build.yaml up -d --build` |
| 更换端口 | 在 `.env` 文件中设置 `RIGHTCOMMIT_PORT=9090`，然后运行 `docker compose up -d` |

> [!WARNING]
> 请勿运行 `docker compose down -v`。此命令会删除 `rightcommit-data` 卷及其中的所有账户、工作流和密钥。

默认情况下，Docker Compose 将应用绑定到 localhost。

<a name="configuring-the-agents"></a>

## 配置智能体

请先阅读[服务提供方条款与许可用途](PROVIDER_TERMS.md)。三个智能体都支持多用户使用。仍需遵守服务提供方的授权要求和凭据权限范围。

设置中只接受 API 密钥，不接受 Claude Pro/Max 或 ChatGPT 订阅，应用也不提供 Claude 或 ChatGPT 账号登录。管理员可以在 `.env` 中设置令牌，让 Claude Code 使用自己的 Claude 订阅；其他用户都需要 API 密钥。

| 智能体 | 所需凭据 | 指南 |
|---|---|---|
| Claude Code | Claude Console 中创建的 Anthropic API 密钥，或由管理员配置的网关、Amazon Bedrock、Google Vertex AI、Microsoft Foundry。仅管理员可以通过 `.env` 中的 `CLAUDE_CODE_OAUTH_TOKEN` 使用自己的 Claude 订阅；API 密钥字段会拒绝订阅令牌。 | [HOW_TO_CONFIGURE_CLAUDECODE.md](HOW_TO_CONFIGURE_CLAUDECODE.md) |
| Codex | OpenAI 平台 API 密钥，或由管理员配置的兼容服务提供方。不支持 ChatGPT 登录。 | [HOW_TO_CONFIGURE_CODEX.md](HOW_TO_CONFIGURE_CODEX.md) |
| Pi | 所选服务提供方（Anthropic、OpenAI、Gemini 等）的 API 密钥或云凭据。使用 Anthropic 时，Claude 订阅令牌会被拒绝。 | [HOW_TO_CONFIGURE_PI.md](HOW_TO_CONFIGURE_PI.md) |
| 将 Pi 与 OpenRouter 搭配使用 | 账户中有余额的 OpenRouter API 密钥。 | [HOW_TO_CONFIGURE_PI_OPENROUTER.md](HOW_TO_CONFIGURE_PI_OPENROUTER.md) |
| 在 Pi 中使用本地模型 | 在本机运行的模型服务器（Ollama、LM Studio、vLLM），无需 API 密钥。 | [HOW_TO_CONFIGURE_PI_LOCAL.md](HOW_TO_CONFIGURE_PI_LOCAL.md) |

<a name="documentation"></a>

## 文档

| 文档 | 内容 |
|---|---|
| [SPEC-DRIVEN-DEVELOPMENT.md](SPEC-DRIVEN-DEVELOPMENT.md) | 用 Spec Kit、OpenSpec 或其他任何框架进行规范驱动开发：完整示例和 SDD 工作流模板 |
| [CHANGELOG.md](CHANGELOG.md) | 各版本的变更 |
| [HOW_TO_RESET_RESTORE_RIGHTCOMMIT.md](HOW_TO_RESET_RESTORE_RIGHTCOMMIT.md) | 清空数据重新开始、恢复重置留下的备份、删除旧备份 |
| [ADD_A_SKILL.md](ADD_A_SKILL.md) | 一次为 Claude Code、Codex 和 Pi 添加技能 |
| [ADD-PI-PACKAGES.md](ADD-PI-PACKAGES.md) | 将 pi.dev 上的包添加到 Pi 智能体 |
| [ADD-A-FRAMEWORK.md](ADD-A-FRAMEWORK.md) | 在自己的项目中使用 Spec Kit、OpenSpec 或其他框架 |
| [ADD-OPENSPEC.md](ADD-OPENSPEC.md) | 在工作流中使用 OpenSpec |
| [UPDATE-PI-DEFAULT-PACKAGES.md](UPDATE-PI-DEFAULT-PACKAGES.md) | 更新 rightcommit 随附的 Pi 工具（面向维护者） |
| [配置指南](guides/README.md) | 配置访问权限并运行工作流示例，涵盖 ServiceNow（ITSM、目录请求和 Spec Kit）、Atlassian（Jira、Bitbucket 和 Confluence）GitHub（Issue 和 CI）以及 Local CI 测试 |

<a name="license"></a>

## 许可证

Copyright (c) 2026 Lorenzo Toscano

rightcommit 是开源软件，采用 **[Apache-2.0 许可证](LICENSE)** 发布。

根据许可证条款，你可以使用、修改、自托管和分发代码，包括用于商业目的。完整条款见 [LICENSE](LICENSE)。

上述许可适用于 rightcommit 本身。与之配合使用的第三方产品或服务仍受各自条款和[支持的部署范围](INTENDED_USE.md)约束。

随附图标和 Spec Kit 文件的第三方声明见 [`licenses/`](licenses)。

<a name="contributing"></a>

## 参与贡献

欢迎提交 Pull Request。项目依据 [Apache-2.0 许可证](LICENSE)接受贡献。参见 [CONTRIBUTING.md](CONTRIBUTING.md)。

第一次参与 rightcommit？[查看尚未关闭的入门议题](https://github.com/rightcommit/rightcommit/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22)。

发现问题或有建议？欢迎[创建 Issue](https://github.com/rightcommit/rightcommit/issues)。
欢迎用中文提 Issue。

咨询与反馈请发邮件至 [hello@rightcommit.com](mailto:hello@rightcommit.com)。

## 贡献者

<a href="https://github.com/Shards-inc"><img src="https://avatars.githubusercontent.com/u/226203219?v=4&amp;s=96" alt="Shards-inc" width="48" height="48"></a>
<a href="https://github.com/dannyward630"><img src="https://avatars.githubusercontent.com/u/71604164?v=4&amp;s=96" alt="dannyward630" width="48" height="48"></a>
<a href="https://github.com/Golden007-prog"><img src="https://avatars.githubusercontent.com/u/140976895?v=4&amp;s=96" alt="Golden007-prog" width="48" height="48"></a>

---

rightcommit 是 Lorenzo Toscano 以个人身份创建并维护的独立业余项目。他在业余时间使用个人资源开发。该项目与其雇主、雇主的客户或其他组织无关，也未获得这些机构的背书或赞助。项目中的观点与决定均为个人立场。

<sub>Claude Code、Codex、Pi、GitHub、Azure DevOps、ServiceNow、Jira、Confluence、Bitbucket、Spec Kit、Rust、Docker 等名称和商标归各自权利人所有。rightcommit 与这些权利人无关联。</sub>
