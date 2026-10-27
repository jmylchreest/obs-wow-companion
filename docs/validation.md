# Validation evidence

Release qualification is incomplete. These are observed results, not support claims for every desktop or game build.

| Check | Result | Environment / scope |
| --- | --- | --- |
| Workspace unit/integration tests | Passed 42 tests with nextest | Rust 1.98.1, Linux, FFmpeg 9.0.2; includes randomized timeline and backlog restart tests |
| Strict Clippy, all targets | Passed | Linux Rust 1.95 and Windows GNU Rust 1.98 cross-target, warnings denied |
| Windows-target core tests | Passed 18 tests under Wine | GNU target; stable file-ID fix; not native Windows gameplay validation |
| Windows desktop cross-build | Passed | x86_64-pc-windows-gnu; not Windows runtime testing |
| Real OBS control and export | Passed | OBS 32.2.2, WebSocket 5.7.4, Xvfb, software H.264/AAC, synthetic input |
| Two audio tracks through split and export | Passed | Manual and combat-log raid intervals across real OBS segments |
| Native Linux window | Visually checked after correcting GTK content container | X11 virtual display, WebKitGTK 2.54.1; synthetic preview displayed |
| Local service authentication | Passed | Unauthenticated requests rejected; cross-origin mutation rejected; authenticated configuration updates and graceful Quit exercised; native X11 WM_DELETE_WINDOW exits through the guarded quit endpoint |
| Dependency audit | Passed advisory, license, source and ban policy | One documented GTK build-macro maintenance exception; duplicate dependency warnings |
| Native OBS starter files | Linux native load, recording, menu imports and rollback passed | Sanitized OBS 32.2.2 serialization, 720p/30, two tracks |
| Minimum supported Rust | Passed workspace check and strict Clippy | Rust 1.95 |
| Startup registration | Passed isolated Linux enable/disable | Desktop smoke uses a private XDG config directory |
| Release packaging | Linux archive built | cargo-dist 0.32.0; docs/screenshots/templates/notices/checksum included; Windows package runtime pending |
| Multi-segment precise timing | Passed 20-segment test | 600 video frames at 30 FPS without timestamp gaps; both decoded audio tracks checked for silence at joins |
| Export failure/retry | Passed | Existing output validated; wrong duration rejected; failed-job staging cleaned |
| Process crash during export | Passed | Private app killed with a media worker outstanding; restart used a distinct attempt path, retained source and published one decodable clip |
| Lost OBS events | Passed injected receiver overflow | Disarms, preserves source, withholds uncertain footage |
| WebKit UI interaction | Passed | Offline dismissal and connected wizard completion against isolated OBS: Next validates and saves credentials, rejects incorrect passwords without advancing, visible success, automatic advance and manual step navigation, single explicit recording-folder change, manual-only choice, completion without arming or a mandatory media test; discovery and addition/removal of multiple installations; form typing, 2x playback, favourites, automatic playback on recording selection, contextual editing controls and protected-delete state |
| Secure credential storage | Passed in an isolated Linux Secret Service | Save/read/delete and port isolation; real OBS authenticated save, rejected bad replacement, reuse and deletion; Windows cross-target lint passed, native credential runtime pending |
| OBS readiness inspection | Passed | Real OBS 32.2.2 greeting, directory/splitting/source checks; inspection leaves a running recording active |
| Synthetic parser benchmark | 10,000 lines in approximately 6.98 ms | Ryzen 7 9800X3D, Linux 7.2.10, optimized build; 3,000 expected activities retained |

The parser benchmark measures in-memory parse/detection, not disk tailing, OBS or media processing. Run `cargo bench -p owc-wow-log --bench ingestion --locked` to reproduce it. The separate backlog test crosses the 1 MiB polling boundary and serializes/restores its cursor without losing any of 10,000 lines. These measurements do not establish a universal game CPU budget.

## Reproduce local integration checks

Run `cargo test --workspace --locked` and `cargo clippy --workspace --all-targets --locked -- -D warnings`.

Build `cargo build --example obs_probe --bin obs-wow-companion --locked`, then run `python3 scripts/isolated-obs.py`. Set `OWC_BROWSER_SETUP_TEST=1` to include the complete browser wizard against this isolated OBS instance. It creates a private OBS config, virtual display and synthetic media in a printed temporary evidence directory. It never loads your live OBS profile and disables physical PulseAudio input. Requires OBS, Xvfb, FFmpeg and an unused port 4457/display 121.

Build `cargo build --bin obs-wow-companion --locked`, then run `python3 scripts/desktop-smoke.py`. It uses display 122 and a fresh data directory. Inspect the resulting desktop screenshot; the API checks alone cannot establish that the webview renders correctly.

Build the app, then run `python3 scripts/recovery-smoke.py` on Linux for process-kill/retry/orphan-worker isolation. It uses a private data directory and never connects to OBS.

Build the app, then run `python3 scripts/browser-smoke.py` for WebKit WebDriver interaction and playback checks (private display 123, port 4458).

See the [compatibility matrix](compatibility.md) for the distinction between synthetic, Wine and native gameplay evidence.

CI workflows now cover both OSes, packages/installers and release gates. actionlint and seven release-boundary tests pass locally; 30 Rust tests passed with the CI/JUnit profile before onboarding; the expanded 42-test suite passes locally. The generated shell installer and installed service passed in a private temporary installation. Remote execution is pending repository access; see [CI operation](ci.md).

## Outstanding release gates

- Actual Windows runtime, WebView2 playback, installation and game capture.
- GNOME and KDE Wayland portal capture, X11 game capture, permissions/restart behavior.
- Versioned real-game fixtures and gameplay validation for each advertised WoW flavour/activity.
- End-to-end disk-exhaustion tests and Windows process-crash qualification; Linux process-kill/retry, durable-job recovery, failed exports and lost-event handling have regression coverage.
- Windows setup/import/autostart, native Windows installer execution, clean-machine dependency qualification, dependency redistribution review and remote release CI. Linux imports/rollback and startup toggles, notices generation and dependency policy checks have passed.
- Codec/frame timing and marker alignment across long actual recordings and encoder configurations; the synthetic 20-segment continuity check is one measured configuration.

Do not publish a stable release until these gates have evidence or an explicitly revised support scope.

Run `python3 scripts/credential-smoke.py` to test the OS password store in a disposable D-Bus session and GNOME keyring. Requires `dbus-run-session` and `gnome-keyring-daemon`. The normally ignored store test is run explicitly here. After building the OBS probe, `OWC_KEYRING_OBS_TEST=1` also checks authenticated saving, reuse and deletion with isolated OBS. These tests never use the desktop user's keyring.

Launcher discovery fixtures verify a WoW install in a secondary Steam library, missing/malformed/oversized Steam metadata fallback, and Battle.net product filtering and Wine drive mapping. The steamlocate integration passes Rust 1.95 strict Clippy and Windows cross-target Clippy; native Windows and macOS launcher discovery still needs runtime qualification.

The streamlined wizard passed isolated OBS/WebKit verification: optional picture/audio help, footer Finish action, connection messages confined to Connect, and rerunning setup with the current port and two configured installations. Evidence: `/tmp/owc-browser-wi94sxl8` and `/tmp/owc-obs-pahjzi5_`.

Saved-password startup reuse passed with a fresh engine using the persisted profile and a disposable Secret Service store; reconnection remained disarmed. Secure save, incorrect-password preservation, endpoint isolation and deletion also passed. Evidence: `/tmp/owc-keyring-jhmzlj6j`, `/tmp/owc-obs-9xrj5dbt`. The host rosecd service and unlocked default collection were checked without reading personal secrets; host credential saving was not tested.

Reconnect and scene-control fixtures pass: a dropped WebSocket reconnects through the scheduled retry without restoring recording ownership; scene restoration respects user overrides and streaming. Native OBS scene switching and restoration also passed. Linux MSRV 1.95 and Windows cross-target lint checks pass.

Final OBS/WebKit scene-settings and wizard navigation run passed: `/tmp/owc-obs-92yj3gfu` and `/tmp/owc-browser-z5evmu7r`.

Automatic capture and event-stream UI passed against isolated OBS/WebKit (`/tmp/owc-obs-_bf6jlqy`, `/tmp/owc-browser-ie4m28bb`): paused log events did not record, a fresh encounter started OBS without an arm action, its end stopped after post-roll and exported a ready clip. Unit coverage includes overlapping activities and old-event rejection.

