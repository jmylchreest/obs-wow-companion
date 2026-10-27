# Changelog

## Unreleased

### Added

- Add macOS to source CI and exercise FFmpeg/ffprobe lookup, explicit Unicode paths, missing tools and real media operations on Linux, Windows and macOS.

- Clear log beside Collapse in Events removes saved diagnostic history while new events continue to arrive.
- Export project aide decisions before commits through cargo-husky, with a matching CI check for machine-specific paths.

### Changed

- Serve six focused player guides as HTML on the website and in future release archives. Consolidate contributor documentation, archive old development journals and correct outdated setup and validation instructions.

### Fixed

- Make the fake OBS recovery timer advance with elapsed time so slower CI runners exercise the same ownership checks reliably.

- Preserve verified OBS output identity after the first recording fragment is cleaned up, so later splits still save a recoverable ownership checkpoint. Log post-roll stop decisions and ownership rejection reasons.

- Resume an active key after a crash by replaying missed combat-log events. Finish completed or abandoned keys correctly, and wait for startup log replay before stopping verified Companion-owned OBS capture.

- Stop a verified Companion-owned OBS recording after restart when its interrupted key is abandoned, whichever startup event arrives first.

- Version public image and video URLs after captures so returning visitors receive the updated footage.

- Use Ruby Life Pools footage for the public screenshots and demo, matching the selected recording.

## 0.1.0-dev.1 — 2026-10-27

### Changed

- Match adjacent buttons and inputs in Settings, export controls, marker entry, the events toolbar and the labels footer.

- Compact settings fields and installation rows, with automatic-recording controls grouped beneath each installation.

- Simplify automatic recording preferences per installation: raid minimum difficulty or Off, Mythic+ minimum level or Off, and delves off by default.

- Let the death list fill the review pane, with Export and Video details kept above the labels footer.

- Align the recording filters to the right and add a matching protected-recordings toggle.
- Use consistent dark scrollbars throughout the app, leave room beside recording actions, and replace the events footer label with an accessible expand/collapse button.
- Keep recovery actions beside the status heading, with a full-width progress bar below that gives way to the file review panel when finished. Remove banner dismissal and event shortcuts; empty unrecoverable files offer confirmed deletion, with cleanup policy shown per file.

- Recover closed, unindexed footage using verified timing; show a recovery panel, retain retryable errors and include orphaned files in storage accounting.
- Use typed connection, credential, recovery and encoding progress data with a shared English message catalogue and locale-aware formatting.
- Seek near the clip start when locating keyframes in long source recordings.
- Distinguish unrecoverable recordings from footage still awaiting recovery. Explain missing footage, preserve encounter details, and allow checking again after restoring source files.

### Fixed

- Verify the Windows installer download against its embedded SHA-256 checksum before extracting or running it. Test both installers against a modified download before installing the genuine package.

- Retain the paused video frame when switching between Recordings and Settings.

- Read OBS audio track names stored in NAME metadata, including existing recordings.
- Clear the attention banner when no unresolved file or retryable recording issues remain, including stale results saved by older builds. Keep historical unrecoverable statuses on their recordings.
- Show OBS as offline when its WebSocket server is unreachable, suppress stale password-store warnings, and show the automatic retry countdown.
- Retain OBS connection errors in the event log and status pill, and show automatic recording as blocked while OBS or setup is unavailable.
- Replace the native audio-track dropdown with a compact player button and radio menu matching the marker filter.
- Add familiar player keyboard shortcuts for playback, seeking, mute, volume, speed and fullscreen; reserve M for mute instead of recording.

- Ignore generic OBS audio-handler metadata in track names, preserve case-insensitive track titles, and clarify the wait for OBS to finish saving footage.

- Replace fragment-based Live review with Stop recording for automatic encounters, retaining captured footage and allowing later encounters to record.

- Show a red D for failed or abandoned Mythic+ recordings even when upgrade timing was not recorded.

- Give native test sessions private runtime/config directories so D-Bus-activated services cannot replace desktop credential-agent sockets.

- Bound Linux CI dependency installation and avoid optional recommended packages after a slow mirror exhausted the packaging job.

- Discard cancelled OBS requests before sending them, preventing an expired queued recording command from executing later.

- Publish complete initial UI/API state before opening the window, fixing a Windows startup race while OBS reconnects.

- Retain delayed encounter completions after OBS stops or disconnects, recovering closed footage where available and showing unavailable metadata otherwise. Unrelated delayed events no longer postpone an owned recording's stop.

- Preserve source timestamp precision during precise exports, avoiding a lost frame at each fragment boundary with FFmpeg 6.
- Read and write validation files explicitly as UTF-8 on Windows.

- Preserve native OBS probe output on timeout and include nested timing/coverage reports in CI artifacts.

