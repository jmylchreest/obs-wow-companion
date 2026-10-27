# First-release completion audit

Status: **not qualified for a stable release**. Last reviewed 2026-10-27.
The [0.1.0-dev.1 testing preview](https://github.com/jmylchreest/obs-wow-companion/releases/tag/v0.1.0-dev.1) passed all automated publication gates and is public. Its gate is documented in [releasing](releasing.md); a preview does not waive these stable-release requirements. This audit preserves the
full [specification](specification.md), including the former 1.0 scope. Passing a
unit test does not establish native gameplay compatibility. Current external
checks are described in [validation](validation.md); rerun them against the final
candidate. Numbers below follow the specification's acceptance-criteria order.

## Acceptance evidence and remaining work

| # | Requirement | Evidence and remaining gate |
| --- | --- | --- |
| 1 | Both-OS OBS playback and setup failures | Linux isolated/browser checks recorded; native Windows credential round trip passed remotely; capture, playback and permissions checks remain outstanding. |
| 2 | Delayed start and retained pre-roll | Timeline tests cover retained intervals. A controller regression accepts a 30-second delayed start covered by the current session's retained segment and rejects absent or stopping captures. A partial-coverage regression retains the encounter and produces an explicit truncated plan; isolated native Linux OBS also passes a 30-second delayed automatic encounter with complete coverage, 1,004 ms retained pre-roll and two audio tracks. Native Linux partial-history export also passes with an explicit warning and playable retained footage. Backdated completion after OBS stops now retains metadata and recovers closed footage; the private native probe produces a complete two-track clip without restarting OBS. Windows validation remains. Encounter-triggered capture cannot supply footage before OBS started. |
| 3 | 45-minute nested M+ | 45-minute file-watcher fixture passes with five nested bosses/deaths, periodic actions, five-minute timeout, one successful run and no duplicate raids. Native media timing is covered separately. |
| 4 | Consecutive overlapping pulls | Controller overlap and timeline tests pass; verify published clips against frame-clock source. |
| 5 | Rotation, truncation, UTF-8, replay | Tailer restart/backlog and UTF-8 tests pass. Watcher now defers expiry while unread batches remain. Broader live-delivery qualification remains. |
| 6 | Clock/pause/disconnect gaps | Clock-jump, missed-event and disconnect tests pass; sleep/resume and real output transitions remain runtime gates. |
| 7 | Split continuity | Synthetic media continuity and isolated OBS checks recorded; long-session qualification for each supported native output combination outstanding. |
| 8 | Marker/cut alignment | Media cut and origin tests pass. Private Linux OBS marker/frame calibration measures 133.3 ms error and rejects a three-second negative control; Windows and real gameplay calibration remain required. |
| 9 | User-owned output safety | Ownership and reconnect mock tests pass; isolated OBS evidence recorded. Native Windows counterpart outstanding. |
| 10 | Streaming safety/load | Scene override/streaming guards tested; concurrent stream load/drop qualification outstanding. |
| 11 | Crash export recovery | Durable plan tests and process-kill recovery checks passed on Linux and Windows in the published preview gate. Real gameplay and disk-pressure qualification remain separate. |
| 12 | Quota and disk exhaustion | Cleanup and low-space controller/mock-OBS tests pass, including one owned stop and preserved sources; real private-tmpfs FFmpeg and ZIP ENOSPC/retry checks pass. SQLite ENOSPC atomic rollback/reopen/retry also passes. Controller acknowledgement retention and retry after a real checkpoint ENOSPC also pass; private native Linux OBS disk exhaustion now stops output, preserves closed footage byte-for-byte and allows restart after freeing space. Native Windows and companion-integrated disk-full qualification remain. |
| 13 | Audio tracks across both targets | Real FFmpeg track preservation, named tracks and mixing tests pass locally; native Windows playback/export qualification outstanding. |
| 14 | 10x ingestion benchmark | Release-profile watcher/bus/SQLite benchmark passes: 100,002 lines in 0.620 seconds, expected lifecycle/marker counts retained and acknowledged queue bounded. See dated evidence for the defined synthetic reference and scope. |
| 15 | Queue latency and progress | Isolated native Linux OBS probe measures final closure to exporting/ready snapshot at 64.54 ms with an idle worker (100 ms sampling). Competing-load and Windows qualification remain. |
| 16 | Versioned game-mode coverage | Synthetic Retail/Classic fixtures pass. Real Temple log replay was inspected locally, but comprehensive labelled client/mode fixtures remain missing. |
| 17 | User documentation | Six HTML player guides cover setup, OBS, review, sharing, troubleshooting and compatibility. Contributor references and historical evidence are indexed separately. |
| 18 | All former 1.0 features | Broad implementation exists; native Windows and real PvP/Classic validation remain unproven. |
| 19 | Native starter imports | Linux isolated import/recording evidence recorded; clean Windows import and local-source playback outstanding. |
| 20 | Safe import and rollback | Template checker and historical Linux rollback checks pass; Windows counterpart outstanding. |
| 21 | Recipe versus native exports | Documented in OBS setup and templates; include in final walkthrough review. |
| 22 | New-user walkthrough | Existing screenshots/evidence cover portions; complete final candidate walkthrough on both targets remains. |

## Current verification

The published preview at `b223f592a01d7113fa3d7f8614686c0b5e25a063` passed all automated publication gates, including native Windows packages, installers and crash recovery. [Validation](validation.md) records the run and current evidence. Older checks remain in the [dated journal](archive/validation-2026-10.md); their earlier failures and pending checks must not be read as current results.

## Additional source-contract gaps

The acceptance checklist is not the entire specification. Source inspection on
2026-10-26 identified these media-adapter requirements, now implemented:

- Thumbnail generation is implemented in the media adapter and used for shared
  web-player posters. Decode/source-preservation, ZIP inclusion and hosted WebKit
  poster loading/visibility before playback pass.
- Sharing exports now expose cancellation during media preparation and ZIP writing.
  Linux WebKit cancellation, child-process teardown, ZIP-writer cancellation and
  retry are verified. Automatic encounter-export cancellation is implemented with
  tested staging cleanup, source preservation and durable retry state. Linux
  WebKit Cancel preparation and successful Retry with real process teardown pass;
  native Windows validation remains outstanding.

- Desktop Open folder actions now exist for original clips and exported bundles.
  Path validation and WebKit bridge wiring pass; real native file-manager opening
  remains a platform walkthrough check.

The log adapter now combines native filesystem notifications with periodic
reconciliation, using one coalesced pending wakeup and preserving checkpoint
backpressure. Linux create/append/rotation and missing-folder recovery tests pass;
Windows native behaviour remains a validation gate.

Continue auditing the complete source contract; this list does not establish that
all first-release features are finished.

The repository contract also has a competing-writer regression: SQLite returns
`DatabaseBusy` after three seconds without advancing the cursor or partially
inserting an activity; retry and reopen preserve exactly one durable activity.
These regressions are included in workspace tests. They do not prove bounded I/O on an unresponsive filesystem.

## Remaining stable-release work

1. Complete native Windows and GNOME/KDE Wayland capture/playback walkthroughs, including permissions, audio and existing OBS profiles.
2. Gather permitted real client-mode fixtures and verify marker timing, split continuity and overlapping encounters under load.
3. Finish platform-specific disk-pressure, sleep/resume and streaming qualification. Preserve failed-job sources and never publish partial assets.
4. Re-run the automated release gates against the final candidate and retain exact evidence.

A public preview invites this testing; it does not waive the specification.