Current review/event regression suite: 49 tests pass (OS credential-store test remains separately opt-in), with strict Clippy and formatting checks passing. Added coverage includes installation-scoped manual markers without a dungeon start, checkpoint acknowledgement, forced abandonment with stale-worker rejection, live manual markers, atomic sidecar replacement and preserved timing. WebKit checks cover fullscreen entry/exit with integrated controls, named marker persistence, label updates, and chronological ordering across active and saved recordings.

Shutdown recovery follow-up: 51 tests cover manual-start checkpointing, abandoned recovery, completed manual clips staying finished, queued sidecar edits surviving restart, missing sidecar reconstruction, coalescing multiple edits, and write-failure retry. Linux process-kill export recovery passed again at `/tmp/owc-recovery-3pvit5te`. Label search and fitting row labels are included in WebKit regression checks.

Video.js review follow-up: 52 Rust tests include bounded, installation-scoped combat sampling and persisted sampling state. WebKit checks compare each signpost's rendered centre against the actual seek-bar rectangle and video time at normal size, 1024px width and fullscreen. They also verify standard playback-rate selection, fullscreen position preservation, boss/combat spans, manual marker persistence and label search. Bundled player fonts are served locally under the unchanged Content Security Policy.

Player/export polish passed isolated WebKit testing at `/tmp/owc-browser-s4kaaqdh`: volume/time separation, event-label hover with seek-tooltip suppression, five overlapping signpost lanes, draggable export boundaries, fullscreen range alignment and an actual precise clip export. Export settings open by default. The workspace suite passed 51 tests with one OS credential-store test skipped; strict Clippy and formatting passed.

Original-first playback passed WebKit verification at `/tmp/owc-browser-_snhrac_`: the MKV fixture played directly; an injected unsupported-format error exercised automatic MP4 fallback. Repeated selection preserved the player source, and no generic recording-end marker was rendered. Media tests compare video stream hashes to verify lossless H.264 copying and unchanged cache modification time on reuse. 52 Rust tests passed (one credential-store test skipped), with strict Clippy and formatting passing. Native Windows decoder behavior remains unqualified.

Playback metadata verification passed at `/tmp/owc-browser-sm577gd9`: original playback exposes source metadata; simulated rejection switches to the compatibility explanation with the rejection reason and video-unchanged mode. AAC stream copying passes the existing media/preview tests. All 52 Rust tests pass with one opt-in credential-store test skipped; strict Clippy passes.

Marker visibility toggle passed WebKit verification at `/tmp/owc-browser-d2efzf8u`. A targeted engine regression verifies old configurations default to visible and a hidden preference survives engine restart without changing recording state.

Result/delete UI passed WebKit checks at `/tmp/owc-browser-4fd9mc41`: footer Delete placement, per-row trash controls, cancelled confirmation retaining a clip, and Shift-click deleting only the test clip. Parser tests cover +1/+2/+3/deplete boundaries, unknown timers and abandoned runs. 54 workspace tests passed with one credential-store test skipped.

Pending-to-ready playback and click-only OBS status passed WebKit regression at `/tmp/owc-browser-v431zron`, including source reset, restored playback rates, hover-title removal, and explicit status-panel opening/closing. Personal pending clips were diagnosed read-only; their still-open/unindexed source footage has not been recovered by this UI change.

In-app media-test removal: WebKit onboarding and playback passed at `/tmp/owc-browser-dmpmdxd2`; local service authentication, removed diagnostic-command rejection and shutdown passed at `/tmp/owc-service-7f7prt52`. Strict Clippy passes. Native X11 window-close verification timed out twice (`/tmp/owc-desktop-jnq06zlc`, `/tmp/owc-desktop-mlxb686_`) and remains unresolved; do not interpret this run as native-close qualification. UI/service fixtures are silent; test-only audio continuity fixtures retain signals for gap detection and are never played.

Pending-player visibility passed WebKit regression at `/tmp/owc-browser-rt2_qqx1`: pending clips show neutral completed-encounter wording, hide the play overlay and control bar, and restore playback controls when video becomes ready. This change does not resolve source-file recovery for existing pending clips.

Recording ownership recovery passed with real isolated OBS at `/tmp/owc-obs-h_46dqlb`: start managed recording, split, recreate the engine, verify the persisted output and counters, resume ownership, then stop and close the final fragment after post-roll. Both fragments were indexed exactly once. Reset counters and paused recordings are rejected by the protocol fixture. `OWC_RECONNECT_TEST=1` enables this test in the scheduled OBS integration workflow.

Final recovery verification: 55 workspace tests passed (two opt-in tests skipped), strict Clippy and formatting passed, and the desktop binary built successfully. The opt-in native OBS recovery test passed separately as above. Two personal Kith'ix clips were reconstructed from preserved fragments and probed after atomic replacement: wipe 290.365 seconds and kill 381.101 seconds. The original database and affected files are retained in `recovery-backup-20261025-205530` in the local data directory. The wipe lacks the requested pre-roll because recording began at encounter start; no missing history was fabricated.

Play/pause source-switch regression reproduced in WebKit at `/tmp/owc-browser-u6hfj3qu`: replacing a playing source left Pause displayed on the paused replacement. After synchronizing the retained control bar from the current video state, the full browser suite passed at `/tmp/owc-browser-uiuhyid0`, including playing-to-pending-to-ready transitions and subsequent Play/Pause clicks. The desktop binary was rebuilt.

Dungeon-exit grace tests cover cancellation on return, a late return, outside combat and repeated zone changes, checkpoint round-tripping, completion during grace, and installation-scoped abandonment publication from the log watcher. The 57-test workspace suite passed before the additional watcher test; that watcher test passed separately. Strict Clippy and the rebuilt desktop binary passed. Live-game exit-event emission remains dependent on the game writing ZONE_CHANGE to the selected combat log.

## Export performance and advanced settings

2026-10-25, synthetic 1920×1080/30 H.264 input, three 10-second fragments with a half-second cut at each end. All timings include probing and full output decode validation; these are short local benchmarks, not a guarantee for long gameplay footage.

The initial sandbox baseline was 5,834 ms for precise CPU export and 1,313 ms for the old copy-through-temporary-parts route. After direct concatenation, the same sandbox workload took 1,060 ms in copy mode (19% less than the old copy route, 82% less than the original precise path). This before/after pair is one run each.

Three alternating host runs gave these medians under the same conditions:

| Method | Median elapsed | Runs |
| --- | ---: | --- |
| Precise CPU | 2,488 ms | 2,657 / 2,455 / 2,488 ms |
| Precise VAAPI | 3,402 ms | 3,383 / 3,458 / 3,402 ms |
| Direct copy, following CPU | 491 ms | 507 / 491 / 482 ms |

Copy took about 80% less time than CPU precise export. VAAPI worked but was slower on this host, so GPU is an explicit preference. Maximum child RSS across the CPU/GPU run set was 943,556 KiB; the separate initial VAAPI/copy run peaked at 221,236 KiB. Direct copy avoids per-fragment temporary media writes; full decode validation still has a CPU cost. Evidence JSON and outputs: `/tmp/owc-export-perf`.

Reproduce by creating `source.mkv` in a scratch directory with FFmpeg (`testsrc2=size=1920x1080:rate=30`, 10 seconds, H.264, 60-frame GOP, one AAC audio track), then run `cargo run -p owc-media --example export_benchmark -- /absolute/scratch/directory cpu` or replace `cpu` with `vaapi`. Each run writes unique outputs and JSON metrics; remove the scratch directory when finished. Output quality settings differ between hardware backends, so runtime is not a quality-equivalence comparison.

Verification: 61 workspace tests passed; CPU fallback on an invalid VAAPI device, quoted source paths, unchanged decoded frames across direct concatenation, stream-layout rejection, audio-track retention, old-config defaults and existing precise continuity tests passed. WebKit advanced-settings persistence and playback passed at `/tmp/owc-browser-ykqvy7yb` (including screenshot review). Real isolated OBS tested precise manual clips and automatic Fast copy with two tracks at `/tmp/owc-obs-7da7o9pn`. NVENC, QSV and AMF command paths are implemented but still need testing on their respective hardware and drivers; Windows native media execution remains a release qualification gate.

