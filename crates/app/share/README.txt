Extract the whole ZIP, then open index.html in a modern browser. If local video loading is blocked, use Open video and choose video.mp4. No companion, OBS, account, internet connection or installation is required. Browser/platform codec support can vary.

To host: upload the extracted folder to a static HTTPS host. Serve MP4 with video/mp4 and byte-range support for seeking. All assets are local; no external CDNs or analytics are used.

recording.json uses owc-share schema version 1. Marker at_ms values are milliseconds from this video start. Recent-hit offsets remain relative to each death. Local source paths and installation IDs are omitted. Player names, labels, chat/voice and anything visible/audible in the recording are retained. The ZIP contains the selected audio track only.

The JSON is also embedded in index.html to avoid file:// fetch restrictions. Editing the standalone JSON does not change the embedded copy. The player is read-only.
