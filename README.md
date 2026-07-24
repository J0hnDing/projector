# Projector

Projector is a lightweight local desktop application for observing software projects. Register a project directory, switch between projects, read its key Markdown files, inspect its current Git state, and deliberately fast-forward a clean project from its configured upstream.

## MVP capabilities

- Register and remove local project directories. Removing a project only removes its registry entry.
- Show project name, path, Git branch, clean/dirty state, upstream relationship, last successful fetch, last repository activity, and last-opened time.
- Render `README.md`, `TODO.md`, and work history (`WORK_HISTORY.md` or `WORKING_HISTORY.md`) from either the project root or `docs/`, using case-insensitive filename matching.
- Show up to 25 recent commits.
- Run `git fetch --all --prune` in the background at startup and on manual refresh, then report ahead, behind, diverged, synchronized, or unknown status.
- Manually pull the selected project with fast-forward-only semantics when it has a checked-out upstream branch and a clean working tree.
- Refresh automatically after changes beneath a registered directory, with a manual refresh available as a fallback.
- Handle missing documents, non-Git folders, missing remotes or upstreams, authentication/offline failures, fetch timeouts, moved/inaccessible paths, and read errors without preventing access to the rest of the project view.

Projector runs only two bounded Git commands: the background fetch above and an explicitly selected `git pull --ff-only --no-rebase --recurse-submodules=no`. Pull refuses dirty working trees, detached branches, missing upstreams, and histories that require a merge or rebase. Projector never pushes, checks out branches, builds projects, or provides project runtime controls.

## Stack and design

The desktop shell is [Tauri 2](https://v2.tauri.app/) with a React and TypeScript interface built by Vite. Tauri uses the operating system webview instead of bundling a browser engine. Rust owns the local trust boundary: it validates and stores registered paths, reads recognized documents, observes Git through libgit2, coordinates bounded Git fetches, and emits change events. The UI receives structured data through a small set of Tauri commands and has no general filesystem, shell, or process permission.

This split leaves a clear boundary for later services while keeping the observation MVP small. Any future runtime-management service must be separate and explicitly permissioned; it is not present in this milestone.

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

The application stores `registered-projects.json` in the operating system application-data directory for the `com.local.projector` identifier. It contains only registry version, project ID, canonical location, display name, registration time, and last-opened time. A separate `git-sync-cache.json` stores only the last successful fetch time per project. Project content and Git history are never copied into application storage.

Filesystem observation is created only for registered roots. Document and Git requests, including pull, use a registered project ID rather than accepting a path from the UI. Git worktrees whose `.git` metadata resolves outside the registered root are intentionally not inspected in the MVP.

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
- This milestone has no project execution, runtime health monitoring, agent API, cloud synchronization, analytics, or team features.