Final export checks: formatting, strict workspace Clippy, Windows GNU cross-target media Clippy, all 61 workspace tests, and the rebuilt desktop binary passed. Missing fast-start alignment now falls back to precise cuts with a saved warning instead of leaving the automatic job pending.

Death recap and navigation follow-up: 65 workspace tests passed (two opt-in tests skipped), strict workspace/all-target Clippy passed. New checks cover per-player bounded damage history, expiry/resurrection/zone resets, checkpoint serialization and delivery in both activity metadata and independent manual marker events. WebKit regression run `/tmp/owc-browser-mc382jsv` passed, including recent-hit rendering and Settings/Recordings navigation without source reload or loss of position/rate; opening Settings pauses playback. The originally reported visual unload was not reproduced in the isolated browser; return navigation explicitly refreshes the paused frame for WebKit.

Diagnostics/inactivity follow-up: 67 workspace tests passed (two opt-in tests skipped), including bounded rotating event history, truncated-line recovery, five-minute group-action inactivity and real FFmpeg machine-readable progress during export and playback preparation. Strict Clippy passed. WebKit `/tmp/owc-browser-bu9jhznf` passed with event viewer opening/closing, search and missing-exit explanations. The personal Den source was read-only inspected: start 23:15:02, final logged event 23:28:40, no later zone or challenge-end boundary, and the watcher cursor matched file size. Its absence of an exit event explains why the 60-second exit grace did not start; runtime was still using the old 30-minute fallback.

Death navigation/video zoom: build passed and WebKit `/tmp/owc-browser-rcc39aqn` passed. Browser assertions verify early deaths clamp to zero with five-second pre-roll, wheel zoom is cursor-centred, controls are not transformed or treated as wheel-zoom targets, and Reset restores 1x. Existing fullscreen, fine seek, export-range and manual-marker checks remain passing.

Early retained-footage review: 69 workspace tests passed (two opt-in tests skipped), with strict Clippy and formatting passing. Real FFmpeg checks cover unchanged decoded frames across three joined fragments, selected audio tracks, video-only input, invalid-track rejection, and successful playback preparation while the final library file is still pending. Coverage tests reject open, incomplete and gapped footage; origin translation rejects mismatched playback sources. The one-time Fast migration preserves later explicit Precise choices. Housekeeping regression checks pin sources/staging while preparing, remove interrupted staging afterward, and retain the current preview under cache pressure.

WebKit `/tmp/owc-browser-jk1acuye` passed against the rebuilt desktop app: Fast default/settings persistence, playback reset, uninterrupted source/time on final publication, death pre-roll, wheel zoom, Settings navigation, fullscreen and precise export remain passing. These are Linux synthetic tests; native Windows playback and vendor GPU qualification remain outstanding.

Embedded events/capture controls: rebuilt desktop binary and WebKit `/tmp/owc-browser-c9mopip3` passed. Checks cover workspace-aligned dock edges, non-modal opening/collapse, real pointer dragging, keyboard resizing/minimum height, event search and offline-disabled Record now inside the recordings pane. Duplicate top capture status is hidden. Screenshot review confirmed readable event rows at minimum height; existing playback/export flows remain passing.

Shutdown and readable event ages: the open-SSE service regression reproduced a ten-second shutdown timeout before the fix and passed afterward at `/tmp/owc-service-7pqaick4`. Native window close passed at `/tmp/owc-desktop-jbxnx12y` after replacing completion polling with an explicit Tao user event. WebKit `/tmp/owc-browser-rzfrcgav` passed including clicking Quit and checking process exit with the UI event stream connected, plus duration formatting at seconds/minutes/hours/days boundaries. Build, strict Clippy and formatting passed. Native-close evidence is X11; Niri itself was not automated.

Desktop shutdown hardening: accepted Quit now notifies Tao directly through Wry IPC, while HTTP graceful draining has a two-second bound. Native-close `/tmp/owc-desktop-jh0jr5ho` and open-SSE service `/tmp/owc-service-whjkfo2q` passed; strict Clippy passed. The reported stuck personal window was verified to be running a deleted/superseded executable and was terminated after its stopped-state report.

Marker layout/filtering: WebKit `/tmp/owc-browser-i_lk7jgx` passed checks for five overlapping lanes versus separated markers all in lane zero, global visibility, death-category filtering without removing the death table, and automatic discovery of an unknown marker type. The Rust preference-restart test and strict Clippy passed. The player uses seek-bar width and reruns placement after rendering rather than assigning lanes against a zero-width marker layer.

Sharing and library layout: 71 workspace tests passed (two opt-in tests skipped), with strict Clippy and formatting passing. Sharing unit tests cover portable metadata, script-safe embedding, archive contents and no-overwrite behavior. WebKit `/tmp/owc-browser-zv4j57sh` passed ZIP export, extracted local playback through the file-picker fallback, static HTTP playback, 300-recording filtering/scrolling, 300-death scrolling, pinned filters, reserved footer space, date/key/difficulty filters and normal Quit. A WebDriver screenshot during playback in a secondary tab timed out on an earlier run; the final run verifies playback state without that screenshot. Final build and JavaScript syntax checks passed. Native Windows/macOS sharing playback has not been tested.

Export panel and mixed audio: 72 workspace tests passed (two opt-in tests skipped), plus strict Clippy. The real FFmpeg test verifies both 440 Hz and 880 Hz inputs survive in one mixed stream, silent output has no audio streams, and invalid tracks are rejected. WebKit `/tmp/owc-browser-y_zcuue5` passed ZIP playback and layout assertions for a destination wider than 90% of the form, the export action inside its panel, and equal status-pill heights. The full suite requires permission to bind local mock OBS sockets.

Export picker and asset details: 73 workspace tests passed (two opt-in tests skipped), strict Clippy and formatting passed. A new library regression verifies latest-bundle replacement, restart persistence and deletion cascade. FFmpeg verifies named tracks survive precise cuts; `/tmp/owc-audio-demo/multitrack-demo.mkv` contains three verified named AAC tracks. WebKit `/tmp/owc-browser-k4bb0wdr` passed collapsed export, embedded track labels, latest bundle rendering, picker cancellation/stale-selection handling and ZIP playback. Metadata table screenshot is `asset-details.png`. Native startup/close passed at `/tmp/owc-desktop-5ossa2ud`; actual platform save-dialog interaction and Windows/macOS remain manual checks. Third-party notices regenerated for rfd.

Player audio switching: WebKit `/tmp/owc-browser-i7ljchqd` passed actual track changes while paused and playing, preserving position and 0.5x playback speed, followed by the complete existing UI/ZIP checks. Completed preview snapshots now retain their audio track number. Speed capture reads the native video element because the Video.js getter can lag until ratechange is dispatched. Build, strict Clippy and JavaScript syntax checks passed.

Combined date picker: WebKit `/tmp/owc-browser-ywcwklap` passed reverse-order date selection, explicit Apply, real Escape-key dismissal, outside pointer dismissal preserving the applied range, adjacent reset and All dates reset. Screenshot `date-range.png` reviewed. The full existing player/export browser checks remained passing; build and JavaScript syntax checks passed.


### 2026-10-26: Missing Temple encounter investigation

Read-only inspection found Temple +20 starting at 15:34:52 and completing at
16:05:39 in the local combat log. OBS reported VAAPI import failures and fallback,
then began recording at 15:34:53; its log ends there, the WebSocket disconnected,
and the corresponding MP4 is zero bytes. This establishes missing footage but does
not by itself prove the cause of the OBS process failure.

The controller cleared tracked activity IDs on disconnect and discarded completion
when capture was no longer armed. Interrupted IDs now survive independently of OBS
ownership so completion is checkpointed with an interruption warning. A regression
test drives the actual disconnect transition and verifies the saved outcome.

Replaying the full combat log through the current parser/detector produced zero
parse errors, zero inactivity expirations, and a successful +20 completion. The
live inactivity abandonment is therefore not reproduced by the current detector
alone; delivery timing, buffering and the running build still need investigation.
The user's library has not been modified by this investigation.


### 2026-10-26: Bounded-reader inactivity regression

