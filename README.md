# Projector

Projector is a lightweight local desktop project manager shared by people and software agents. Register a project directory, inspect structured TODOs and working history, read its project documents, observe Git state, and deliberately commit, pull, or push through narrow registered-project actions. Markdown files remain the human-readable source of truth.

## MVP capabilities

- Create Projector-ready project folders or register existing local project directories. New projects receive `README.md`, `STARTUP.md`, `AGENTS.md`, `TODO.md`, `WORK_HISTORY.md`, and three project-scoped Codex worker configurations under `.codex/agents/`, with an enabled-by-default option to initialize a Git repository; removing a project only removes its registry entry.
- Configure separate Custom instructions, Projector, and Subagents sections for `AGENTS.md`, plus worker models, reasoning effort, descriptions, and developer instructions, from the AGENTS.md Settings tab. New projects use these defaults, and users can explicitly apply them to selected or all registered projects.
- Keep registered projects in a user-defined fixed sidebar order configured from Appearance settings, while showing project name, path, clean/dirty state, simplified synchronized/unsynchronized upstream state, and last activity.
- Render `README.md`, `AGENTS.md`, startup scripts and instructions from `STARTUP.md`, `TODO.md`, and work history (`WORK_HISTORY.md` or `WORKING_HISTORY.md`) from either the project root or `docs/`, using case-insensitive filename matching. HTTP(S) links in the Startup tab open in the system default browser.
- Start a registered project only when the user presses its sidebar Start button: run every fenced `powershell` block in `STARTUP.md` in a separate visible PowerShell console rooted at the project directory, then open each HTTP(S) website link found outside code blocks. Projector does not inspect, validate, sequence, stop, or health-check those user-authored commands.
- Parse TODOs and working history into validated structured records while preserving malformed or unrecognized source text.
- Show TODOs in one compact list with priority, availability, category, and newest/oldest filters; each row opens dependencies, rationale, and acceptance criteria in a closable detail window while validation warnings remain visible.
- Sort working history from newest to oldest; keep date/time, category, and area visible while opening summary and limitations in a closable detail window, with category and area filters.
- Allow local agents to add TODOs and propose either TODO completion or standalone working-history entries through a loopback-only API. Pending proposals stay in Projector's internal storage until the user approves or rejects them.
- Show all pending proposals in a dedicated review tab with local-only Approve and Reject actions.
- Offer dark and light appearance modes from Settings, using a restrained black, grey, and white foundation. Semantic indicators retain green for available and clean states, yellow for medium priority, blocked, and dirty states, grey for low priority, and red for high, critical, destructive, and error states.
- Retain the bounded Codex Desktop lifecycle collector internally. Its project tab is temporarily hidden because a Codex Desktop lifecycle-hook bug prevents dependable status reporting.
- Show up to 25 recent commits.
- Run `git fetch --all --prune` in the background at startup and on manual refresh, then report ahead, behind, diverged, synchronized, or unknown status.
- Manually pull the selected project with fast-forward-only semantics when it has a checked-out upstream branch and a clean working tree.
- Commit all current project changes with a user-entered message through libgit2, and push only the checked-out branch to its existing configured upstream.
- Refresh automatically after changes beneath a registered directory, with a manual refresh available as a fallback.
- Handle missing documents, non-Git folders, missing remotes or upstreams, authentication/offline failures, fetch timeouts, moved/inaccessible paths, and read errors without preventing access to the rest of the project view.

Projector initializes Git directly through libgit2 only when the user selects that option while creating a project. It does not create an initial commit or remote. Commit stages all registered-project changes and creates one libgit2 commit from the user-entered message without executing hooks. The bounded Git subprocesses are background fetch, explicitly selected `git pull --ff-only --no-rebase --recurse-submodules=no`, and `git push --porcelain` for the current branch's existing configured upstream. Pull refuses dirty working trees, detached branches, missing upstreams, and histories that require a merge or rebase. Push is limited to 30 seconds and never forces, pushes tags, creates an upstream, or prompts for credentials. Projector never checks out branches or exposes generic file or shell operations. Its only project runtime control is the explicit Start action backed by registered-root `STARTUP.md`; the launched consoles remain the user's responsibility.

## Stack and design

