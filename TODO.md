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
