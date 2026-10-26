# Recording ownership, storage and recovery

## Recording ownership

Starting automatic or manual capture while OBS is idle creates a managed recording in the dedicated spool. Ownership becomes confirmed only after OBS reports recording started. The companion can split or stop only a recording it still owns.

Connecting to an existing recording uses observe mode. The companion does not stop, split or pause it. Clips may wait for that file to close. Pausing, changing profiles, losing events or disconnecting makes timing/ownership uncertain; automatic extraction is withheld where continuity cannot be established. Stop the recording yourself in OBS, reconnect, then start a new recording.

The companion never sends streaming controls. Setup shows OBS rendering/output skipped-frame counters so capture load is visible. Investigate overload in OBS and lower recording load if needed.

## Files and quotas

The data directory contains SQLite metadata, a dedicated capture spool, exported videos and temporary playback previews. Spool and library directories must be separate and cannot contain each other. OBS and the companion must see the same filesystem path.

Only registered managed source segments inside the dedicated spool are automatically removed. Open files, active encounters and unfinished exports retain their sources. Oldest unprotected ready recordings are eligible for library cleanup. A recording used by an unfinished trim is pinned even after the trim fails. Finish or delete the failed job to release that pin.

Protection prevents deletion; favourites are a searchable `favourite` tag and do not themselves prevent deletion. Storage limits are best-effort working limits: long encounters, protected recordings and temporary exports can exceed them. Low space stops a clearly owned recording while preserving pinned footage. External OBS recordings are never deleted.

If OBS itself runs out of disk space, it may show an error dialog. Free space before
trying again. Previously closed footage is retained, but the interrupted file may
be empty or incomplete and cannot be promised recoverable.

## Restart and failed jobs

Export plans and precise/fast mode are durable. An interrupted export becomes pending after restart, and a validated published file is reused rather than creating a duplicate. Failed jobs retain their sources; use Retry after fixing permissions, free space or FFmpeg configuration.

An encounter or manual clip active during application restart becomes abandoned with a warning. Manual start and finish transitions are committed with their log checkpoint, so a completed manual clip cannot be revived by an older checkpoint. Only confirmed closed segments are eligible for recovery; an interrupted/open OBS file is not assumed complete. Unindexed files remain on disk for manual recovery. The companion does not silently remux or delete them.

System clock changes invalidate the current timeline. Ambiguous local log timestamps at daylight-saving changes are rejected rather than assigned a guessed offset. Check log-error counts and keep the system clock synchronized before a session.

## Playback

Original MKV exports retain every recorded audio stream. Playback uses the original where supported, otherwise preparing a compatible H.264/AAC MP4 for the chosen track and leaving the original intact. Automatic encounter exports default to fast copying; fast cuts expand to the preceding keyframe and store that adjusted timeline origin. Explicit range exports remain precise. Closed, continuous retained footage can prepare playback before final export validation completes, without replacing playback when the saved clip becomes ready.

Use **M** in the app to start/end a manual interval while capture is active, and **Space** to play/pause. Shortcuts do not run while editing form controls. Closing the native window requests Quit, which stops owned recording and saves unfinished work for restart. You do not need to pause or wait for exports. Startup is opt-in, and automatic capture starts on a fresh supported encounter once OBS is connected and setup is ready.

## Metadata reconciliation and remaining limits

Metadata edits and export completion enqueue a sidecar update in the same SQLite transaction. The queue entry is removed only after the JSON sidecar has been atomically replaced. On startup, saved recordings are checked again; matching sidecars are left alone, stale ones are updated, and missing or malformed sidecars are rebuilt from database metadata and the video. Failed writes remain queued and retry, with an error shown in the app. An unsupported sidecar schema is preserved and reported rather than overwritten.

This closes the database/sidecar crash window without requiring a cross-file transaction. SQLite remains the working source of truth; deleting the database and rebuilding the whole library from sidecars is not implemented.

The process-kill export smoke test passes on Linux, including orphan-worker isolation, retry from a saved plan, preserved source footage, and one validated final output. Its worker gate explicitly ignores startup FFmpeg version checks and interrupts an actual export part.

Power-loss/filesystem durability and native Windows process-crash behavior still require qualification. An open or unindexed OBS recording cannot be reconstructed reliably from metadata alone; those files are preserved for manual recovery.


## Cancelling clip preparation

While a selected recording is being prepared, **Cancel preparation** stops its
media worker. Wait for the cancellation result, then choose **Retry** when ready.
The unfinished job retains its plan and source footage across restarts; it does
not immediately restart itself. Cancellation does not stop OBS or delete the
recording. Cleanup waits for the worker result and preserves the failed job's
sources. If publication has already completed, the completed result is retained.


Normal Quit or window-close checkpoints active intervals, stops companion-owned
capture and cancels unfinished media preparation. Automatic clip plans resume on
the next launch; sharing ZIP exports need an explicit retry. The Auto recording
preference is unchanged. A bounded cleanup wait replaces the old requirement to
pause and finish exporting before quitting. Existing user-owned OBS recording
continues.