The desktop shell is [Tauri 2](https://v2.tauri.app/) with a React and TypeScript interface built by Vite. Tauri uses the operating system webview instead of bundling a browser engine. Rust owns the local trust boundary: it validates and stores registered paths, parses and atomically updates recognized state documents, observes Git through libgit2, coordinates bounded Git synchronization, launches the narrow user-selected Startup action, and emits change events. One project-state service supplies the parser, validator, deterministic writer, concurrency lock, and mutation logic used by the desktop and local API. The UI has no general filesystem, shell, or process permission.

This split leaves a clear boundary for later services while keeping mutation authority narrow. The Startup launcher is intentionally not a runtime manager: it does not track process state or provide stop/restart controls. Any broader runtime-management service must be separate and explicitly permissioned.

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

While Projector is running, its JSON API is bound only to `http://127.0.0.1:48721/v1`. `GET /projects` returns registered project IDs and `GET /projects/{projectId}/state` reads structured state. The project-state mutations are:

- `POST /projects/{projectId}/todos`
- `POST /projects/{projectId}/todos/{todoId}/complete`
- `POST /projects/{projectId}/work-history`

Project and TODO IDs are URL parameters rather than redundant JSON fields. Adding a TODO accepts `title`, `priority`, `category`, `area`, `dependencies`, `rationale`, and `acceptanceCriteria`. Requesting TODO completion accepts only `summary` and `limitations` and returns a pending proposal with `kind: "todoCompletion"`, the `todo` snapshot, and `proposedEntry`. Proposing standalone working history accepts `title`, `category`, `area`, `summary`, and `limitations` and returns the same proposal shape with `kind: "workHistory"` and `todo: null`. Both proposal variants also contain `id`, `projectId`, and `requestedAt`.

The API rejects unknown registry IDs and never accepts a filesystem path. A completion request validates the TODO and its dependencies and rejects a duplicate request while that TODO already has a pending proposal. A standalone history request validates the proposed entry and current history document. Both store and return a proposal without changing project Markdown. Agents have no approval or rejection endpoint. The desktop's local-only approval action branches by proposal kind: TODO completion revalidates the live TODO snapshot and updates both documents with rollback, while standalone history atomically appends only its proposed entry. In both cases the persisted proposal is removed only after the project-file mutation succeeds; rejection removes only the proposal. There is no arbitrary Markdown, filesystem, shell, Git, or runtime endpoint. The HTTP contract is intentionally small so a later MCP adapter can remain a thin compatibility layer.

Agents working in registered projects must use these operations rather than directly modifying `TODO.md` or `WORK_HISTORY.md`.

## Codex Desktop hooks

Codex monitoring is opt-in and Windows-local. Projector does not edit Codex configuration. The Codex project tab is currently hidden because of a Codex Desktop lifecycle-hook bug; the bounded collector remains documented here for development and eventual restoration. To enable detection, merge the following four handlers into `%USERPROFILE%\.codex\hooks.json` while preserving any existing hooks:

```json
{
  "hooks": {
    "SessionStart": [{
      "hooks": [{
        "type": "command",
        "command": "powershell.exe -NoLogo -NoProfile -NonInteractive -Command \"$body=[Console]::In.ReadToEnd(); try { Invoke-WebRequest -UseBasicParsing -Uri 'http://127.0.0.1:48721/v1/codex/hooks' -Method Post -ContentType 'application/json' -Body $body -TimeoutSec 1 -ErrorAction Stop | Out-Null } catch {}; [Console]::Out.Write('{}')\"",
        "timeout": 3
      }]
    }],
    "SessionEnd": [{
      "hooks": [{
        "type": "command",
        "command": "powershell.exe -NoLogo -NoProfile -NonInteractive -Command \"$body=[Console]::In.ReadToEnd(); try { Invoke-WebRequest -UseBasicParsing -Uri 'http://127.0.0.1:48721/v1/codex/hooks' -Method Post -ContentType 'application/json' -Body $body -TimeoutSec 1 -ErrorAction Stop | Out-Null } catch {}; [Console]::Out.Write('{}')\"",
        "timeout": 3
      }]
    }],
    "SubagentStart": [{
      "hooks": [{
        "type": "command",
        "command": "powershell.exe -NoLogo -NoProfile -NonInteractive -Command \"$body=[Console]::In.ReadToEnd(); try { Invoke-WebRequest -UseBasicParsing -Uri 'http://127.0.0.1:48721/v1/codex/hooks' -Method Post -ContentType 'application/json' -Body $body -TimeoutSec 1 -ErrorAction Stop | Out-Null } catch {}; [Console]::Out.Write('{}')\"",
        "timeout": 3
      }]
    }],
    "SubagentStop": [{
      "hooks": [{
        "type": "command",
        "command": "powershell.exe -NoLogo -NoProfile -NonInteractive -Command \"$body=[Console]::In.ReadToEnd(); try { Invoke-WebRequest -UseBasicParsing -Uri 'http://127.0.0.1:48721/v1/codex/hooks' -Method Post -ContentType 'application/json' -Body $body -TimeoutSec 1 -ErrorAction Stop | Out-Null } catch {}; [Console]::Out.Write('{}')\"",
        "timeout": 3
      }]
    }]
  }
}
```

Review and trust the exact hook definition in Codex with `/hooks`, then start or resume a Codex Desktop session while Projector is running. The command forwards the hook JSON to `POST /v1/codex/hooks`, always emits an empty JSON object, and exits successfully if Projector is unavailable, so monitoring cannot block or steer Codex. Manual session linking is unavailable while the Codex project tab is hidden. Projector never links by working directory automatically.

Only `session_id`, `cwd`, `agent_id`, `agent_type`, receipt timestamps, and derived lifecycle transitions are retained. Projector ignores model, permission, prompt, assistant-message, transcript-path, and tool fields. Duplicate events do not create duplicate transitions; a stopped subagent ID is not reopened by a late start event. The store retains at most 50 unlinked sessions and 100 transitions per session. Linked sessions remain until they are unlinked or their project is removed.

## Project migration

The AGENTS.md Settings tab can migrate the current Custom instructions, Projector, and Subagents sections plus `.codex/agents/worker-{low,medium,high}.toml` files into selected registered projects or all registered projects. It preserves unrelated `AGENTS.md` sections, creates adjacent `.projector-backup` files before the first overwrite of an existing managed file, and refuses managed paths that resolve outside the registered project root.

The separate legacy project-state migration remains available from the Projector repository:

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
- `STARTUP.md`
- `TODO.md`
- `WORK_HISTORY.md` or `WORKING_HISTORY.md`

Missing files are shown as missing. A document path that resolves outside the registered root (for example through a symlink) is rejected. Markdown is rendered as GitHub-flavored Markdown. Embedded HTML is parsed for common README layout such as `<div align="center">`, then sanitized before rendering; scripts and unsafe attributes are removed.

The sidebar Start action recognizes non-empty fenced code blocks whose language is exactly `powershell`. Each block is passed without command validation to `powershell.exe -NoLogo -NoExit -Command` in its own new console, with the registered project root as the working directory. HTTP(S) URLs in Startup prose are deduplicated and opened afterward; URLs inside code blocks are not treated as websites. Projector does not wait for a server to become ready, and closing Projector does not stop the launched consoles.

## Local data and access

The application stores `registered-projects.json` in the operating system application-data directory for the `com.local.projector` identifier. Project array order is the fixed sidebar order; each entry otherwise contains only registry version, project ID, canonical location, display name, registration time, and last-opened time. A separate `git-sync-cache.json` stores only the last successful fetch time per project. Pending review proposals are stored in `completion-proposals.json`; each contains its kind, registered project ID, proposed history entry, request time, and, for TODO completion, the TODO snapshot needed for stale-proposal validation. User-edited Projector and subagent defaults are stored as preferences in `subagent-settings.json`; removing that override restores the bundled static defaults. The dark/light appearance choice is stored in the local desktop webview. Opt-in Codex lifecycle metadata and manual project links are stored in `codex-sessions.json` using the bounded, content-free contract above. This explicitly permitted internal storage lets preferences, reviews, and monitoring links survive restarts. Other project content and Git history are not copied into application storage.

Filesystem observation is created only for registered roots. Project creation accepts a user-selected parent folder and a validated single folder name, creates minimal README, startup, and Projector state documents, snapshots the effective static-or-user-edited Projector and subagent configuration into `AGENTS.md` and `.codex/agents/`, and can initialize in-root Git metadata through libgit2. Existing projects change only through the explicit, selection-based Settings migration action. Git initialization uses the configured `init.defaultBranch` when valid and otherwise uses `main`; it creates neither a commit nor a remote. After creation and registration, document and Git requests, including pull, commit, and push, use the registered project ID. Git worktrees whose `.git` metadata resolves outside the registered root are intentionally not inspected in the MVP.

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
- Codex hook events missed while Projector is closed cannot be replayed. Persisted running states become unknown after restart, and lifecycle hooks cannot reliably distinguish waiting, success, or failure.
- Multi-file approval provides in-process rollback if a replacement fails; it does not claim a distributed transaction across storage devices.
- Migrated legacy entries without a source time use `00:00`; absent areas use `unknown`; absent limitations use `none`.
- This milestone has no project execution, runtime health monitoring, arbitrary file editing, MCP adapter, cloud synchronization, analytics, or team features.