- Correct Windows validation fixtures for event-log resizing, native versus Wine paths, and observing in-progress ZIP writes through a file handle.

- Preserve the available part of a delayed encounter when its start is outside retained footage, marking the export truncated instead of dropping the encounter.

- Recover delayed encounter starts when the current OBS session retains footage covering the start, without starting a new recording for historical log events.

- Fix the Rust 1.95 lint failure found by initial Linux/Windows CI; extend Windows validation to native window-close and credential-store round trips.

- Quit and window-close now stop owned capture and cancel unfinished encoding automatically, retaining queued clips for restart without requiring Pause or waiting for a full export.

- Wait for OBS stop confirmation before asserting scene restoration in the native integration probe; early fragment exports can become ready before recording stops.

- Restore the crash-recovery smoke test’s held-worker injection after FFmpeg progress options changed argument ordering; align cargo-dist Linux D-Bus build dependencies with CI.

- Validate shared videos by full decoding, duration and audio-track checks before publishing a player ZIP; damaged media leaves no final bundle.

- Drain unread combat-log batches before applying wall-clock inactivity expiry, preventing false abandonment while the watcher catches up.

- Keep tracked encounters in the library when OBS disconnects before completion, with an interrupted-recording warning instead of silently dropping the encounter.
- Retain combat-log checkpoint acknowledgements after database write failures and retry automatically, saving pending encounters with the cursor before allowing watchers to advance.

### Added

- Add a player-focused quick start, testing and bug-report forms, preview release notes and refreshed public screenshots with a privacy-obscured gameplay demo.

- Add experimental Retail delve recording from zone entry to exit, with death and boss markers, a Delves filter and a DV title prefix. Existing installations can enable it in Activity filters; tier and completion are not inferred.

- Attach raw log timestamps, normalized event times, receipt times, file identities and exact byte offsets to emitted log events for timing diagnostics.

- Run interrupted-export recovery checks on Windows as well as Linux, waiting for orphan workers to finish before validating the recovered clip.

- Exact-version CI caches for Cargo validation and packaging tools, separated by OS, architecture and Rust compiler.

- Native OBS validation of delayed encounters with missing leading footage, including truncated metadata and a playable partial clip.

- Native combat-log file notifications with bounded wakeups and periodic reconciliation when notifications are missed or unavailable.

- Native OBS regression for a 30-second delayed encounter start, retained pre-roll and automatic recording completion.

- Player-focused getting-started README and responsive project website, with actual app screenshots and a silent demo using fictional encounter data.

- Open the original recording or latest shared bundle folder directly from desktop video details, with errors for missing files.

- Cancellable JPEG thumbnail generation and a poster image in exported web-player bundles.

- Cancel preparation for automatic clips, retaining a durable retryable plan and source footage without silently starting encoder fallback.

- Cancel export for player bundles, with cancellation during media preparation and ZIP copying, protected source footage until completion, and retry without replacing an earlier successful bundle.

- Native ZIP destination picker, embedded audio track names, a reproducible three-track demo, and persistent latest-bundle details alongside original video metadata tables. Export now starts collapsed.

- Standalone player ZIP export to a chosen disk path, with selected-track MP4, sanitized versioned metadata, offline player assets, precise range support and progress. ZIPs publish atomically without overwriting existing files.
- Dynamic activity/outcome/difficulty/key filters, local-date From/To filters and a separate sort row.

- Marker filter menu with dynamically discovered event categories, Toggle all and saved category preferences.

- Early, continuous playback from closed retained fragments while final export runs. Compatible video/audio is copied; the player keeps its source, position and speed when export completes.

- Cursor-centred mouse-wheel video zoom (up to 6×), drag to pan and a Reset control. Timeline and controls remain unscaled.

- Compact status footer and searchable event viewer with watcher health, abandonment countdowns, OBS lifecycle events, saved rotating event logs and a Save log action. FFmpeg exports and playback copies report processed video time and speed, sampled into the event log every five seconds.

- Death recaps show the last three observed incoming hits within ten seconds, including ability, attacker and time before death. Recaps persist in recording metadata and trimmed clips; older recordings show “Not recorded”.

- Coloured, fixed-width recording title prefixes for Mythic+ key levels, retail raid difficulty (` M`, `HC`, `NM`, `RF`) and manual recordings (`--`).
- Per-clip waiting reasons and live export stages with elapsed time, including explicit guidance when an observe-only OBS recording prevents clip completion.

- Advanced export settings: fast lossless copying for automatic clips and CPU, NVIDIA NVENC, Intel Quick Sync, AMD AMF or Linux VAAPI encoding for precise cuts. Failed copy/GPU attempts retry with CPU encoding; Video details shows the actual method and fallback reason.
- A reproducible synthetic export benchmark example.

