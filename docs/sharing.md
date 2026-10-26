# Sharing recordings

Select a ready recording, expand **Export** (collapsed by default), choose audio tracks and a destination `.zip` path, then click **Export ZIP**. Use the folder button beside the path to choose a destination with the desktop save dialog. The initial destination is the companion data directory's `exports` folder. You may enter another absolute path. Existing files are never overwritten; choose a new filename for another export. In browser-only mode, enter the path directly. The picker returns a path; it does not start exporting or overwrite an existing file.

The full recording is exported unless **Select range** is active. Range handles and Start/End fields select a precise cut, translated from the currently displayed video's actual origin. Full H.264/yuv420p recordings with AAC audio are remuxed without video/audio encoding. Other formats require conversion; precise cuts use the configured export encoder with the existing CPU fallback. Select one track to keep it, several to mix them into one AAC track, or none for silent video. Mixing normalizes the selected inputs to avoid simple summation clipping; it is not a loudness mastering pass. Track 1 is selected initially because it may already contain the complete OBS mix. Selecting that mix together with its individual source tracks can duplicate sound. The metadata lists the selected one-based source track numbers.

The export runs in the background with progress in the export panel and Events. **Cancel export** stops preparation or ZIP copying; wait for “Export cancelled” before retrying. Source footage and the previous successful bundle remain intact. If publication already completed before cancellation arrived, the completed bundle is kept. Recording/source housekeeping and deletion protect its inputs while it runs. Quit cancels sharing work and briefly waits for cleanup; retry an interrupted sharing export after restarting. Automatic recording exports remain queued for the next launch. A `poster.jpg` preview is generated from the exported video and shown before playback. It is at most 640 pixels wide and contains the same visual content as the shared video. The finished MP4 is checked for duration, audio-track count and full decodability before packaging. This adds a validation pass, shown as “Checking saved video and audio”; a malformed video is rejected even when its container metadata is readable. The ZIP is streamed to disk and published atomically only when complete, with ZIP64 for large videos. It is stored without compression because the video is already compressed. Allow disk space for temporary MP4/trim files plus the final ZIP. An interrupted sharing export is not automatically resumed; retry it. A crash can leave hidden temporary files in the destination directory; no incomplete final ZIP is published.

## Contents

- `index.html`: standalone player with embedded metadata for local viewing.
- `video.mp4`: browser-compatible H.264/AAC media.
- `recording.json`: versioned portable metadata.
- `assets/`: bundled Video.js, fonts, player JavaScript and CSS.
- README and software licenses.

Extract everything together and open `index.html`. If the browser blocks the local video URL, use **Open video** and select `video.mp4`. The player includes playback speed, fullscreen, seekable signposts, dynamic marker categories, combat shading, five-second death pre-roll, recent hits, fine seek and video zoom/pan. It is read-only and does not contact the companion or external services.

For web hosting, upload the extracted directory to a static host. Serve MP4 with the correct MIME type and byte-range support for seeking. The ZIP itself is a download artifact, not a website. Community upload/search/accounts are not implemented.

## Metadata

The share format is `format: "owc-share", schema_version: 1`. Marker `at_ms` values and activity bounds are relative to this exported video's start. Recent-hit offsets remain relative to each death. Markers outside the exported range are omitted. Activity bounds may extend outside a selected clip, allowing the viewer to clip raid combat shading accurately.

Local filesystem paths, source plans, installation names/UUIDs and protection flags are omitted. Character/player names, marker labels, user tags and the video's visible/audible content remain. The sharing file is separate from the original library sidecar and is not a library-import format. JSON embedded in HTML is escaped to prevent marker labels or names from becoming markup/scripts. Editing `recording.json` alone does not update the HTML's embedded copy.

## Track names and export details

Track labels use names stored in the original media file, with numbered fallbacks when names are absent. The app does not infer which application or microphone produced a track. In OBS, use **Settings → Output → Advanced → Audio** to name tracks for future recordings.

**Video details** shows Original video and Latest exported bundle tables side by side (stacked in narrow windows). The latest successful bundle records its location, completion time, ZIP size, selected range/source tracks, and the actual exported MP4's duration, codec, dimensions and audio. These details survive restarting. Failed exports do not replace the previous success. Older exports from before this feature are not reconstructed, and moving or deleting a ZIP outside the app does not update its recorded location. Deleting a library recording removes this metadata but does not delete independently shared ZIPs.

A generated 12-second fixture can be recreated with `python3 scripts/create-audio-demo.py /tmp/multitrack-demo.mkv`. It contains a full mix plus separate 440 Hz and 880 Hz demonstration tracks. Select the full mix alone, or the two separate tracks together. It is a standalone test file, not automatically added to your personal library.

In the desktop app, **Video details → Open folder** opens the containing folder
for the original clip or the latest exported bundle. Missing or moved files show
an error. These buttons are hidden in browser-only mode.
