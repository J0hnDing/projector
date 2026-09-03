<div align="center">

<img src="src/assets/projector-logo.png" alt="Projector logo" width="104" height="104">

# Projector

### A lightweight, local-first workspace for building and managing software with AI agents.

[![Local first](https://img.shields.io/badge/local--first-yes-14b8a6?style=for-the-badge)](#local-first-and-narrowly-permissioned)
[![Tauri](https://img.shields.io/badge/desktop-Tauri%202-FFC131?style=for-the-badge&logo=tauri&logoColor=111827)](src-tauri/Cargo.toml)
[![React](https://img.shields.io/badge/frontend-React%2019-61DAFB?style=for-the-badge&logo=react&logoColor=111827)](package.json)
[![Rust](https://img.shields.io/badge/backend-Rust-000000?style=for-the-badge&logo=rust&logoColor=white)](src-tauri/Cargo.toml)

[**Why Projector**](#why-projector) · [**Project memory**](#a-shared-project-memory) · [**Agent workflow**](#the-human-and-agent-workflow) · [**Agent configuration**](#agentsmd-without-the-maintenance-burden) · [**Run locally**](#run-from-source)

</div>

> [!IMPORTANT]
> Projector has no hosted account, remote database, or second cloud copy of your project. Your project files remain the source of truth.

## Why Projector

AI can turn an idea into working code remarkably quickly. That speed creates a different kind of problem: the project can move faster than its developer can keep a reliable mental model of it.

- What did I implement?
- What did an AI agent implement that I have not reviewed yet?
- What is unfinished, blocked, or supposed to happen next?
- Which instructions are my agents actually following?
- How do I keep several vibe-coded projects understandable without adding another heavyweight planning system?

Projector is built around those questions. It gives every registered project a small, durable workspace made from familiar Markdown files, then presents that state in a focused desktop interface. Humans and agents share the same TODOs and working history, agent-completed work enters an explicit review queue, and project instructions stay close to the code in `AGENTS.md`.

## The Projector approach

Traditional project-management software is often designed for teams, tickets, sprints, and reporting. Projector is designed specifically for developers working locally on AI-assisted and vibe-coded products.

Its job is to preserve continuity while implementation moves quickly:

1. **Know what the project is.** Read its README, agent instructions, startup notes, TODOs, and working history in one place.
2. **Know what comes next.** Keep unfinished work structured, prioritized, categorized, and dependency-aware.
3. **Know what needs your attention.** Let agents submit completed work for review without silently declaring it accepted.
4. **Know what actually happened.** Turn approved completions into a chronological, Git-friendly working history.
5. **Keep agents aligned.** Edit and distribute `AGENTS.md` guidance and project-scoped subagent configurations without hand-maintaining the same boilerplate in every repository.

Projector is intentionally small. It is a workspace for understanding and steering local projects, not a replacement for an IDE, a cloud issue tracker, or a general process manager.

| Local by default | Human-reviewed | Agent-ready |
| :---: | :---: | :---: |
| Project state stays in readable files beside the code. | Agent-reported completion waits for explicit approval. | TODO, history, `AGENTS.md`, and subagent workflows are designed to work together. |

## A shared project memory

Projector treats a concise set of files as the readable memory of a project:

| File | What it answers | How Projector uses it |
| --- | --- | --- |
| `README.md` | What is this project? | Renders the project overview as sanitized GitHub-flavored Markdown. |
| `AGENTS.md` | How should agents work here? | Displays project guidance and can manage selected Projector and Subagents sections from Settings. |
| `STARTUP.md` | How do I start it? | Shows startup instructions and supports an explicit, user-triggered PowerShell launcher. |
| `TODO.md` | What should happen next? | Parses open work into searchable, sortable, dependency-aware records. |
| `WORK_HISTORY.md` | What has been completed and accepted? | Presents a chronological record of completed work, including summaries and limitations. |

These files are ordinary Markdown. They can be read in an editor, reviewed in Git, copied with the repository, and understood without Projector. Projector adds structure and safer workflows without replacing the files with an opaque application database.

For supported documents, the project root takes precedence over `docs/`, and filenames are matched case-insensitively. `WORKING_HISTORY.md` is also recognized as a compatibility alias for `WORK_HISTORY.md`.

## The human-and-agent workflow

### 1. Create or register a project

Register any existing local project directory, or create a Projector-ready folder from the app.

A newly created project receives:

- `README.md`
- `STARTUP.md`
- `AGENTS.md`
- `TODO.md`
- `WORK_HISTORY.md`
- `.codex/agents/worker-low.toml`
- `.codex/agents/worker-medium.toml`
- `.codex/agents/worker-high.toml`

Git initialization is optional. When selected, Projector creates an unborn local repository using the configured `init.defaultBranch` when valid, or `main` otherwise. It does not create a commit, remote, license, or generic `.gitignore`.

Registering a project does not rewrite it. Removing a project from Projector removes only the registry entry; it never deletes the project directory.

### 2. See the whole workspace

The sidebar keeps registered projects in a user-defined order and surfaces the information most useful during rapid development: project path, working-tree state, upstream synchronization state, and recent activity.

Inside a project, focused tabs expose:

- the README;
- `AGENTS.md`;
- startup instructions;
- structured TODOs;
- pending agent review proposals;
- working history; and
- recent Git activity.

Filesystem changes beneath registered roots trigger refreshes automatically, with a manual refresh available when needed.

### 3. Keep the next work explicit

Projector turns `TODO.md` into a compact work view instead of treating it as an unstructured wall of text. TODOs carry:

- a stable `TODO-NNN` ID;
- priority: `critical`, `high`, `medium`, or `low`;
- category: `feature`, `bugfix`, `refactor`, `test`, `documentation`, `research`, or `others`;
- an area of the project;
- dependencies on other TODO IDs;
- a rationale; and
- acceptance criteria.

The interface supports creating TODOs, priority, availability, category, and age filters. Opening a TODO shows its dependencies, rationale, and acceptance criteria, and lets the user permanently delete it. Deletion preserves the IDs of remaining TODOs and clears references to the deleted item from their dependencies. Missing dependencies, cycles, malformed entries, and unrecognized source content are surfaced as validation warnings rather than silently discarded.

TODO status is derived rather than duplicated: an item with unresolved dependencies is blocked; one without unresolved dependencies is available to work on.

### 4. Let agents record progress

While Projector is running, local agents can use its loopback API to:

- register new unfinished work;
- submit a TODO as completed with a summary and limitations; and
- submit a standalone working-history entry for notable work that was not represented by a TODO.

This keeps project bookkeeping in the same agent loop as implementation. An agent does not need to ask the developer to manually reconstruct every task or completion after the fact.

### 5. Review before history becomes fact

> [!IMPORTANT]
> Agent-reported completion is not automatically treated as accepted work. Completing a TODO or submitting standalone history creates a **Pending Review** proposal; project Markdown remains unchanged until the user approves it in the desktop app.

- **Approve a TODO completion:** Projector revalidates the live TODO, removes it from `TODO.md`, clears the satisfied dependency from remaining TODOs, and appends the proposed entry to `WORK_HISTORY.md`.
- **Approve standalone history:** Projector appends the proposed entry to `WORK_HISTORY.md`.
- **Reject:** Projector discards the proposal without changing project files.

This makes the distinction visible:

- `TODO.md` is what remains to be done.
- **Pending Review** is what an agent says is done but the developer has not accepted.
- `WORK_HISTORY.md` is what has been reviewed and accepted as completed.

That boundary is central to Projector: agents can keep records current, but the user remains the authority on what becomes project history.

## `AGENTS.md` without the maintenance burden

Agent instructions are part of the project, but keeping them useful across several repositories can become repetitive. Projector provides an `AGENTS.md` Settings experience for the sections it knows how to manage:

- **Custom instructions** for your own reusable guidance;
- **Projector** instructions describing the structured TODO and working-history API; and
- **Subagents** instructions describing when and how work should be delegated.

You can edit the complete Markdown for each section, preview the generated files, save the configuration as the default for new projects, or reset it to Projector's bundled defaults.

Settings do not silently rewrite existing repositories. When you want to update current projects, explicitly select one, several, or all registered projects and run the migration. Projector preserves unrelated `AGENTS.md` content, writes an adjacent `.projector-backup` before the first managed overwrite, validates that every target stays inside the registered root, and reports failures per project.

## Project-scoped subagents

Projector generates three configurable Codex worker definitions under `.codex/agents/`:

- `worker_low` for mechanical, localized, low-risk changes;
- `worker_medium` for standard features and bug fixes that require investigation and tests; and
- `worker_high` for complex, coupled, ambiguous, or high-risk work.

For each worker, Settings exposes the model, reasoning effort, description, and developer instructions. The generated `AGENTS.md` section also establishes delegation expectations such as bounded ownership, acceptance criteria, validation, reporting, and main-agent integration responsibility.

Codex's built-in read-only `explorer` remains available alongside these project-defined workers. Projector is configuring the project files agents consume; it is not hosting or remotely orchestrating the agents itself.

## Git awareness without becoming a Git client

Projector provides enough Git context and control for the everyday local workflow while keeping the boundary narrow:

- display the current branch, clean or dirty working-tree state, upstream state, and up to 25 recent commits;
- run a non-blocking, deduplicated `git fetch --all --prune` at startup and on manual refresh;
- pull only with fast-forward-only semantics when the current branch has an upstream and the working tree is clean;
- stage all current changes and create one libgit2 commit from a user-entered message, without executing hooks; and
- push only the checked-out branch to its existing configured upstream.

Fetch, pull, and push are each bounded to 30 seconds. Pull never merges, rebases, recurses into submodules, or prompts for credentials. Push never forces, creates an upstream, pushes tags, or prompts for credentials. Projector does not expose checkout or a general shell through the frontend.

## Explicit project startup

`STARTUP.md` keeps the knowledge required to run a project next to the code. Projector renders that file and adds copy controls for fenced PowerShell blocks.

The sidebar **Start** action is deliberately explicit. Only when the user presses it, Projector:

1. reads the registered project's discovered `STARTUP.md`;
2. opens every non-empty fenced block whose language is exactly `powershell` in a separate visible PowerShell console rooted at the project directory; and
3. opens deduplicated HTTP(S) links found outside code blocks in the system browser.

Projector does not validate or sequence those project-authored commands, wait for readiness, hide the consoles, track the processes, or provide stop and restart controls. Closing Projector does not stop anything launched from `STARTUP.md`.

## Local-first and narrowly permissioned

The Rust backend owns Projector's local trust boundary.

- Only directories explicitly registered by the user are observed.
- After registration, frontend commands use registry IDs rather than arbitrary paths.
- Paths are canonicalized, and documents or Git metadata resolving outside the registered root are rejected.
- Removing a registry entry never removes or edits the project directory.
- Markdown files remain the canonical project state.
- Structured writes are serialized and use atomic replacement.
- Rendered Markdown passes through a raw-parse-then-sanitize pipeline so common README layout HTML works while scripts, event handlers, and unsafe URLs do not.
- The frontend has no general filesystem, shell, or process permission.

Projector stores only its own local metadata and preferences in the operating system application-data directory for `com.local.projector`, including the project registry, Git fetch timestamps, pending review proposals, appearance preference, and saved agent settings. It does not copy project documents or Git history into a second canonical store.

Projector currently has no cloud synchronization, analytics, accounts, team features, Kanban board, arbitrary document editor, or remote API.

## Technology

Projector is a [Tauri 2](https://v2.tauri.app/) desktop application with a React and TypeScript interface built by Vite.

Tauri uses the operating system webview rather than bundling a browser engine. Rust handles registry validation, filesystem observation, Markdown state parsing and writing, Git operations, startup launching, and the loopback agent API. React provides the project workspace, review flow, settings, and document views.

The current workflow is Windows-oriented, including visible PowerShell startup consoles and Windows packaging.

## Run from source

### Prerequisites

- Node.js and npm
- Rust and Cargo
- Tauri 2 platform prerequisites
- WebView2 and Windows build tools on Windows

Install dependencies and launch the development app:

```powershell
npm install
npm run tauri dev
```

Projector uses the project-local Tauri CLI; a global installation is not required.

### Verification

```powershell
npm test
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml
npm run tauri build -- --no-bundle
```

Build the Windows NSIS installer with:

```powershell
npm run tauri build -- --bundles nsis
```

## Advanced migration

The Settings UI is the supported way to apply current Projector and subagent defaults to selected registered projects.

The repository also retains a bounded legacy state migration utility:

```powershell
cargo run --manifest-path src-tauri/Cargo.toml --bin projector-migrate
```

It scans only direct project directories beneath its approved projects root, recognizes root-level or `docs/` state files, creates adjacent `.projector-backup` files before changing managed content, validates the migrated result with Projector's production parser, restores originals on validation failure, and writes `MIGRATION_REPORT.md`. Repeated runs are designed to be idempotent.

## Current limitations

- Displayed document content is capped at 2 MB; larger files are clearly marked as truncated.
- Git history is capped at the 25 most recent commits.
- Very large directory trees can consume additional filesystem-watcher resources; manual refresh remains available.
- Git submodules and linked worktree metadata outside the registered project directory are not traversed.
- The registry does not yet support import/export, path relocation, or custom display names.
- The loopback API is available only while Projector is running and has no remote transport.
- Multi-file approval includes in-process rollback if a replacement fails, but it is not a distributed transaction across storage devices.
- Broader runtime management, health checks, stop/restart controls, arbitrary file editing, and broader Git workflows remain out of scope.
- Projector retains a bounded, content-free Codex lifecycle collector, but its project tab is currently hidden because Codex Desktop lifecycle hooks do not yet provide dependable status reporting.

---

<div align="center">
  <sub>Keep the project legible while humans and agents keep it moving.</sub>
</div>
