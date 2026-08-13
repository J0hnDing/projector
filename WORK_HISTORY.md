## 2026-07-23 00:00 — Manual fast-forward pull

- Category: feature
- Area: unknown

### Summary

- Added a per-project Pull button backed by a registered-ID-only, 30-second `git pull --ff-only` workflow.
- Pull now refuses dirty working trees, detached branches, missing upstreams, concurrent Git synchronization, and histories that cannot fast-forward.
- Added backend and interface tests for pull eligibility, invocation, and safe failure messaging.

### Limitations

none

## 2026-07-23 00:00 — Contributor guidance

- Category: feature
- Area: unknown

### Summary

- Added repository-wide `AGENTS.md` guidance covering Projector's architecture, observation boundary, narrow Git fetch exception, verification gates, and documentation expectations.

### Limitations

none

## 2026-07-23 00:00 — Git synchronization awareness

- Category: feature
- Area: unknown

### Summary

- Added non-blocking `git fetch --all --prune` at startup and on manual refresh, with duplicate-fetch suppression and a 30-second timeout.
- Kept cached local Git information visible while fetches run and pushed completion updates to the UI.
- Added upstream ahead/behind/diverged/synchronized classification, persisted last-success timestamps, and graceful remote, authentication, offline, timeout, and missing-upstream states.
- Added Rust tests for status classification and fetch failure handling.

### Limitations

none

## 2026-07-22 00:00 — Initial MVP

- Category: feature
- Area: unknown

### Summary

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

### Limitations

none

## 2026-07-23 22:40 — Structured project management and local agent API

- Category: feature
- Area: project-state-management

### Summary

Added one shared Rust parser, validator, deterministic writer, and locked mutation service for structured TODO and working-history Markdown. Added ranked TODO and dependency views, validation warnings, working-history grouping and filters, and related-TODO links. Exposed loopback-only add_todo, complete_todo, and add_work_history operations for registered projects. Migrated Eidolon, Epistome, and Projector with backups, validation, rollback, idempotency, and concise agent instructions. Verification passed: 10 frontend tests, frontend production build, 28 Rust tests, cargo fmt check, and the Tauri no-bundle release build.

### Limitations

The API is available only while Projector runs. There is no MCP adapter, remote transport, generic file editing, project execution, or arbitrary shell access. Conservatively migrated unknown fields remain explicit unknown values.

## 2026-07-24 17:47 — Compact priority TODO and chronological history views

- Category: feature
- Area: ui

### Summary

Replaced full inline TODO cards with four compact priority columns showing title and metadata, with selectable rationale and acceptance-criteria details. Removed the dependency visualization from the UI while preserving structured dependency validation. Replaced category-grouped working history cards with a newest-first title list and selectable summary and limitations details.

### Limitations

none

## 2026-07-24 22:18 — Closable TODO and working-history detail windows

- Category: refactor
- Area: ui

### Summary

Moved TODO rationale and acceptance criteria, plus working-history summary and limitations, from inline expansion into a reusable closable detail window. Working-history rows now retain their date and time, category, and area in the compact newest-first list. Added close-button, backdrop, and Escape dismissal with focused dialog behavior and updated component tests.

### Limitations

The detail window is an in-app modal rather than a separate operating-system window.

## 2026-07-27 21:52 — Unified agent API and project creation flow

- Category: feature
- Area: project-state

### Summary

Moved project and TODO identifiers into resource URLs, unified TODO and history categories, derived TODO state from dependencies, reduced completion to summary and limitations, removed related-TODO history data, and added safe UI project-folder creation with Projector instructions and empty state documents.

### Limitations

Existing structured TODO files without categories are read as others until rewritten or migrated. The running Projector instance must be restarted before the new routes are served.

## 2026-07-27 23:08 — TODO category fields and filtering

- Category: feature
- Area: ui

### Summary

Added explicit feature or research categories to every existing Projector TODO and added category filtering to the TODO page while preserving the four priority columns and detail behavior. Added frontend coverage for filtering and updated the README capability description.

### Limitations

none

## 2026-07-31 22:38 — Initialize new projects with README and optional Git

- Category: feature
- Area: project-creation

### Summary

Added an enabled-by-default, user-controlled Git initialization option to project creation using libgit2. New projects now receive a minimal README alongside AGENTS.md, TODO.md, and WORK_HISTORY.md. Git initialization respects a valid configured init.defaultBranch with a main fallback, creates no commit or remote, and rolls back the new directory if creation or registration fails. Updated the Tauri command, frontend API and dialog, documentation, and Rust and React coverage.

### Limitations

none

## 2026-08-03 00:36 — Initialize new projects with configurable subagent workers

- Category: feature
- Area: project creation and settings

### Summary

Added static subagent defaults, creation-time AGENTS.md and .codex/agents worker generation, editable global settings with backend validation and previews, and tests covering default mappings, customization, reset, snapshot behavior, and no migration of registered projects. Worker low uses gpt-5.6-luna at medium effort and worker medium uses gpt-5.6-luna at max effort.

### Limitations

none

## 2026-08-03 22:47 — Add Codex Desktop agent monitoring using hooks

- Category: feature
- Area: codex-desktop-integration

### Summary

Added an opt-in Codex Desktop lifecycle-hook integration with bounded content-free session storage, manual per-project linking, factual subagent state and transition UI, restart-safe unknown states, validation and retention tests, and documented best-effort Windows hook setup.

### Limitations

Lifecycle hooks expose only running, stopped, or unknown facts. Events missed while Projector is closed are not replayed, and waiting, successful completion, and failure are not inferred. Hook setup and session linking remain manual.

## 2026-08-09 12:44 — Added settings-driven project migration

- Category: feature
- Area: settings

### Summary

Added an editable Projector AGENTS.md section separate from Subagents settings, kept both as new-project defaults, and added explicit selected-or-all migration for managed AGENTS.md sections and worker TOMLs with registered-root validation, backups, per-project results, and tests.

### Limitations

Migration manages only the Projector and Subagents sections plus worker-low.toml, worker-medium.toml, and worker-high.toml; other AGENTS.md content is preserved.

## 2026-08-12 18:59 — Startup links open in the default browser

- Category: feature
- Area: project-documents

### Summary

Installed the Tauri opener plugin and routed HTTP and HTTPS links from STARTUP.md through a validated desktop command to the system default browser.

### Limitations

Only credential-free HTTP and HTTPS links in the Startup tab are opened externally.

## 2026-08-12 18:01 — Startup instructions tab

- Category: feature
- Area: project-documents

### Summary

Added a concise Startup tab that safely renders project-owned STARTUP.md content, discovers it from the project root or docs directory, and includes it in new-project initialization.

### Limitations

Existing projects must add STARTUP.md to the project root or docs directory.