- Show fitting labels beneath recording details, with existing-label suggestions and partial matching in search.

- Abandon an in-progress Mythic+ key without deleting footage or cancelling other active recordings.
- Named manual markers during recording and playback, atomic sidecar updates for markers and annotations, marker preservation in trimmed clips, and fullscreen playback with integrated controls.

- Manual clips collect boss start/end and player-death markers even when opened mid-dungeon. Choose the log source when multiple installations are configured.

- Independent combat-log watchers publish installation-identified lifecycle events, retaining source paths and waiting for durable checkpoints before advancing.

- Internal typed capture-event bus, with an OBS WebSocket publisher and serialized activity lifecycle handling.
- Automatic encounter recording, overlapping-activity tracking, post-event stopping, and a remembered Pause/Resume control.
- Live recording-list updates over an authenticated event stream, including active encounters and export results.

- Automatically reconnect when OBS returns, with bounded retries and session or securely stored credentials.
- Optional recording-scene selection, guarded restoration, and scene-creation instructions in Settings and the setup guide.

- Use steamlocate for portable Steam installation discovery (including Windows registry, Linux native, Flatpak and Snap locations), secondary libraries and Proton prefixes, and follow Battle.net recorded WoW paths including Wine drive mappings. Prepare standard macOS discovery paths.

- Discover WoW installations in common locations or a chosen folder; add and remove multiple installations during onboarding, with duplicate-folder protection.

- Save authenticated OBS passwords in the OS credential store, reuse them on reconnect and remove them from settings.

- Streamlined everyday UI: one connection setup flow, contextual capture and recording actions, automatic playback preparation, expandable tuning/filter/export controls, and preference save feedback.

- Startup OBS readiness detection and a four-step setup wizard covering connection, recording settings, media verification and WoW logs.

- Rust desktop companion for separately installed OBS Studio.
- Combat-log activity detection, manual intervals and retrospective clip planning.
- Authenticated OBS control with conservative recording ownership and segmented capture.
- Local SQLite library, tags/protection, filtering, playback previews and trim exports.
- Multiple audio track preservation, precise re-encoding and synthetic media diagnostics.
- OBS setup guidance, native starter configuration and isolated integration probes.

- Windows/Linux CI with native package verification, installer smoke tests, reports, dependency checks and explicit release gates.

### Changed

- Replace separate date inputs with one calendar range picker, draft selection with Apply, Escape/outside-click cancellation, and one-click reset to All dates.

- Make the player audio-track dropdown visible as a selector; preserve playback position, speed and play/pause state when changing named audio tracks.

- Simplify export into one panel with a full-width destination and its own Export ZIP action; mix selected audio tracks into one output track, or export silently. Match header status pill dimensions and show plain key-level numbers in filters.

- Main views use underlined tabs; automatic recording is a prominent header switch, separate from Record now in the library.
- Recording and review panels scroll independently with pinned library filters, a bounded death list and a compact non-overlapping label/Delete footer.
- Remove the redundant recent-hit tooltip; document current UI tooltips in docs/ui-tooltips.md.

- Show event ages and inactivity countdowns in compact day/hour/minute/second units.

- Move capture status exclusively to the footer and manual Record now/Stop recording plus Pause/Resume controls into the recordings pane.

- Events expands into an embedded footer panel aligned with the main workspace, with a draggable/keyboard-resizable separator and an initial height below the video.

- Automatic exports now default to Fast copy, including a one-time profile migration. Subsequent explicit cut-mode choices and existing queued jobs are preserved.

- Death table and death-marker clicks seek five seconds before the death, clamped to the retained video start.

- Default inactivity timeout is five minutes. Mythic+ inactivity tracks group actions and encounter boundaries rather than background aura traffic; confirmed dungeon exits still use 60 seconds. Existing 30-minute defaults migrate once, preserving other custom values.

- Join the recordings and review panes with a draggable, keyboard-accessible divider, saved pane width and collapse state, and reduced window padding.
- Simplify OBS status details to actionable issues or a single Ready message, without repeating connection success.

- Stack favourite and protection controls vertically beside recording titles to leave more room for names.

- Fast exports concatenate compatible continuous OBS fragments directly, avoiding per-fragment temporary copies. Stream layout and codec configuration must match; final duration, audio tracks and a full decode are still validated before publication.

- Remove the in-app synthetic media test, generated tones, test player, diagnostic API and diagnostic state. Automated playback checks create silent fixtures outside the app.

- Align raid K/W and Mythic+ upgrade/deplete badges beside recording durations. Preserve logged keystone elapsed time and the dungeon timer used to determine upgrades.
- Move Delete into the review footer and add per-recording trash buttons, with confirmation or Shift-click to delete immediately.

