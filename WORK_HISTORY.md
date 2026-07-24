# Working history

## 2026-07-23 — Manual fast-forward pull

- Added a per-project Pull button backed by a registered-ID-only, 30-second `git pull --ff-only` workflow.
- Pull now refuses dirty working trees, detached branches, missing upstreams, concurrent Git synchronization, and histories that cannot fast-forward.
- Added backend and interface tests for pull eligibility, invocation, and safe failure messaging.

## 2026-07-23 — Contributor guidance

- Added repository-wide `AGENTS.md` guidance covering Projector's architecture, observation boundary, narrow Git fetch exception, verification gates, and documentation expectations.

## 2026-07-23 — Git synchronization awareness

- Added non-blocking `git fetch --all --prune` at startup and on manual refresh, with duplicate-fetch suppression and a 30-second timeout.
- Kept cached local Git information visible while fetches run and pushed completion updates to the UI.
- Added upstream ahead/behind/diverged/synchronized classification, persisted last-success timestamps, and graceful remote, authentication, offline, timeout, and missing-upstream states.
- Added Rust tests for status classification and fetch failure handling.

## 2026-07-22 — Initial MVP

- Bootstrapped Projector as a React, TypeScript, and Tauri 2 desktop application.
- Added a local JSON registry containing only canonical project locations and app metadata.
- Added native directory registration and safe removal that never deletes project files.
- Added root/`docs/` discovery and rendered reading views for `README.md`, `TODO.md`, and `WORK_HISTORY.md`.
- Added read-only libgit2 observation for branch, clean/dirty state, recent commits, and last activity.
- Added recursive filesystem notifications with debounced refresh and a manual refresh fallback.
- Added explicit missing, inaccessible, non-Git, and oversized-document states.
- Restricted the frontend capability set to core window behavior and native folder selection; no filesystem, shell, or process plugin is enabled.
- Added Rust tests for registry, document, Git, and error behavior plus React tests for registration and reading flows.
- Added case-insensitive document discovery and support for both `WORK_HISTORY.md` and `WORKING_HISTORY.md`.
- Added sanitized embedded HTML rendering for common README layout markup.
- Configured Windows release builds as GUI applications so they do not open a console window.
