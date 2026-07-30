# Projector

Projector is a lightweight local desktop project manager shared by people and software agents. Register a project directory, inspect structured TODOs and working history, read its README, observe Git state, and deliberately fast-forward a clean project from its configured upstream. Markdown files remain the human-readable source of truth.

## MVP capabilities

- Create Projector-ready project folders or register existing local project directories. New projects receive `AGENTS.md`, `TODO.md`, and `WORK_HISTORY.md`; removing a project only removes its registry entry.
- Show project name, path, Git branch, clean/dirty state, upstream relationship, last successful fetch, last repository activity, and last-opened time.
- Render `README.md`, `TODO.md`, and work history (`WORK_HISTORY.md` or `WORKING_HISTORY.md`) from either the project root or `docs/`, using case-insensitive filename matching.
- Parse TODOs and working history into validated structured records while preserving malformed or unrecognized source text.
- Show TODOs in four compact critical, high, medium, and low priority columns with category filtering; title-and-metadata cards open dependencies, rationale, and acceptance criteria in a closable detail window while validation warnings remain visible.
- Sort working history from newest to oldest; keep date/time, category, and area visible while opening summary and limitations in a closable detail window, with category and area filters.
- Allow local agents to add TODOs and propose either TODO completion or standalone working-history entries through a loopback-only API. Pending proposals stay in Projector's internal storage until the user approves or rejects them.
- Show all pending proposals in a dedicated review tab with local-only Approve and Reject actions.
- Show up to 25 recent commits.
- Run `git fetch --all --prune` in the background at startup and on manual refresh, then report ahead, behind, diverged, synchronized, or unknown status.
- Manually pull the selected project with fast-forward-only semantics when it has a checked-out upstream branch and a clean working tree.
- Refresh automatically after changes beneath a registered directory, with a manual refresh available as a fallback.
- Handle missing documents, non-Git folders, missing remotes or upstreams, authentication/offline failures, fetch timeouts, moved/inaccessible paths, and read errors without preventing access to the rest of the project view.

Projector runs only two bounded Git commands: the background fetch above and an explicitly selected `git pull --ff-only --no-rebase --recurse-submodules=no`. Pull refuses dirty working trees, detached branches, missing upstreams, and histories that require a merge or rebase. Projector never pushes, checks out branches, builds projects, provides project runtime controls, or exposes generic file or shell operations.

## Stack and design