- Add a player eye button to show or hide event signposts, enabled by default and saved in app configuration across restarts.

- Show original/compatible playback status and fallback reasons, with a Video details panel for container, codecs, resolution, frame rate, pixel format, audio tracks, duration and size. Copy AAC audio unchanged when building compatibility previews.

- Unify player typography and volume/time spacing, stack overlapping signposts, and keep event labels separate from seek-time tooltips.
- Add a default-open export panel with draggable timeline range handles, precise exports and right-aligned recording actions. Apply consistent panel styling to disclosures.

- Replace the separate playback-button strip with a bundled Video.js control bar. Anchor event signposts and combat colours directly to its seek track, including fullscreen and resized views.
- Show boss fights in red and sampled group combat activity in orange; non-boss timing is explicitly estimated.

- Compact recording library with heart and lock toggles, integrated playback controls, clickable combat/death timeline markers, a deaths table and automatically saved labels.
- Keep the OBS health pill status-only; rerun setup from Settings.

- Remove mandatory picture/audio confirmation and allow rerunning setup with current settings.

- Simplify installation setup, keep Finish setup in the navigation footer, and take users directly to any missing confirmation. Show connection messages only on the Connect step.

### Fixed

- Timeline markers reuse the lowest free lane and recalculate after layout; separated markers no longer form a staircase.

- Accepted desktop Quit now sends a direct native close acknowledgement; remaining HTTP connections get at most two seconds to drain. Only browser mode displays instructions to close the window manually.

- Quit and native window close now terminate the live event stream instead of waiting indefinitely for it during server shutdown. The desktop receives an explicit service-completion event instead of polling for it.

- Pause video when opening Settings and refresh the paused frame when returning, retaining its source, position and playback speed without reloading.

- Reset the video element before loading a compatibility copy after original-format rejection, avoiding an unloaded replacement source in WebKit.

- Abandon an unfinished Mythic+ run after 60 seconds outside its dungeon, cancelling the grace period on return and stopping owned recording through the normal post-roll flow.

- Keep the player play/pause icon and accessible label synchronized with the current video when switching recordings, including switching away during playback.

- Persist companion recording checkpoints and resume a verified owned OBS recording after reconnect or app restart, including its current fragment and post-roll stop. Reject changed outputs, reset counters, pauses and uncertain in-flight splits.
- Avoid indexing OBS's original output filename twice after a split/reconnect.

- Hide video controls, play overlay and timeline until the selected video is playable. Pending encounter clips no longer claim their source file is still open based only on OBS recording activity.

- Show pending clips waiting for an open OBS source, reset stale player media on selection, and load playback automatically when export finishes.
- Remove the duplicate OBS hover tooltip; show status details only when the pill is clicked.

- Play original recordings directly when supported; copy compatible H.264 video into fallback MP4 previews instead of re-encoding it.
- Coalesce repeated playback selections and wait for in-flight preview work without showing duplicate-request errors.
- Remove redundant recording-end signposts while retaining boss-end events.

- Persist manual-clip start and finish transactionally; recover interrupted manual clips as abandoned without reviving finished clips.
- Retry sidecar updates durably after crashes and repair stale or missing sidecars on restart.
- Make the process-kill export test interrupt encoding rather than the FFmpeg startup check.

- Sort active and saved recordings together by start time, and show the saved reason for unavailable clips.

- Reuse securely saved OBS credentials on startup and wait for readiness before opening setup.

- Make the generated playback test optional troubleshooting, show progress and results, and let existing OBS users confirm picture/audio without running it.

- Make Next on the OBS connection step validate, connect and securely save the entered password just like Check again, instead of skipping the action.

- Simplify recording setup to two numbered checks and one explicit Change folder action; remove redundant confirmation and decorative dots.

- Replace descriptive setup cards with actionable fixes, compact passed checks and conventional settings labels.

- Use one connection check in guided setup, show connection success and step status, and advance after connecting while preserving manual navigation.
- Remove em dashes from application UI copy.

- Honour desktop/compositor window-close requests as normal quits, with capture/export guards, instead of minimising.

- Use the current split file when OBS reports the original filename on recording stop.
- Retain trim source plans and precise export settings across interrupted jobs.
- Reject newer database schemas without modifying them.
- Attach the Linux webview to the desktop content container.
- Detect Windows log rotation with stable file IDs to prevent rereading appended logs.
- Encode joined audio once in precise exports to avoid accumulating delay at segment boundaries.
- Validate existing export files before recovery and clean temporary files after failed exports.
- Isolate retry output paths from media workers surviving an application crash.
- Preserve trailing backslashes in Windows startup directory arguments.

This development version has not passed all public-release gates; see the validation document.
