# Projector contributor guidance

This file applies to the entire repository.

## Product contract

Projector is a lightweight, local Tauri desktop application for observing registered software projects. Keep the interface focused on rapid project switching, documentation, tasks, working history, and Git awareness.

Preserve these boundaries:

- Project files remain the source of truth.
- Store only registered project metadata, preferences, and Git fetch timestamps in Projector's application-data directory.
- Access only directories explicitly registered by the user. After registration, frontend commands should identify projects by registry ID rather than accept arbitrary paths.
- Canonicalize paths and reject document or Git metadata that resolves outside a registered root.
- Removing a project must only remove Projector metadata; never delete or edit the project directory.
- Do not introduce a separate current-state document or status field. Infer status from documents and Git.
- Do not add cloud synchronization, analytics, Kanban features, document editing, agent integrations, or team features unless a later milestone explicitly requests them.

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
- `src-tauri/src/git_sync.rs` owns the narrow background fetch, manual fast-forward pull, and fetch cache workflows.
- `src-tauri/src/watcher.rs` owns registered-root filesystem notifications.
- `src-tauri/capabilities/default.json` must remain minimal.

Keep slow filesystem, Git, and subprocess work off the UI thread. Prefer cohesive changes within these existing modules over new layers or broad restructuring.

Rust models serialize with camel-case field names. Keep `src/types.ts`, Tauri command arguments, emitted event payloads, tests, and Rust models aligned whenever a contract changes.

## Document handling

Recognize filenames case-insensitively with project-root precedence over `docs/`:

- `README.md`
- `TODO.md`
- `WORK_HISTORY.md`
- `WORKING_HISTORY.md`

Preserve the 2 MB display limit and clear missing, truncated, inaccessible, and invalid-path states. Markdown may render embedded HTML only through the existing raw-parse-then-sanitize pipeline. Do not allow scripts, event handlers, unsafe URLs, or unsanitized HTML.

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
