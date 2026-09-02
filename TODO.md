## TODO-001: Define the security and lifecycle design for a permissioned runtime-management service

- Priority: low
- Category: research
- Area: runtime-management
- Dependencies: none
- Rationale: Projector's narrow Startup launcher intentionally does not own or track processes. Stop, restart, and build controls require an explicit trust boundary, authorization model, and lifecycle owner before they can be considered for implementation.

-Acceptance Criteria:
Document the proposed service boundary, user-authorization flow, permitted operations, process ownership, restart recovery, failure handling, and test strategy. Keep the service separate from the observer and existing Startup launcher, and identify any Tauri capabilities or subprocess permissions it would require. Do not implement runtime controls as part of this research task.

## TODO-002: Add bounded health and log observation for managed project processes

- Priority: low
- Category: feature
- Area: runtime-management
- Dependencies: TODO-001
- Rationale: Health and logs are meaningful only when Projector has an approved runtime service with explicit ownership of the processes being observed.

-Acceptance Criteria:
Observe only processes started and owned by the approved runtime-management service. Require deliberate user authorization, bound log retention and resource use, represent unavailable and stale states clearly, avoid exposing secrets, and cover startup, shutdown, restart, failure, and recovery behavior with backend and UI tests.

## TODO-003: Add registry backup, restore, and registered-path relocation

- Priority: low
- Category: feature
- Area: registry
- Dependencies: none
- Rationale: Users currently have no supported way to preserve their registered-project list across installations or reconnect a registry entry after moving its project directory.

-Acceptance Criteria:
Export and restore a versioned registry-metadata format without copying project content or private application state. Validate and canonicalize every restored or relocated path, reject paths that are missing, duplicated, or outside the directory explicitly selected by the user, preserve stable project IDs where safe, and never edit or delete either the old or new project directory. Cover malformed input, conflicts, inaccessible paths, rollback, and successful relocation with Rust and UI tests.

## TODO-005: Automate signed Windows installer publication

- Priority: low
- Category: feature
- Area: release
- Dependencies: none
- Rationale: Windows release builds are currently manual and unsigned, which makes releases harder to reproduce and gives users no publisher verification.

-Acceptance Criteria:
On a versioned release, build the NSIS installer with the project-local Tauri toolchain, sign it using credentials supplied only through the release environment, verify the signature before publication, and publish the installer with checksums and release notes. Keep pull requests and untrusted builds unable to access signing credentials, document certificate rotation and release recovery, and test the workflow without requiring production credentials.