A synthetic log larger than the 1 MiB read limit reproduced premature inactivity
abandonment: the first batch held old combat, while the successful completion was
in the next unread batch. The regression failed before the change. The tailer now
reports unread bytes and the watcher defers wall-clock expiry until caught up;
backlog batches retain acknowledgement/backpressure but skip the idle poll delay.
This demonstrates and fixes a real failure mode, not proof of the Temple incident's
specific cause. Explicit log-event lifecycle transitions remain processed normally.


### 2026-10-26: Workspace regression run after capture fixes

`/tmp/owc-tools/bin/cargo-nextest nextest run --workspace` passed all 75 enabled
tests; two opt-in isolated credential/native-OBS tests were skipped. Execution
required localhost sockets for mock OBS. The initial restricted run failed four
app connection tests at listener creation, not at application assertions.
Documentation links and OBS template validation also passed. The release audit
now maps the 22 specification acceptance criteria without declaring native
Windows, real-client coverage or disk-pressure qualification complete.


### 2026-10-26: Share publication validation

Sharing now checks output duration within 250 ms, the selected/mixed audio count,
and a full FFmpeg decode before publishing its ZIP. The decode routine is shared
with automatic-export validation. A real generated H.264 MP4 succeeds; corrupting
its media payload while retaining readable container metadata makes decoding and
sharing fail, with no final damaged ZIP and no leftover staging directory.
The targeted regression and strict app Clippy pass. This does not replace native
browser compatibility checks or disk-exhaustion qualification.


### 2026-10-26: MSRV, CI policy and real process recovery

- `cargo +1.95 check --workspace --all-targets --locked`: passed.
- `python3 -m unittest discover -s scripts -p 'test_ci*.py'`: seven passed.
  Its archive-verification message refers to synthetic test fixtures, not proof
  of newly built native platform release packages.
- `/tmp/owc-tools/bin/actionlint`: passed.
- `cargo build -p owc-app --locked`: passed.
- Corrected `scripts/recovery-smoke.py` to identify the fragment output argument
  regardless of trailing FFmpeg progress options. Before correction the hold hook
  was never triggered and the test timed out after a normal successful export.
- Corrected process-crash smoke passed at `/tmp/owc-recovery-f1_s1fbx`: kill during
  a held worker, restart with a distinct staging path, exactly one published clip,
  original source retained, old worker unable to replace it, full output decode.
- Added `libdbus-1-dev` to cargo-dist's Linux build dependencies, matching the
  maintained CI environment and current desktop dependencies.


### 2026-10-26: Low-space ownership and preservation regression

`low_space_stops_only_owned_output_once_and_preserves_sources` exercises the
controller's real available-space branch against a localhost mock OBS server.
A deliberately high test threshold triggers pressure without filling the disk.
Observed output receives no mutating controls; managed output receives exactly
one StopRecord across repeated cleanup passes. Open, pending-source and external
files remain byte-for-byte intact, and the UI status reports the storage stop.
Concurrent read-only setup checks are permitted by the assertions.

This is controller integration evidence, not full ENOSPC or native OBS shutdown
qualification. Actual filesystem write failures during SQLite/FFmpeg/ZIP output
remain a separate release gate.


### 2026-10-26: Browser candidate and long-key watcher validation

Current build passed `scripts/browser-smoke.py` with isolated Xvfb/WebKit and
private data at `/tmp/owc-browser-eq58oltw`. Artifacts include the shared ZIP,
standalone viewer, playback/settings/metadata and date-filter screenshots. The
post-reset date-filter/metadata screenshot was inspected: library filters remain
pinned, export controls and both metadata tables are within the review pane, and
the label/delete footer remains below the scrollable content.

Added a 45-minute synthetic file-watcher regression with 30-second group activity,
five nested boss starts/ends and five deaths. Polling uses the configured five-minute
inactivity timeout throughout. Exactly one start and one successful M+ completion
are emitted, with all 15 boss/death markers and no duplicate raid activities.
The targeted test and strict app Clippy pass. This verifies synthetic elapsed-time
behaviour, not a 45-minute native recording or real client-format qualification.


### 2026-10-26: Native desktop and isolated OBS candidate rerun

Desktop/API startup and X11 WM_DELETE_WINDOW quit passed at
`/tmp/owc-desktop-yid87t2q`. Isolated OBS 32.2.2/WebSocket 5.7.4 passed manual and
combat-log recording, splitting, two-track export, prior-scene restoration and
native reconnect/owned-fragment closure at `/tmp/owc-obs-9ssjux_y`.

The first probe run (`/tmp/owc-obs-wmem0nzm`) asserted scene restoration as soon as
closed-fragment exports were ready, while OBS was still stopping. The probe now
requires confirmed idle ownership before checking the restored scene. This is a
test timing correction; the successful rerun verifies the existing restoration
behaviour after the stop event. Strict app Clippy and documentation checks passed.
Tests used private OBS configuration and Xvfb, not the user's live scene collection.
Native Wayland and Windows gameplay qualification remains outstanding.


### 2026-10-26: Sharing cancellation implementation

Player-bundle exports now expose Cancel export. Media futures are cancelled through
watch signalling; existing media-process kill-on-drop handles process teardown.
Blocking ZIP work observes cancellation between chunks and before publication,
and the controller keeps source protection until the worker reports completion.
An already published successful bundle wins a late cancellation race.

Five sharing tests pass, including in-flight future drop on cancellation and
cancelled ZIP nonpublication/source preservation followed by a successful retry.
Strict app Clippy and JavaScript syntax checks pass. Native/browser cancel-button
interaction and process teardown still need end-to-end validation; this does not
claim automatic encounter-export cancellation is implemented.


### 2026-10-26: Cancellation browser/process and ZIP-worker verification

The WebKit smoke now holds an actual media child process, clicks Cancel export,
waits for cancelled controller state and process disappearance, verifies no ZIP or
sharing staging directory remains, restores the normal media tool, and retries
successfully through the same form. Full smoke passed at
`/tmp/owc-browser-byj7ovna`. Cleanup also kills only the identified temporary
wrapper if the smoke aborts unexpectedly.

A separate ZIP test holds the writer at its first progress update, cancels after
its temporary output has received data, then verifies no final/temporary ZIP and
unchanged source size. This covers cancellation inside the blocking writer, not
just before export starts. Automatic encounter-export cancellation remains open.


### 2026-10-26: Automatic preparation cancellation

Added Cancel preparation for the selected active export. The media runner observes
cancellation, kills its child through future drop, suppresses CPU fallback, and
returns through normal staging cleanup. The controller keeps work/source guards
until completion, persists a failed retryable job with a cancellation message,
and does not immediately requeue it.

All six media tests pass, including cancelled-export cleanup and valid retry.
The controller regression verifies that cleanup retains source footage, the plan
survives library reopening and explicit Retry returns it to the pending queue.
Strict app Clippy and UI JavaScript syntax checks pass. Automatic cancel-button
interaction still requires browser/native verification.


### 2026-10-26: Automatic preparation browser/process validation

Full WebKit smoke passed at `/tmp/owc-browser-5_vtr4do`. It starts the same durable
preparation path used by automatic exports via the internal trim fixture, holds
its real encoder child, selects the unfinished recording, clicks Cancel preparation,
and waits for process disappearance and retryable failed state. It verifies staging
cleanup, restores FFmpeg, clicks Retry and waits for a ready clip. Sharing cancel,
ZIP publication/playback and the remaining UI smoke checks also pass in this run.
The test uses a private data directory and does not change live OBS configuration.
This validates Linux WebKit interaction, not native Windows cancellation.


### 2026-10-26: Thumbnail adapter and sharing posters

The media adapter now generates a single JPEG at a validated timestamp, preserving
aspect ratio with width capped at 640 pixels and no upscaling. It uses the existing
cancellable media runner and temporary-directory cleanup. Shared bundles include
poster.jpg and the player references it before playback; it is also declared in
portable video metadata.

The media regression decodes the generated JPEG, checks dimensions, rejects negative
and end-of-video timestamps, and checks byte-for-byte source preservation. The
real bundle test verifies the included JPEG. Both targeted tests and strict app
Clippy pass. Final browser poster rendering validation remains outstanding.


### 2026-10-26: Consolidated cancellation/thumbnail candidate verification

- Workspace nextest: 84 passed, two opt-in tests skipped. The native OBS opt-in
  test was separately verified in the preceding isolated run; credential-store
  qualification remains separately recorded.
