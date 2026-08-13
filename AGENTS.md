# Projector contributor guidance

This file applies to the entire repository.

## Product contract

Projector is a lightweight, local Tauri desktop application for managing and observing registered software projects. Keep the interface focused on rapid project switching, documentation, structured TODOs, working history, and Git awareness.

Preserve these boundaries:

- Project files remain the source of truth.
- Store only registered project metadata, preferences, Git fetch timestamps, and pending review proposals in Projector's application-data directory.
- Access only directories explicitly registered by the user. After registration, frontend commands should identify projects by registry ID rather than accept arbitrary paths.
- Canonicalize paths and reject document or Git metadata that resolves outside a registered root.
- Removing a project must only remove Projector metadata; never delete or edit the project directory.
- Do not introduce a separate current-state document or status field. Infer status from documents and Git.
- Do not add cloud synchronization, analytics, Kanban features, generic document editing, project execution, or team features unless a later milestone explicitly requests them.
- Projector is authoritative for structured TODO and working-history mutations. Agents use the narrow loopback API; they must not directly edit `TODO.md` or `WORK_HISTORY.md`.

Project execution is out of scope for the current MVP, but it is a valid future product direction. Do not add start, stop, restart, build, test, or other runtime controls as an incidental change. A future milestone may add them through a separate, explicitly permissioned service with clear user authorization, lifecycle ownership, failure handling, and tests.

The allowed subprocesses are the existing bounded background fetch:

```text
git fetch --all --prune
```

and the explicit manual fast-forward pull:

```text
git pull --ff-only --no-rebase --recurse-submodules=no
```

Keep fetch non-blocking, deduplicated per project, limited to 30 seconds, and failure-tolerant. Keep pull limited to 30 seconds, deduplicated against other Git synchronization for the project, and available only for a registered repository with an attached upstream branch and clean working tree. Pull must never merge, rebase, recurse into submodules, or prompt for credentials. Push, checkout, broader Git workflows, and runtime management require a separate explicit product decision and narrowly scoped permission and safety design. Do not obtain Git capability by adding general shell or process permissions to the Tauri frontend.

## Architecture

- `src/` contains the React and TypeScript reading interface.
- `src/api.ts` is the frontend boundary for Tauri commands and events.
- `src/types.ts` mirrors Rust response contracts.
- `src-tauri/src/lib.rs` owns command orchestration and application state.
- `src-tauri/src/registry.rs` owns registry validation and persistence.
- `src-tauri/src/observer.rs` owns bounded document and libgit2 observation.
- `src-tauri/src/project_state.rs` owns the shared TODO/history parser, validator, deterministic writer, and mutation service.
- `src-tauri/src/agent_api.rs` exposes that service through the loopback-only agent API.
- `src-tauri/src/migration.rs` owns the bounded direct-child project migration and `AGENTS.md` updates.
- `src-tauri/src/git_sync.rs` owns the narrow background fetch, manual fast-forward pull, and fetch cache workflows.
- `src-tauri/src/watcher.rs` owns registered-root filesystem notifications.
- `src-tauri/capabilities/default.json` must remain minimal.

Keep slow filesystem, Git, and subprocess work off the UI thread. Prefer cohesive changes within these existing modules over new layers or broad restructuring.

Rust models serialize with camel-case field names. Keep `src/types.ts`, Tauri command arguments, emitted event payloads, tests, and Rust models aligned whenever a contract changes.

## Document handling

Recognize filenames case-insensitively with project-root precedence over `docs/`:

- `README.md`
- `STARTUP.md`
- `TODO.md`
- `WORK_HISTORY.md`
- `WORKING_HISTORY.md`

Preserve the 2 MB display limit and clear missing, truncated, inaccessible, and invalid-path states. Markdown may render embedded HTML only through the existing raw-parse-then-sanitize pipeline. Do not allow scripts, event handlers, unsafe URLs, or unsanitized HTML.

TODO and working-history mutations must go through `ProjectStateService`, use atomic replacement, and remain serialized by its write lock. Preserve malformed source content and surface validation warnings rather than silently dropping it.

## Development commands

Use PowerShell syntax in repository instructions. Use the project-local Tauri CLI; do not require a global installation.

```powershell
npm install
npm run tauri dev
```

Relevant verification:

