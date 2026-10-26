# Implementation plan

This is the execution checklist for the complete first public release. A completed implementation item is not automatically a passed release gate. Evidence is recorded in `docs/validation.md` as checks run.

The [release audit](release-audit.md) maps every acceptance criterion to evidence and remaining gates.

## Execution order

1. [complete] Workspace, Rust 1.95 policy, independent provenance and deterministic domain contracts established; randomized timeline checks pass.
2. [implemented; validation expanding] Parsers, bounded tailing and activity state machines work with synthetic Retail/Classic fixtures; real client fixtures remain a release gate.
3. [implemented; failure testing in progress] Authenticated OBS client, correlation/timeouts, capability checks and conservative ownership are integrated.
4. [implemented; timing qualification in progress] Real isolated OBS recording/splitting/export and two audio tracks pass; long-session frame continuity remains to be measured.
5. [implemented; quota/recovery testing in progress] SQLite library, durable plans/modes, metadata, protection and source pins are integrated. Restart and failed-trim regressions pass.
6. [implemented; UI testing in progress] Desktop interface, previews, filters, trimming, favourites, shortcuts and opt-in startup exist. Linux window and startup toggles pass; Windows runtime remains outstanding.
7. [in progress] Native OBS starter files pass clean native-load/recording tests. Setup/recovery docs exist; Linux menu-driven imports/rollback and screenshots now pass; Windows import and a new-user gameplay walkthrough remain.
8. [in progress] Explicit CI/release workflows, native Windows/Linux package and installer jobs, release-plz preparation, dependency policy, report artifacts and OBS integration canary added. Linux archive/installer tests pass locally; remote CI is active; initial failures are being resolved and Windows/clean-machine qualification remains.
9. [in progress] Current verification commands and results are recorded in validation.md; older test counts describe historical runs only. Continue failure tests and complete the requirement-by-requirement release audit.
10. [awaiting appropriate environments] Actual Windows and GNOME/KDE Wayland gameplay validation require those environments and labelled real-client logs. No stable-release claim until these gates pass.

## Design constraints

- No reduced first release: former MVP and 1.0 scope remains required.
- Upstream sources are behavioral references; new source and synthetic fixtures are independently authored. No upstream code/assets are copied.
- No publishing or external messages are needed to prepare a reviewable release candidate.
- Never mutate the user's live OBS configuration during tests. Use isolated configuration and a virtual display for integration tests.
- Prefer complete vertical slices and real failure tests over disconnected stubs.
- Public support claims require actual capture/playback validation, not just cross-compilation.

## Release gates

Record build/test commands, versions and results. Windows gameplay capture and GNOME/KDE Wayland capture need appropriate actual environments. Synthetic tests can validate parser and media logic but cannot establish in-game coverage for every client build.