- Rust 1.95 workspace/all-targets locked check: passed.
- Full WebKit smoke: passed at `/tmp/owc-browser-9zhgnkzv`, including both real-worker
  cancellation/retry flows and hosted shared-player poster loading before playback.
  The poster is a successfully loaded image of nonzero width no greater than 640,
  referenced by Video.js and visible before playback. ZIP contents include it.

These are Linux results and do not establish native Windows gameplay support.


### 2026-10-26: Watcher pipeline 10x synthetic ingestion qualification

Opt-in `ingestion_pipeline_ten_times_reference_rate` measures file tailing, parsing,
detection, event-bus delivery, JSON checkpoint serialization and SQLite transactions.
Its generated fixture represents 100 seconds at a synthetic reference rate of
1,000 events/second, with 100 deaths and 100 sampled combat markers. It requires
completion within 10 seconds (10x that defined rate), zero parse errors, exact
cursor completion, one start/successful completion, all expected marker events,
and one outstanding acknowledged batch per watcher.

On Ryzen 7 9800X3D / Linux 7.2.10, release profile: 100,002 lines / 10,197,039 bytes
processed in 0.620 seconds (161,201 lines/second), ten batches, zero additional
queued batches. The full [measurement](evidence/ingestion-2026-10-26.json) is retained.
This is a single local synthetic run, excluding OBS, UI snapshots, media encoding
and competing workloads; it is not a universal gameplay CPU or memory guarantee.

Reproduce with `OWC_INGESTION_REPORT=/tmp/owc-ingestion-report.json cargo test -p owc-app --release ingestion_pipeline_ten_times_reference_rate --lib -- --ignored`.
Strict app Clippy passes. The test is opt-in because its timing threshold is a
qualification measurement rather than a reliable shared-CI unit-test deadline.


### 2026-10-26: Real filesystem exhaustion for media and ZIP outputs

`python3 scripts/disk-full-smoke.py` creates a private Linux user/mount namespace
and a 4 MiB tmpfs. It never fills a normal host filesystem. Opt-in regressions fill
only that mount, require actual “No space left on device” errors, then free space
and retry. FFmpeg failure completes within a ten-second bound, leaves no published
clip or staging directory, preserves the source byte-for-byte, and retries to a
fully decodable video. ZIP failure leaves no final or temporary ZIP, preserves its
source, and retries to an archive containing identical video bytes.

These tests exercise actual ENOSPC, beyond the separate mocked low-space controller
threshold test. SQLite exhaustion and native OBS disk-full behaviour remain separate
qualification work. The script requires Linux unprivileged user namespaces and
mount support; unsupported environments should report that limitation, not count
this test as passed.


### 2026-10-26: Real SQLite disk-full atomicity

The private 4 MiB tmpfs script now also exercises SQLite checkpoint failure. An
8 MiB activity update fails with SQLite's DiskFull code. The checkpoint transaction
rolls back both its cursor update and activity update, preserving the original
record. After closing/reopening, the prior cursor and single activity remain;
a smaller retry commits and PRAGMA integrity_check returns ok. Media and ZIP
ENOSPC/retry tests pass again in the same script.

The controller now retains checkpoint acknowledgements after failure and retries
from its timer without replaying lifecycle events or blocking independent OBS
handling. All running-controller cursor writes also include pending completions.
The private-tmpfs controller regression forces an oversized transaction to fail,
verifies that the watcher acknowledgement remains pending and the previous cursor
is unchanged, then reduces the payload and verifies successful retry, acknowledgement
and one completed encounter. This qualifies controller transaction recovery; it
does not simulate a native OBS disk-full failure. Strict workspace Clippy passes.


### 2026-10-26: Folder actions and candidate regressions

Desktop video details now offer Open folder separately for the published recording
and latest exported ZIP. The native handler resolves an existing absolute file to
its containing directory and launches the platform file manager using an argument
array. It never executes the video or ZIP. Missing files return an actionable error.
Browser-only mode hides these native actions.

The path guard test covers missing files, directories, relative paths, URL-shaped
input, Unicode and shell punctuation. WebKit run `/tmp/owc-browser-c_shlpr_`
passed the full browser suite and verified both folder actions send the selected
asset's path to the desktop bridge; this uses a bridge stub and is not evidence
that Explorer or a Linux file manager opened. Native file-manager qualification
remains part of the platform walkthrough.

The current workspace nextest run passed 85 tests with 7 opt-in tests skipped
(credential store, native OBS reconnect, throughput and four ENOSPC checks).
Strict workspace/all-target Clippy and documentation checks pass.


### 2026-10-26: Automatic shutdown and native export latency

Quit no longer rejects an active recording or export. The serialized controller
checkpoints tracked active intervals as incomplete (manual intervals retain unknown
outcome), suppresses fresh starts without persisting Pause, stops only owned OBS
capture, and signals media cancellation. Automatic jobs cancelled during shutdown
return to durable pending state; sharing jobs are retried explicitly. The HTTP quit
path allows ten seconds after preparation for source closure and worker cleanup,
then exits with recovery records intact.

Mock OBS tests verify one owned stop, no observed-output mutation, idempotent
shutdown, rejected fresh manual starts and unchanged Auto recording preference.
The cancellation regression verifies pending plans/source preservation and no new
export start during shutdown. Native OBS run `/tmp/owc-obs-dg4_kws1` confirms
owned capture stops during shutdown. Native X11 close run
`/tmp/owc-desktop-_q29kgje` exits through the new flow. WebKit run
`/tmp/owc-browser-dtb_fv6x` clicks Quit with a real held encoder, verifies process
exit and child teardown, unchanged source existence, no export staging directory,
and a durable pending plan. Native Windows remains unqualified.

The same isolated OBS probe measures final closure to first exporting/ready UI
snapshot at **64.54 ms**, below two seconds. It uses an independent STOPPED
subscription and 100 ms polling, a short single-file raid, synthetic media and an
idle worker. It does not establish performance under competing jobs or on Windows.
[Machine-readable evidence](evidence/queue-latency-2026-10-26.json) records OBS
versions and measurement scope.

Workspace nextest: 87 passed, seven opt-in checks skipped. Strict Clippy, Rust 1.95
MSRV and documentation checks pass.


### 2026-10-26: Linux archive, installer and public documentation

The optimized cargo-dist Linux archive passed checksum, required-content,
help and version checks. The extracted binary passed native desktop/window-close
(`/tmp/owc-desktop-xbdhus0e`), WebKit playback/export/busy Quit
(`/tmp/owc-browser-i3evbyss`) and process-kill export recovery
(`/tmp/owc-recovery-ytjlt3ec`). The generated shell installer installed into an
isolated directory via a private artifact server, then passed service/media checks
(`/tmp/owc-installer-birpxggi`). Global generation used a temporary committed
source snapshot because the working repository had no HEAD at that point.
This is local host evidence, not clean Ubuntu or native Windows qualification.

The public website uses actual app captures with generated silent video and
fictional encounters, produced by `scripts/capture-showcase.py`. No user library
or combat logs were copied. Desktop and 390px mobile rendering, local assets and
anchor links pass (`/tmp/owc-site-_j7b_ob9`). Captures were visually reviewed.
The website is static and uses no analytics or external font dependencies.


### 2026-10-26: Initial remote CI failure and expanded Windows checks

Main-branch run 40369293609 passed dependency/license/workflow policy, but both
native compiler jobs stopped at Rust 1.95's duration-suboptimal-units lint in the
watcher fixture. The failure was reproduced with the exact pinned toolchain
locally and fixed without suppressing the lint.

The desktop smoke script now supports a PID-scoped Windows WM_CLOSE request,
rather than only X11 WM_DELETE_WINDOW. It checks authenticated local service
access and requires the native window's JavaScript quit path to exit the process.
Windows deliberately skips the Linux autostart test because Windows known-folder
APIs cannot be redirected with XDG environment variables. The existing Linux path
passes after this change (`/tmp/owc-desktop-7nj49np5`).

The package workflow now runs this desktop check on Windows, and the quality
workflow opts into the existing unique-scope credential-store round trip on its
disposable Windows runner. Their inclusion is not yet a passing runtime result.


### 2026-10-26: Delayed start with native automatic capture

