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

## Recording recovery follow-up (2026-10-27)

Track each item separately; implementation is complete only after its acceptance checks pass.

- **R1 Connection health (in progress):** Preserve reachability, credential-store, authentication, disconnect and status-request errors in the event log and status snapshot. Deduplicate repeated failures, redact passwords, clear current errors on successful connection, and show automatic recording as blocked without changing its saved preference. Verify failed connection → retry → healthy connection and UI propagation.
- **R2 Durable source timing (pending):** Persist file identity, ownership and wall-clock/media-time anchors across split, shutdown and restart. Reconcile closed, unindexed files using durable provenance first; embedded UTC creation time and OBS logs are fallback evidence. Record timing confidence; reject ambiguous clocks, pauses and discontinuities rather than guessing.
- **R3 Recover encounters (pending):** Re-plan unavailable encounters against reconciled sources, preserve marker alignment and all audio tracks, and publish clips only after validation. Recovery must be idempotent and leave original files intact until successful. The two runs reported on 2026-10-27 are a local validation case; no personal media goes into CI.
- **R4 Orphan accounting and retention (pending):** Include unindexed spool files in usage totals. Pin candidates needed by recovery or exports. Delete only proven Companion-owned, closed, unreferenced sources under retention rules; report unknown files for review. Verify active files and unrelated recordings survive.
- **R5 Recovery progress UI (pending):** Show a compact, non-modal panel beneath the header, constrained to workspace width, only while recovery is running or needs attention. Publish job stage, measured file/recording counts and affected activity IDs through the existing event/status stream. Use determinate progress only when totals are known; otherwise show the operation, elapsed time and waiting reason. Provide View events, Retry for recoverable failures and a dismissible completion summary. Recording rows and the selected viewer must reflect Checking footage, Recovering, Waiting for OBS to finish saving, Ready, Partial footage, Unrecoverable or Needs attention from the same backend job state. Preserve errors until acknowledged; restart must resume from durable state without stale progress. Verify small windows, long lists, reconnect, completion and failure.
- **R6 Failure explanations and qualification (pending):** Replace generic unavailable reasons with missing source, uncertain timing, gaps or connection failure where evidence supports it. Keep unavailable as the compatible stored lifecycle value; a persisted recovery result of unrecoverable means the checked recording folder has no usable complete source. Explain what is missing, retain encounter metadata and allow a recheck after restoring footage. Unverified timing, offline OBS and export errors remain retryable and must not be labelled unrecoverable. Test restart during capture, OBS close/reopen, missing/locked credentials, missed split events, partial footage, recovery crashes and cleanup after recovery.

Record commands and evidence in validation.md. Keep recovery progress separate from existing release gates.

Implementation checkpoint (2026-10-27): typed connection, credentials, automatic recording, readiness, processing, media progress, asset and recovery codes are in place. Recovery now persists anchors and job results, reconciles closed files conservatively, retries clips, reports unrecoverable versus uncertain results, and accounts for orphaned files without acquiring their cleanup ownership. The header panel, per-recording status and Advanced recheck action are implemented. Both reported encounters recovered in an isolated copy. Full localization of legacy/static UI prose and broader cross-platform recovery qualification remain separate follow-through; the catalogue currently covers the migrated statuses, progress, recovery and selected formatting paths. See validation.md for measured coverage.


## Status and localization review (2026-10-27)

This is a source review, not a completed localization migration. Implement the following as one coordinated contract/presentation pass with the recovery work.

### Findings and required changes

