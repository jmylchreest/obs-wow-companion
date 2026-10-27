# Recording metadata sidecars

Each exported `<recording-id>.mkv` has a neighbouring `<recording-id>.json`. The existing format remains `schema_version: 1`.

- `activity`: recording identity, installation UUID, name, outcome, start/end, warnings and every captured or user-created marker.
- `activity.markers`: entries with `at_ms`, `kind` and `label`. Kinds include `death`, `boss_start`, `boss_end`, `round_start`, `manual` and `combat_activity`. Combat-activity markers are bounded samples of group damage/miss events, used to draw estimated non-boss combat spans; they are not exact enter/leave-combat events.
- `plan`: export coverage and original source intervals. `actual_start_ms` is the origin used to align events with the exported video.
- `media`: probed duration, audio tracks and other media details.
- `annotations`: current `tags` and `protected` state, added when an export finishes or annotations change.

Marker playback position in milliseconds is `marker.at_ms - plan.actual_start_ms`. Activity timestamps normally use Unix milliseconds. Trimmed exports use the parent video's relative timeline; the same subtraction still applies. Markers outside retained footage are kept in metadata but are not clickable playback targets. Trimmed clips copy only markers within their chosen interval.

The companion still uses SQLite for its working library and live capture checkpoints. Sidecars are written on export and updated when saved markers or annotations change, using a temporary file and atomic replacement. A durable SQLite queue tracks each required sidecar update. Failed writes are shown as errors and retried; startup checks repair stale, missing or malformed sidecars from the database and existing video. Unknown schema versions are preserved. Sidecars are portable metadata, but automatic library reconstruction/import from them is not currently implemented. Copy both video and JSON when moving recordings outside the app.

## Warcraft Recorder compatibility

Inspected upstream revision `2643938980b9fe2031aa17275d3ee528bd2fd20e`:

- [Sidecar naming and writing](https://github.com/aza547/wow-recorder/blob/2643938980b9fe2031aa17275d3ee528bd2fd20e/src/main/util.ts) uses a JSON file beside an MP4 and scans MP4 files for its library.
- [Metadata types](https://github.com/aza547/wow-recorder/blob/2643938980b9fe2031aa17275d3ee528bd2fd20e/src/main/types.ts) include category, flavour, combatants, death records and dedicated dungeon/Shuffle timelines. They do not define a generic custom-marker field matching ours.

These are different schemas. We retain the companion schema to preserve installation identity, arbitrary markers, uncertain outcomes and export timing without inventing upstream combatant fields. No Warcraft Recorder import/export compatibility is claimed.

Death markers may include `recent_hits`, containing up to three observed incoming damage events from the preceding ten seconds, oldest first. Each hit has `offset_ms` relative to the death (zero or negative), `source`, `ability` and optional `spell_id`. Relative offsets survive clip trimming without a second clock conversion. Missing/empty recaps mean data was not captured, not that no damage occurred. These are observed hits, not a guaranteed killing-blow attribution; damage amounts are not currently stored. The viewer displays newest first.

## Portable player bundles

Sharing uses a separate `format: "owc-share", schema_version: 1` document in `recording.json`. Marker `at_ms` values and activity bounds are relative to the exported video's start. Recent-hit offsets remain relative to each death. Markers outside the selected range are omitted; activity bounds may extend beyond it so the player can clip combat shading correctly.

Paths, source plans, installation names/UUIDs and protection flags are omitted. Player names, marker text and labels remain. The standalone HTML embeds an escaped copy of the metadata for local viewing; editing `recording.json` alone does not change that copy. This is not a library-import format. See the [sharing guide](../site/guide/sharing.html) for the player workflow.