`scripts/isolated-obs.py` now delivers an encounter start 30 seconds late while
OBS is already recording an earlier automatic encounter. The delayed encounter
replaces the earlier raid and must retain its own footage, then stop capture
automatically after completion. This avoids manual capture's completion fallback.
The Linux X11 run passed in `/tmp/owc-obs-mmyp5e2c`: complete coverage, 1,004 ms
retained pre-roll, 33,033 ms output and two AAC audio tracks. Lossless copying
failed duration validation for this cut and correctly fell back to CPU encoding.
[Structured evidence](evidence/delayed-start-2026-10-26.json) records that limitation.

The same run passed scene restoration, automatic closure-to-queue latency
(65.24 ms), and owned shutdown. Workspace nextest: 88 passed, 7 opt-in tests
skipped; Rust 1.95 strict Clippy and documentation checks passed. This uses
synthetic log events and video, not native Windows or live gameplay evidence.


### 2026-10-26: Native combat-log notifications

The log worker now uses notify's platform backend to wake on filesystem changes,
while retaining its 250 ms reconciliation interval. A one-slot channel coalesces
bursts; access-only events are ignored to avoid waking from our own reads. Durable
checkpoint acknowledgement still gates the next read. Missing or replaced folders
and missed notifications remain covered by reconciliation.

Linux tests verify create, append and rename notifications, startup with a missing
folder followed by recreation, and no next batch before acknowledgement. Existing
rotation, UTF-8, lifecycle and backlog tests pass. Windows native notification
behaviour remains subject to CI and gameplay qualification. Dependency notices
include notify's CC0 dedication, with a package/version-scoped cargo-deny allowance.

Workspace nextest after notification integration: 90 passed, 7 opt-in skipped.
Rust 1.95 strict Clippy, dependency checks and documentation validation pass.
The generated notice retains upstream CC0 text verbatim, including its whitespace.


### 2026-10-26: Delayed starts outside retained history

A late start now remains eligible when OBS is already capturing valid footage,
even if the start precedes the retained interval. The controller regression
finishes a 30-second-old encounter with only its last five seconds retained and
checks `Coverage::Truncated` plus the actual source origin. Export already adds
the retained-footage warning to metadata. No existing capture, invalid timelines,
pause and stopping guards remain in force; old events cannot start new OBS output.
Capture lifecycle tests pass. Native partial-coverage playback remains part of
final platform qualification; the complete-coverage native case is recorded above.


### 2026-10-26: Native partial-history export

The expanded isolated OBS probe passes complete and truncated delayed encounters
with native filesystem notifications enabled (`/tmp/owc-obs-ei_zpu03`). The partial
case requests an interval beginning before capture, requires `truncated` coverage,
a source origin after the requested start, a 5–15 second playable output and the
retained-footage warning. Both audio tracks are retained. See
[measured partial-history evidence](evidence/partial-start-2026-10-26.json).

An initial fixture overlapped two separate capture sessions and correctly yielded
a gapped/unavailable clip. The corrected fixture separates sessions by 35 seconds
to isolate missing leading footage. Gap rejection was not relaxed. The final run
also passes complete-history recovery, automatic stopping, scene restoration and
owned shutdown. Rust 1.95 strict Clippy passes. Windows qualification remains open.


### 2026-10-26: Current committed Linux package

Built `b247956` with cargo-dist's local Linux target. The 4,950,460-byte archive
passes checksum, required-content and executable-version verification. It includes
the player website assets alongside the README, guides and native OBS templates.
The extracted executable passes authenticated service/two-track media/shutdown,
native desktop window-close, WebKit original playback and UI, and process-kill
recovery with orphan-encoder isolation.

[Archive checksum and evidence paths](evidence/package-linux-2026-10-26.json)
identify this exact artifact. This is local-host evidence with installed system
dependencies; clean Linux and native Windows package jobs remain release gates.


### 2026-10-26: Native Windows test findings

Windows job 121745891174 in run 40372211349 passed lint and 84 of 87 tests,
with three fixture failures. Event-log rotation used an append-only handle for
`set_len`, which Windows rejects; the fixture now opens for writing, seeks to the
end and closes the handle before rotation. Wine dosdevices path assertions are
Unix-only, with a native drive-path assertion on Windows. ZIP cancellation now
queries size through an opened handle instead of a potentially cached directory
entry, and requires the first MiB of video before cancelling.

All three corrected tests pass on Linux and strict Rust 1.95 lint passes. Native
Windows rerun is still required; these changes are not a passing Windows result.
The failed quality job prevents packaging and release publication as intended.


### 2026-10-26: Fast-copy fallback investigation

Inspected the retained native OBS source from `/tmp/owc-obs-ei_zpu03` to explain
its CPU fallback. A 3.000-second concat input seek for a 33.062-second interval
produced 34.233 seconds, failing duration validation. Moving `-ss` after the input
produced 33.137 seconds but its first decoded-frame hash matched source time
4.000, not 3.000. A duration-only apparent improvement would lose a second of
video and break marker alignment. No seek-order change was applied.

Private experiment outputs are in `/tmp/owc-copy-check-7yb7s7o4`. CPU fallback
remains the validated behavior for this source. Any future fast-seek optimization
must verify decoded frame identity and timestamp origin as well as duration.

The pending implementation and Windows fixture fixes were pushed to the temporary
`ci/release-validation` branch and dispatched as run 40374920263. This allows the
older main-branch Linux job to finish without cancellation. Neither run is yet
a passing full release-validation result.


### 2026-10-26: Reusable CI tool cache

Added a composite action for exact-version Cargo tool installation. Cache keys
include package version, runner OS/architecture and the full `rustc --version`
output, with no broad restore fallback. Only the named executable is cached;
lockfiles, dependencies and required validation commands are unchanged. This is
used for nextest, cargo-deny, cargo-dist and release-plz. Workflow lint and all
seven publication-policy tests pass. Remote cache restore/save behavior still
needs its first workflow run; run 40374920263 predates this change.


### 2026-10-26: Remote OBS qualification and evidence collection

Dispatched OBS integration run 40375838240 on `ci/release-validation`, independently
of the active quality run. This uses a fresh Ubuntu runner and the official OBS
PPA; completion is pending, so it is not qualification evidence yet.

The evidence collector now includes an explicit list of nested OBS reports:
readiness, state, queue latency, complete/partial delayed-start sidecars and the
private fixture event log. Local collection preserved the measured partial-start
report byte-for-byte and excluded library databases and OBS config files.
The probe preserves captured output on timeout; its bound is now four minutes to
accommodate the expanded real-time scenarios and slower hosted encoding. The
already-running remote workflow predates these collection changes. Python syntax
validation and all seven CI policy tests pass.


### 2026-10-26: Ubuntu precise-export and Windows UTF-8 findings

Run 40374920263 exposed a real precise-export issue on Ubuntu FFmpeg 6.1.1:
20 one-second fragments produced 580 rather than 600 frames. Reproduced it in
a private Ubuntu 24.04 Podman container. Preserving the demuxer time base with
`-enc_time_base:v demux` avoids rounding the initial A/V offset to a frame boundary
and dropping each fragment's last frame. The numeric legacy `-1` form is not
accepted by the newer host FFmpeg, so the named option is used on both versions.

The continuity fixture now uses PCM source audio and a precise 21 ms video offset.
This avoids repeating AAC priming silence from the fixture itself; it still checks
600 frames, regular 30 fps spacing, both audio tracks and every 10 ms interior
audio window. All seven enabled media tests pass on Ubuntu FFmpeg 6.1.1 and host
FFmpeg 9.0.2. Full local nextest: 90 passed, 7 opt-in skipped. Strict Rust 1.95
Clippy passes. The container is `owc-ffmpeg6-validation`, for follow-up checks.

Windows passed its corrected workspace tests and native credential-store round
trip, then failed decoding Markdown with Python's default CP1252 encoding.
Validation scripts now explicitly use UTF-8 for Path text I/O. Documentation,
license-notice regeneration and all seven CI policy tests pass locally. Native
Windows rerun remains required. No package publication was allowed by the failed
quality run.


### 2026-10-26: Hosted OBS integration passed