The desktop shell is [Tauri 2](https://v2.tauri.app/) with a React and TypeScript interface built by Vite. Tauri uses the operating system webview instead of bundling a browser engine. Rust owns the local trust boundary: it validates and stores registered paths, parses and atomically updates recognized state documents, observes Git through libgit2, coordinates bounded Git fetches, and emits change events. One project-state service supplies the parser, validator, deterministic writer, concurrency lock, and mutation logic used by the desktop and local API. The UI has no general filesystem, shell, or process permission.

This split leaves a clear boundary for later services while keeping mutation authority narrow. Any future runtime-management service must be separate and explicitly permissioned; it is not present in this milestone.

## Structured project state

`TODO.md` contains open work. Each item uses this deterministic syntax:

```markdown
## TODO-001: Display Git status

- Priority: high
- Category: feature
- Area: git-observer
- Dependencies: TODO-000
- Rationale: Users should understand repository activity without opening a terminal.

-Acceptance Criteria:
Display the current branch, working-tree status, recent commits, and last repository activity.
```

IDs are stable and unique. Priority is `critical`, `high`, `medium`, or `low`; category is `feature`, `bugfix`, `refactor`, `test`, `documentation`, `research`, or `others`; dependencies are comma-separated IDs or `none`. TODOs with dependencies are shown as blocked and TODOs without dependencies as planned; status is not stored separately. A completion request leaves the TODO unchanged while Projector stores a pending proposal internally. When the user approves it, the TODO is removed, its ID is removed from remaining dependency lists as a satisfied prerequisite, and the proposed history entry reuses the TODO title, category, and area. Rejection discards only the proposal. Acceptance criteria is plain Markdown, not a checklist or percentage calculation.

`WORK_HISTORY.md` is append-oriented:

```markdown
## 2026-07-23 16:30 — Git status display implemented

- Category: feature
- Area: git-observer

### Summary

Implemented branch, working-tree, recent commit, and repository activity displays.

### Limitations

Submodule status is not yet supported.
```

Categories use the same values as TODOs. The UI displays entries newest first. Future malformed content is retained as preserved unrecognized content and exposed alongside validation warnings.

## Local agent API

While Projector is running, its JSON API is bound only to `http://127.0.0.1:48721/v1`. `GET /projects` returns registered project IDs and `GET /projects/{projectId}/state` reads structured state. The only mutations are:

- `POST /projects/{projectId}/todos`
- `POST /projects/{projectId}/todos/{todoId}/complete`
- `POST /projects/{projectId}/work-history`

Project and TODO IDs are URL parameters rather than redundant JSON fields. Adding a TODO accepts `title`, `priority`, `category`, `area`, `dependencies`, `rationale`, and `acceptanceCriteria`. Requesting TODO completion accepts only `summary` and `limitations` and returns a pending proposal with `kind: "todoCompletion"`, the `todo` snapshot, and `proposedEntry`. Proposing standalone working history accepts `title`, `category`, `area`, `summary`, and `limitations` and returns the same proposal shape with `kind: "workHistory"` and `todo: null`. Both proposal variants also contain `id`, `projectId`, and `requestedAt`.

The API rejects unknown registry IDs and never accepts a filesystem path. A completion request validates the TODO and its dependencies and rejects a duplicate request while that TODO already has a pending proposal. A standalone history request validates the proposed entry and current history document. Both store and return a proposal without changing project Markdown. Agents have no approval or rejection endpoint. The desktop's local-only approval action branches by proposal kind: TODO completion revalidates the live TODO snapshot and updates both documents with rollback, while standalone history atomically appends only its proposed entry. In both cases the persisted proposal is removed only after the project-file mutation succeeds; rejection removes only the proposal. There is no arbitrary Markdown, filesystem, shell, Git, or runtime endpoint. The HTTP contract is intentionally small so a later MCP adapter can remain a thin compatibility layer.

Agents working in registered projects must use these operations rather than directly modifying `TODO.md` or `WORK_HISTORY.md`.

## Project migration

Run the bounded migration from the Projector repository:

```powershell
cargo run --manifest-path src-tauri/Cargo.toml --bin projector-migrate
```

It scans only direct project directories beneath the approved `C:\Users\John\Projects` root and only recognized root or `docs/` state files. It skips dependency, generated, cache, virtual-environment, and Git-internal trees by never descending into them. Before changing a state or `AGENTS.md` file it creates an adjacent `.projector-backup`, converts conservatively, atomically replaces the file, parses the result with Projector's production parser, and restores the original if validation fails. Repeated runs are idempotent. Unknown values use `unknown` or `none`; original legacy text remains available in the adjacent backups. `MIGRATION_REPORT.md` records outcomes and warnings.

## Project document discovery

For each supported filename, the project root takes precedence over `docs/`:

1. `<project>/<filename>`
2. `<project>/docs/<filename>`

Supported filenames are matched case-insensitively:

- `README.md`
- `TODO.md`
- `WORK_HISTORY.md` or `WORKING_HISTORY.md`

Missing files are shown as missing. A document path that resolves outside the registered root (for example through a symlink) is rejected. Markdown is rendered as GitHub-flavored Markdown. Embedded HTML is parsed for common README layout such as `<div align="center">`, then sanitized before rendering; scripts and unsafe attributes are removed.

## Local data and access

The application stores `registered-projects.json` in the operating system application-data directory for the `com.local.projector` identifier. It contains only registry version, project ID, canonical location, display name, registration time, and last-opened time. A separate `git-sync-cache.json` stores only the last successful fetch time per project. Pending review proposals are stored in `completion-proposals.json`; each contains its kind, registered project ID, proposed history entry, request time, and, for TODO completion, the TODO snapshot needed for stale-proposal validation. This explicitly permitted internal proposal storage lets reviews survive restarts. Other project content and Git history are not copied into application storage.

Filesystem observation is created only for registered roots. Project creation accepts a user-selected parent folder and a validated single folder name; after creation and registration, document and Git requests, including pull, use the registered project ID. Git worktrees whose `.git` metadata resolves outside the registered root are intentionally not inspected in the MVP.

## Development

Prerequisites:

- Node.js and npm
- Rust and Cargo
- The platform prerequisites for Tauri 2 (WebView2 and Windows build tools on Windows)

Install and run the desktop app:

```powershell
npm install
npm run tauri dev
```

Run verification:

```powershell
npm test
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
npm run tauri build -- --no-bundle
```

The project-local `@tauri-apps/cli` is used; a global Tauri CLI installation is not required.

## Known limitations

- Documents are capped at 2 MB of displayed content. Larger files are clearly marked as truncated.
- Git history is capped at the 25 most recent commits.
- Recursive watching can consume extra operating-system watcher resources for very large directory trees. Manual refresh remains available.
- Git submodules and linked worktree metadata outside the registered directory are not traversed.
- The supported filename aliases and root-before-`docs/` lookup order are fixed for the MVP.
- The registry has no import/export, path relocation, or custom project naming yet.
- The loopback API is available only while the Projector desktop process is running and has no remote transport.
- Multi-file approval provides in-process rollback if a replacement fails; it does not claim a distributed transaction across storage devices.
- Migrated legacy entries without a source time use `00:00`; absent areas use `unknown`; absent limitations use `none`.
- This milestone has no project execution, runtime health monitoring, arbitrary file editing, MCP adapter, cloud synchronization, analytics, or team features.
