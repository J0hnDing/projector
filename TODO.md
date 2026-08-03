## TODO-001: Design an explicitly permissioned runtime-management service for starting, stopping, restarting, and building projects. Keep it separate from the read-only observer and require deliberate user authorization before implementation.

- Priority: low
- Category: research
- Area: unknown
- Dependencies: none
- Rationale: unknown

-Acceptance Criteria:
unknown

## TODO-002: Add runtime health and log observation only after the runtime security model is defined.

- Priority: low
- Category: feature
- Area: unknown
- Dependencies: none
- Rationale: unknown

-Acceptance Criteria:
unknown

## TODO-003: Add registry import/export and support relocating a registered path.

- Priority: low
- Category: feature
- Area: unknown
- Dependencies: none
- Rationale: unknown

-Acceptance Criteria:
unknown

## TODO-004: Evaluate configurable document names after real-world MVP usage.

- Priority: low
- Category: research
- Area: unknown
- Dependencies: none
- Rationale: unknown

-Acceptance Criteria:
unknown

## TODO-005: Add platform release signing and automated installer publication.

- Priority: low
- Category: feature
- Area: unknown
- Dependencies: none
- Rationale: unknown

-Acceptance Criteria:
unknown

## TODO-006: Add Codex Desktop agent monitoring using hooks

- Priority: medium
- Category: feature
- Area: codex-desktop-integration
- Dependencies: none
- Rationale: Projector should observe Codex Desktop agent activity locally so users can understand which agents are active, waiting, completed, or failed without manually inspecting each task.

-Acceptance Criteria:
Define the supported Codex Desktop hook events and an explicit local setup boundary; ingest lifecycle and progress events only for user-authorized projects or tasks; display current agent state and recent transitions without adding agent control; recover cleanly from Projector or Codex Desktop restarts; avoid persisting secrets or unrestricted prompt content; and cover event validation, duplicate or out-of-order delivery, disconnects, and UI states with tests that do not require a live Codex Desktop session.