[Run 40375838240](https://github.com/jmylchreest/obs-wow-companion/actions/runs/40375838240)
passed on Ubuntu 24.04 with OBS 32.2.0 at `e65fc66`. The private native probe,
reconnect test and browser setup checks all completed successfully. The probe
confirmed manual and combat-log capture, splitting, two-track export, complete
and partial delayed-history recovery, and shutdown of owned capture with unfinished
work saved. Closure-to-export latency was 113.04 ms in this isolated run.

[Retained summary](evidence/remote-obs-2026-10-26.json) records provenance and
selected probe output. The downloaded artifact also contains setup screenshots.
This is synthetic Xvfb qualification, not Windows, Wayland or real gameplay
qualification, and predates the precise-export fix in `89453f8`. The newer
collector will retain nested sidecars on subsequent runs.

Windows workspace tests and the native credential-store test passed in run
40374920263, although that run failed its later documentation check. Credential
storage is therefore verified independently; full Windows CI/package validation
still requires the UTF-8 fix's rerun.


### 2026-10-26: Portable process-crash qualification

The crash-recovery smoke now compiles a dependency-free Rust worker instead of
executing a Unix-only Python shebang. Windows uses an executable helper and
process-tree cleanup; Linux retains process-group cleanup. Both packaging jobs
now run the same interrupted-export/restart check.

The assertion waits for the old orphan's FFmpeg process to finish before checking
that it cannot change the published clip. Previously a fixed 500 ms sleep could
miss a late write on a slow runner. The portable helper passed locally with
preserved sources, distinct retry staging paths, one published clip and complete
FFmpeg decode (`/tmp/owc-recovery-d8w_6v2q`). Workflow lint and seven release-policy
tests pass. Native Windows execution remains pending; enabling a job is not
runtime evidence.


### 2026-10-26: Latest Linux candidate and hosted quality checks

Rebuilt the Linux archive at `804cd59`, including the FFmpeg time-base fix.
The extracted binary passes package contents/checksum/version, authenticated
service and two-track media, native window-close, WebKit playback/UI, and the
portable process-crash recovery check. Updated
[package evidence](evidence/package-linux-2026-10-26.json) identifies the exact
archive and private fixture directories. These are host checks, not clean-machine
or Wayland qualification.

The Ubuntu quality job in
[run 40377435087](https://github.com/jmylchreest/obs-wow-companion/actions/runs/40377435087)
at `89453f8` passed: 90 workspace tests, seven opt-in skips, the separate native
credential-store round trip, formatting, strict Clippy, doctests, documentation
and parser benchmark. The policy job also passed. Windows and subsequent package
jobs were still pending when this evidence was recorded.


### 2026-10-26: Both native quality jobs passed

Run 40377435087 completed all Linux, Windows and policy quality jobs successfully.
Windows reports 89 workspace tests passed with seven opt-in skips; its separate
password-store test, strict Clippy, doctests and UTF-8 documentation validation
also passed. Linux reports 90 workspace tests and its separate secure-store test.
[Retained CI summary](evidence/quality-ci-2026-10-26.json) records exact job IDs and
Windows test output. Both package jobs have started; installer and native UI
qualification are not inferred from these quality results.


### 2026-10-26: Completion recovery without an active OBS session

The source audit found that an untracked delayed completion was discarded after
capture stopped, even if closed segments could cover it. With setup complete and
automatic recording enabled, supported filtered completions now enter the normal
durable planner regardless of the current OBS connection. They can recover closed
footage, or retain metadata with an explicit unavailable status when none exists.
They do not start OBS, acquire ownership, or postpone another capture's stop.
Pause and incomplete setup still suppress untracked completions.

The regression checks durable metadata, complete planning from closed footage,
unavailable scheduling without footage, pause/setup guards and an unchanged stop
deadline. Full local nextest: 91 passed, seven opt-in skipped. Strict Rust 1.95
application/all-target Clippy passes. Native OBS and remote platform reruns of
this change remain pending; the earlier passing CI run predates it.


### 2026-10-26: Native OBS closed-history recovery passed

The expanded private OBS probe passed at `/tmp/owc-obs-j6n5mv8q`. After both
full and partial delayed-capture cases completed and OBS became idle, it delivered
a backdated five-second fight covered by earlier closed footage. The companion
published a complete, playable two-track clip without arming capture or emitting
any recording-state change. A final OBS status query confirmed it remained idle.
[Measured evidence](evidence/closed-history-2026-10-26.json) records the result.
The private probe also passed the prior capture, splitting, export and shutdown
checks. Native Windows qualification remains pending.

CI evidence collection now includes the closed-history sidecar. Strict Rust 1.95
Clippy and all seven CI policy tests pass. The signing agent is reachable again
but has no loaded identities, so this change is staged and not yet pushed.


### 2026-10-26: Native Windows package exposed initial-state race

Windows package job 121771335377 in run 40377435087 successfully built its MSVC
archive, verified contents/checksum/version, and passed the extracted service's
authentication, two-track media and shutdown checks. Native desktop verification
then failed with `KeyError: connected`: the first status request received the
partial `Starting` placeholder while the engine was reconnecting to OBS.

The service now publishes the engine's complete initial snapshot synchronously
before exposing its URL and starting HTTP service. The desktop assertion remains
strict; it is not hidden behind a readiness sleep. The service smoke also checks
the initial connection/configuration/library fields before sending an action.
Local service and native desktop/window-close verification, plus Rust 1.95
strict Clippy, pass. Desktop evidence is `/tmp/owc-desktop-5zeky07x`. Native Windows desktop
and downstream package/installer reruns remain required. Evidence was downloaded
to `/tmp/owc-windows-package-evidence`; the job log is retained separately.


The library restart regression also replays a completed activity after publication:
one ready row, output path, two-track metadata, protection and labels are preserved,
and no duplicate export is queued. The focused repository test and formatting
check pass; this supplements the delayed-completion recovery change.


### 2026-10-26: Native marker/frame calibration

The private OBS probe now samples the test source's playback cursor when it writes
a death event. A separate FFmpeg/Python check decodes the published clip at that
marker's offset and finds the closest frame in the original 30 fps source.
RGB comparison accounts for the source BT.601 and profile BT.709 matrices; raw
luma comparison initially failed the picture threshold despite a matching frame.
The threshold was not relaxed. Source capture/export checks passed in
`/tmp/owc-obs-pgpfocht`; the corrected comparison passes on that retained footage.

[Measured result](evidence/marker-alignment-2026-10-26.json): 133.3 ms alignment
error, mean RGB sample error 7.81/255, below the unchanged threshold of 12. A
reference shifted by three seconds correctly fails the one-second alignment gate.
A blank-video negative control also fails the picture-match threshold.
This measures a synthetic Linux X11 scene, not real gameplay or Windows. It does
not establish long-session split continuity. The expanded check and reports are
wired into future isolated OBS CI runs.


### 2026-10-26: Native video split-boundary measurement

Extended frame matching to the last decoded frame of each retained fragment and
the first frame of the next. In `/tmp/owc-obs-pgpfocht`, source frame 117 is followed
by 118 across the manual clip's split: one consecutive 30 fps frame step, with
RGB match errors below the unchanged picture threshold. The reusable native
check now requires multiple fragments and checks their boundaries. The measured
boundary is included in the marker-alignment evidence.

A separate decoded-audio inspection found no silent 10 ms window between 3.9 and
4.1 seconds in the published clip. This is not proof of sample-accurate audio
continuity: AAC priming, packet offsets and long-session drift still require
qualification. One short Linux boundary does not qualify every encoder/output
combination or Windows.


### 2026-10-26: Source-line timing provenance

The log adapter now attaches the raw timestamp, normalized UTC event time,
poll receipt time, file identity and exact line-start byte offset to emitted
lifecycle/marker events. Synthetic timeout, restart and user actions have no
fabricated line provenance. Byte positions account for blank lines, CRLF and
partial UTF-8 across checkpoints and truncation. The UI's authenticated last-event
snapshot exposes the structured source data; this does not claim that every raw
combat event is archived or embedded in video sidecars.

New regressions verify the delayed receipt independently of event time and preserve
byte offsets after restart. Full workspace nextest passed 93 tests with seven
opt-in skips after the helper extraction. The focused provenance regression and
strict Rust 1.95 workspace/all-target Clippy also pass.

The release-profile ingestion benchmark also passes with provenance enabled:
100,002 lines (10,197,039 bytes) in 0.606 seconds, expected lifecycle/death counts,
and no waiting batches. See [measured report](evidence/ingestion-provenance-2026-10-26.json).
This is the existing synthetic reference, not a gameplay CPU guarantee.


### 2026-10-26: Combined native rerun

The current application and OBS probe were rebuilt after the provenance changes.
The full native probe in `/tmp/owc-obs-rg6iaidc` passed manual and combat-log
capture, two-track export, delayed complete/partial history recovery, recovery
from already closed OBS files, and owned-recording shutdown. The integrated
frame checker measured 132.7 ms marker alignment error and consecutive source
frames 116 and 117 across the tested split. These repeat the short Linux
synthetic checks; the remaining platform and long-session gates still apply.


### 2026-10-26: Competing database writer

A second SQLite connection holds `BEGIN IMMEDIATE` while the library attempts a
combat-log checkpoint. The regression receives `DatabaseBusy` after the configured
three-second bound, retains the previous cursor and creates no partial activity.
After releasing the lock, retry persists the cursor and exactly one activity;
reopening the database confirms both. The focused test passes in 3.00 seconds.
This establishes bounded SQLite lock contention, not a bound on an unresponsive
filesystem or device. The library's strict Rust 1.95 Clippy check also passes.

Seven CI publication-policy tests pass against the current working tree. Their
archive/installer fixture output does not qualify real platform release bundles.


### 2026-10-26: Cancelled OBS command queue

Source review found that the OBS actor could send a queued mutation after its
reply receiver had been dropped by a timeout or cancelled caller. A socket-level
regression first demonstrated a cancelled `StartRecord` reaching the peer ahead
of `GetRecordStatus`. The actor now discards cancelled requests before sending.
The same regression passes with only the status query reaching the peer.

All seven OBS-client tests and strict Rust 1.95 all-target Clippy pass. This covers
requests still waiting in the actor queue. A command already sent may have taken
effect, so existing reconciliation and ownership guards remain necessary.


### 2026-10-26: Combined recovery and OBS queue verification

Workspace nextest run `0e513914-6bd6-458a-9cb6-4b820630b14e` passed all 95 tests
with seven opt-in environment tests skipped. This includes the competing-database
writer and cancelled OBS queue regressions alongside the provenance and delayed
completion changes. Strict Rust 1.95 workspace/all-target Clippy also passed.
Remote platform qualification still needs the current changes committed and run.


### 2026-10-26: Native OBS disk exhaustion

`python3 scripts/obs-disk-full-smoke.py` runs OBS with a private profile/display
and a 128 MiB tmpfs inside an isolated user/mount namespace. It saves a baseline
recording, starts another, and fills only that private filesystem until ENOSPC.
OBS 32.2.2 emits stopped, settles to inactive, retains both file entries and leaves
the closed baseline byte-for-byte unchanged. After removing the filler, recording
starts and stops again without removing the originals. The successful probe is
`/tmp/owc-obs-full-gcv_0jit`; see [report](evidence/obs-disk-full-2026-10-26.json).
The namespace is unmounted after the run; its report remains on disk.

The first run exposed a short delay between OBS's stopped event and its status
query settling; the assertion now waits up to five seconds. Another run showed
that the interrupted file can be empty because buffered bytes never reached disk.
The test therefore requires preservation of already closed footage, not invented
recoverability of interrupted output. OBS shows its insufficient-disk-space dialog.
This qualifies the native OBS failure/retry path on Linux, not companion-controlled
ENOSPC or Windows. Existing controller low-space and media ENOSPC tests cover those
components separately. The updated probe passes strict Rust 1.95 Clippy.


### 2026-10-26: Hosted Linux dependency-download timeout

Main CI job `121771335555` (run `40377435087`, source `89453f8`) was cancelled
at its one-hour limit during `apt-get install`, before Rust or package compilation.
The log shows slow Azure Ubuntu mirror transfers, including over eleven minutes
for the recommended 27.4 MB `pocketsphinx-en-us` speech model. This is not evidence
of a compiler, runtime or cache failure.

The dependency step now uses `--no-install-recommends`, explicit network
timeouts/retries, and its own 15-minute limit. An Ubuntu 24.04 container simulation
resolves the explicit dependency set without the speech-model package. Workflow
lint and all seven publication-policy tests pass. A real hosted install/build
rerun is still required; dependency resolution alone does not qualify runtime
behaviour. The completed job log is `/tmp/owc-linux-package-121771335555.log`.


### 2026-10-26: OBS-connected browser rerun

The latest combined run passed native capture/export, delayed complete/partial
recovery, closed-history recovery, marker alignment and native reconnect. Browser
validation exposed two fixture races: pointer geometry was sampled before the
ResizeObserver restored the visible pane width, and live OBS snapshots replaced
a deliberately synthetic pending-player state.

The test now waits for the visible width before the same real pointer drag, and
temporarily holds live state events only during synthetic rendering assertions.
It restores live updates before backend configuration, marker, export and shutdown
checks. Assertions were retained. A targeted OBS-connected browser rerun passes
in `/tmp/owc-browser-44jlxza3` (private OBS `/tmp/owc-obs-5a98nbmc`).
`OWC_BROWSER_SETUP_ONLY=1` runs this setup/UI check without repeating the unrelated
capture suite. The earlier standalone browser suite also passed in
`/tmp/owc-browser-c3fr76cu`. Probe formatting and strict Rust 1.95 Clippy pass.

Hosted package reruns remain blocked on committing/pushing: the configured SSH
signing agent alternates between refusing connections and having no identities.
Signing has not been disabled or bypassed. No release is published.


### 2026-10-26: Native test-session isolation

A private D-Bus activation had launched an extra `rosecd` while inheriting the
desktop runtime directory, disrupting the signing-agent socket. The extra test
daemon was stopped and the normal user service restarted; the user subsequently
unlocked the vault and signing succeeded. Native CI probes now enter through
`scripts/private-session.py`, which sets private runtime/config/data/cache paths
before starting D-Bus. It also terminates the private process group and cleans up
portal FUSE mounts without touching mounts outside that temporary directory.
The credential-store probe already sets private paths and keeps its own launcher.

The final isolated OBS/browser setup suite passes in `/tmp/owc-browser-f3vcm2x6`,
including wrapper cleanup. The desktop signing socket inode is unchanged before
and after the complete run. Workflow lint and all seven CI policy tests pass.

## Recovery and typed status validation (2026-10-27)

- Strict workspace Clippy and the 104-test nextest suite passed during the status/recovery pass; seven opt-in native tests remain excluded from that run. Isolated WebKit smoke tests passed, including unrecoverable/uncertain player states, hidden playback controls, OBS failure propagation and numeric progress.
- An isolated SQLite backup and timestamp-preserving copy of the Oct 26 source MP4 recovered both the Temple +20 and Blinding Vale +21 recordings through the normal app API and fast export worker. Both reached ready after media validation. Source size and modification timestamp remained unchanged; the live database was not edited. Local evidence: `/tmp/owc-source-recovery-z091ppb4/result.json`. Personal footage is not committed.
- Recovery tests cover empty folders, uncertain source timing, persisted unrecoverable results, retry, idempotent reconciliation and preservation of unowned files.
- “Unrecoverable” is scoped to footage available in the checked recording folder. Metadata remains available and a user can restore missing files and check again. Offline OBS, unverified timing and failed exports stay retryable.


## Public preview candidate (2026-10-27)

Commit `f2101a1` includes the review/settings layout, protected recording filter, audio-name metadata handling, per-installation automatic-recording thresholds and experimental delve detection. Focused application/parser nextest passed 81 tests with five opt-in skips; strict workspace Clippy passed. The isolated WebKit suite passed with before/after video pixel comparisons when switching tabs, Settings hit-testing over player controls, long-list layout and matching adjacent control heights. This does not establish correctness on every native GPU/desktop.

Remote [run 40501052235](https://github.com/jmylchreest/obs-wow-companion/actions/runs/40501052235) passed Linux and Windows quality checks and the native Windows package service, media, desktop and crash-recovery checks. Linux packaging stopped during dependency downloads; bundle and installer checks were therefore not reached. The following candidate changes the slow Ubuntu mirror and keeps bounded download timeouts. Final candidate package and installer results must be recorded before a preview is published.

Public screenshots use isolated fictional library data. The authorised real-gameplay excerpt is cropped, softened to obscure floating player names, and stripped of original audio. No live library or OBS configuration was modified for the capture.
