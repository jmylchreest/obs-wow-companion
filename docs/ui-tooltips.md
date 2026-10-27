# UI tooltip inventory

This lists application-authored hover text and custom popovers, plus the bundled player's standard control tooltips. Dynamic values below come from the selected recording or current status. Accessible labels without hover text are not tooltips.

## Header and navigation

| Location | Text / contents |
| --- | --- |
| OBS status | Click popover: connection and relevant readiness/error details. No native hover tooltip. |
| Recordings / Settings | No tooltip; labelled tabs. |
| Automatic recording | No tooltip; switch label says Auto recording on / Auto recording paused. |
| Record now / Stop recording | No tooltip. |
| Quit | No tooltip. |

## Recording list

| Location | Text / contents |
| --- | --- |
| Collapse arrow | Collapse recordings / Show recordings |
| Pane divider | Drag to resize. Arrow keys adjust width; double-click resets it. |
| Favourite filter | Show favourites |
| Protected filter | Show protected recordings |
| Recording heart | Favourite / Remove favourite |
| Recording lock | Protect recording / Unlock recording |
| Recording trash | Delete recording (Shift-click skips confirmation), or Unlock recording before deleting |
| Recording title row | Full title including difficulty/key, activity kind, outcome, error and warnings |
| Duration / processing state | Duration again when ready; otherwise detailed processing reason |
| Labels beneath a recording | Full comma-separated label list, including truncated labels |
| Raid result | Boss killed / Wipe |
| Mythic+ result | Depleted: completed over time; Timed: +N keystone upgrade; or Completed; upgrade timing was not recorded |
| Manual installation selector | WoW installation for clip markers |
| Activity, outcome, difficulty, key, dates, sort | No application tooltip; standard native input affordances may vary by OS. |

## Video and review

| Location | Text / contents |
| --- | --- |
| Playback status beside title | Playback mode and compatibility reason; initial recording warnings/errors |
| Play button | Play / Pause / Replay (Video.js) |
| Volume button | Mute / Unmute (Video.js) |
| Playback speed | Playback Rate (Video.js) |
| Fullscreen button | Fullscreen / Exit Fullscreen (Video.js) |
| Seek bar | Hovered video time (Video.js) |
| Audio track selector | Audio track |
| Add-marker flag | Add marker |
| Eye | Marker filters |
| Event signpost | Event time and event label; replaces timeline hover while the marker is hovered |
| Combat shading | Boss label or Combat activity (estimated); bands have pointer events disabled, so this is generally not directly hovered. |
| Range handles | Start: N.NNs / End: N.NNs |
| Zoom reset | Reset video zoom |
| Death row | Review 5 seconds before death, or Outside the retained footage |
| Death count | None |
| Recent hits | None. |
| Retry failed recording | Retry failed export |
| Footer Delete | Delete recording (Shift-click skips confirmation), or Unlock this recording before deleting. |
| Export controls / metadata disclosure / labels input | No application tooltip. |

## Events

| Location | Text / contents |
| --- | --- |
| Footer status | Full status text, useful when the visible line is truncated |
| Watcher status line | Full watched log-file path |
| Event rows, search, Follow, Save log, resize handle | No hover tooltip. Resize instructions are documented; the handle has an accessible label. |

The standalone sharing viewer uses the bundled player's normal control tooltips and timestamp/event-label signpost titles. It has no death-count or recent-hit hover text.

The export path folder icon has the tooltip “Choose ZIP destination”. It opens the native desktop save dialog. Audio track names and export metadata are displayed directly, without additional hover descriptions.

The date range reset button has the tooltip “Clear date filter”; it resets both dates immediately.