1. **Behavior depends on English wording.** ui/setup.js compares full credential-status sentences to decide whether to show notices and password-saved success. Replace credential messages with a typed state plus parameters and optional diagnostic details; never branch on rendered text.
2. **Translated option labels would change filter values.** ui/index.html outcome options omit explicit values. Keep canonical outcome codes in value attributes and localize only labels. Apply this rule to every generated option and accessibility control.
3. **Statuses are only partly typed.** Domain Kind/Outcome/Coverage enums are useful foundations, but readiness Check/Report and Library::fail accept arbitrary status strings. Engine processing stages and preview/share state are ad-hoc JSON. Introduce separate enums for recording lifecycle, processing/recovery stage, connection health, credential state, readiness severity and automatic-recording readiness. Keep persisted/wire snake_case values compatible and test unknown/legacy data.
4. **Presentation is embedded in backend data.** Engine processing_status emits title/detail English; media progress transports a formatted String; Activity warnings and event-log entries store rendered prose. Use stable message IDs plus typed parameters and numeric progress fields. Keep redacted technical diagnostics separate. Historical free-text warnings/logs need a legacy fallback, not reverse parsing.
5. **No shared message catalogue.** ui/app.js label() derives words by replacing underscores and capitalizing. Static HTML, tooltips, ARIA labels, player buttons and share/index.html also contain separate English literals. Use one English catalogue and formatter across desktop UI and exported bundles, with safe text insertion, complete-message interpolation and missing-key fallback. Do not translate protocol enums, OBS commands, paths, player names or user labels.
6. **Grammar and formatting are inconsistent.** setup discovery appends an English plural s; review audio summaries branch on one/many; diagnostics builds English ago sentences and d/h/m/s units. Decimal displays use toFixed. Introduce explicit locale-aware number/date/collation helpers and plural-aware whole messages. Keep media timecodes and machine numeric inputs separate from localized display values.
7. **Calendar mixes localized dates with English layout.** date-range.js localizes month/date labels but index.html fixes Mon–Sun headings and the calendar assumes Monday first. Establish an app locale and calendar/week-start policy; keep ISO date-only storage and test date-boundary behavior.
8. **Recovery time basis needs explicit provenance.** wow-log parsing uses chrono::Local and rejects ambiguous/nonexistent local times. An archived log parsed in another timezone can map to the wrong video position. Persist the relevant timezone/offset or reliable anchors and surface uncertainty. Preserve the documented combat-log wire timestamp grammar; never switch protocol parsing to UI locale.
9. **Long text and RTL need qualification.** Fixed-width player controls, badges, truncated recording titles, physical left/right CSS and directional icons need pseudo-localized and RTL tests. Truncation must retain accessible full labels. Locale switching must invalidate rendered-label caches.
10. **Tests currently assert English presentation.** Separate state/contract assertions from catalogue text assertions. Include a deliberately altered English catalogue or pseudo-locale to prove control flow does not depend on text.

### Acceptance checks for the coordinated pass

- Every status variant maps to a message; unknown variants and old sidecars/log entries degrade safely.
- Changing a message or locale cannot alter filtering, retry eligibility, saved-password handling, ownership or capture behavior.
- Progress exposes phase, completed/total, elapsed time and affected activity IDs without parsing prose.
- Application and exported bundle use matching catalogue keys and formatter behavior offline.
- Test singular/plural/zero counts, decimal-comma locale, composed/decomposed Unicode search, long translations and RTL.
- Test UTC/local boundaries, New Year, DST repeated/skipped hours and recovery on a machine with a different timezone.
- Preserve backward compatibility for database/sidecar codes and user-authored text.


## Early testing preview — 27 October 2026

The user authorised a labelled development preview while broader stable-release qualification continues. Player-facing quick start, report forms, release notes and a GitHub invitation draft are prepared; Reddit talking points are provided for the author to write and send personally. Public assets use isolated fictional metadata and privacy-obscured, muted gameplay. Local documentation, release-boundary tests and desktop/mobile site checks pass.

Publication still requires both native package jobs, bundle verification and both installer smoke tests to pass. The release workflow enforces those dependencies. Stable-release requirements above remain unchanged.

The tagged `0.1.0-dev.1` preview is now public: release run `40552480545` passed all quality, native package, bundle and installer gates. The remaining next step is collecting real-player reports, particularly Windows and experimental modes, before making broader support or stable-release claims.
