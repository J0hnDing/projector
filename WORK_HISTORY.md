# Working history

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