```powershell
npm test
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml
npm run tauri build -- --no-bundle
```

Build the Windows installer when packaging or Windows release behavior changes:

```powershell
npm run tauri build -- --bundles nsis
```

Release builds must retain the Windows GUI subsystem setting in `src-tauri/src/main.rs` so opening Projector does not create a console window.

## Testing expectations

- Add or update Rust tests for registry, path-boundary, document, Git, watcher, fetch, and pull behavior.
- Add or update Rust tests for structured parsing/writing, dependency validation, API mutations, rollback, concurrency, and migration behavior.
- Use temporary directories and local test repositories. Tests must not depend on live remotes, credentials, or internet access.
- Add or update React Testing Library tests for user-visible states and interactions.
- Cover graceful failures as well as successful paths.
- Run focused checks while iterating, then run the relevant full frontend and Rust gates before completion.
- Update `package-lock.json` or `src-tauri/Cargo.lock` whenever dependencies change.

Do not edit generated output under `node_modules/`, `dist/`, or `src-tauri/target/`.

## Documentation and history

- Keep `README.md` aligned with supported behavior, storage, security boundaries, setup, and known limitations.
- Record unfinished or intentionally deferred work in `TODO.md`.
- Record notable completed behavior changes in `WORK_HISTORY.md`.
- Keep runtime management and broader Git workflows beyond fetch and fast-forward-only pull documented as future, permissioned work until a milestone authorizes their implementation.

Preserve unrelated user changes in the working tree and keep each change tightly scoped to the request.

## Projector

Projector is authoritative for structured TODO and working-history mutations. Do not directly edit `TODO.md` or `WORK_HISTORY.md`.

Use Projector's local API at `http://127.0.0.1:48721/v1`. Projector must be running; resolve the registered project ID with `GET /projects`.

- `POST /projects/{projectId}/todos`: `title`, `priority` (`critical|high|medium|low`), `category` (`feature|bugfix|refactor|test|documentation|research|others`), `area`, `dependencies` (TODO ID array), `rationale`, and `acceptanceCriteria`.
- `POST /projects/{projectId}/todos/{todoId}/complete`: `summary` and `limitations`; returns a pending proposal with `id`, `projectId`, `requestedAt`, `kind: "todoCompletion"`, the `todo` snapshot, and `proposedEntry`.
- `POST /projects/{projectId}/work-history`: `title`, `category`, `area`, `summary`, and `limitations`; returns a pending proposal with `id`, `projectId`, `requestedAt`, `kind: "workHistory"`, `todo: null`, and `proposedEntry`.

Use `POST /projects/{projectId}/todos` to record unfinished actionable work. Use `POST /projects/{projectId}/todos/{todoId}/complete` when that TODO is finished. Use `POST /projects/{projectId}/work-history` only for notable, independent, completed work that was not represented by an open TODO. 
Send JSON with camel-case field names. Use an empty array when a TODO has no dependencies and use `none` when there are no known limitations.
## Subagents

Use subagents ONLY for independent, bounded work where delegation materially
reduces wall-clock time, isolates substantial context, or enables useful
parallel execution.

Do not spawn a subagent merely to offload a task the main agent can complete
directly with the context it already has. Prefer the main agent for small,
single-file, tightly coupled, or low-overhead tasks.

Use the built-in `explorer` for independent, read-only codebase investigation.
Use the worker tiers below for implementation work.

Select the appropriate worker tier:

* `worker_low`: mechanical, localized, low-risk changes with an obvious solution.
* `worker_medium`: standard feature or bug-fix work requiring investigation and tests.
* `worker_high`: complex, coupled, ambiguous, or high-risk implementation.

The main agent may spawn multiple instances of the same worker role. Run workers
in parallel only when their tasks are independent and their file ownership does
not overlap.

When delegating, always select an explicit worker tier. Never use default worker. Never omit agent_type; if unsure, use worker_medium.

Give each worker a clear objective, scope, constraints, acceptance criteria,
and owned files. Workers must report work completed, files changed, validation
run, assumptions, blockers, and remaining risks.

The main agent owns decomposition, architecture, worker selection, integration,
diff review, conflict resolution, final testing, and the final response.

Do not delegate when the expected context packaging, review, or integration
overhead is comparable to completing the task directly.
