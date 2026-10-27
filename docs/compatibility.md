# Compatibility qualification

No platform/game combination is yet qualified for a stable release. This matrix distinguishes implemented behaviour from observed runtime evidence. All first-release requirements in the [specification](specification.md) still apply.

## Platform and media

| Combination | Evidence | Remaining qualification |
| --- | --- | --- |
| Linux x86-64, X11, OBS 32.2.2 / WebSocket 5.7.4 | Native window, authenticated control, imported starter settings, split recording, manual and log-triggered export, two audio streams, preview playback | Actual WoW capture, fullscreen transitions, suspend/resume, extended recording load |
| Ubuntu 24.04 hosted Xvfb, OBS 32.2.0 / WebSocket 5.7.4 | Native isolated capture, split/export, two audio tracks, delayed full/partial history, reconnect, browser setup and owned shutdown passed in [run 40375838240](https://github.com/jmylchreest/obs-wow-companion/actions/runs/40375838240) | Synthetic sources only; not actual game or Wayland capture |
| Linux GNOME Wayland | Capture delegated to OBS and portal; no runtime evidence | Portal selection, permission recovery, restart, actual game/audio capture and playback |
| Linux KDE Wayland | Capture delegated to OBS and portal; no runtime evidence | Same checks as GNOME, independently |
| Windows x86-64 | Native quality and packaged service/media, desktop and crash-recovery checks passed in [run 40501052235](https://github.com/jmylchreest/obs-wow-companion/actions/runs/40501052235) | Real OBS/game capture, player audio, startup shortcut, OBS imports and extended load; installer gate awaits a complete two-platform bundle |
| macOS / other architectures | Outside first-release support | Separate future qualification |

The tested starter uses x264, AAC, MKV, 720p/30 and manual splitting. Other encoders, frame rates, containers and OBS versions need their own continuity/marker checks. Precise exports are H.264/AAC MKV; default Fast copy exports retain source codecs in MKV; browser previews are H.264/AAC MP4 with one selected audio stream. FFmpeg/ffprobe are installed separately. There is no bundled media binary or universal GPU claim. Advanced settings offer NVENC, QSV, AMF and Linux VAAPI; unavailable encoders fall back to CPU. Native VAAPI export is tested on the development host, while NVENC/QSV/AMF still require hardware-specific qualification.

## Game activities

| Activity | Implemented behavior | Fixture evidence | Real client evidence |
| --- | --- | --- | --- |
| Retail raid | Encounter boundaries, kill/wipe and reset handling, difficulty and markers | Synthetic parser tests and synthetic log through real OBS | Development-host Kith’ix recordings inspected; broader labelled client/build qualification pending |
| Retail Mythic+ | Whole run, nested bosses, key metadata and abandonment | Synthetic 45-minute run and duplicate-start regression | Development-host runs and source recovery inspected; broader labelled client/build qualification pending |
| Arena / skirmish | Match boundaries and result metadata where logged | Synthetic parser coverage | Pending |
| Solo Shuffle | Match with round markers and zone exit | Synthetic state-machine coverage | Pending |
| Delves (Retail, experimental) | Entry/exit from difficulty 208 zone events; deaths and logged boss markers; unknown result | Synthetic lifecycle tests | Pending real delve logs and gameplay validation; tier and completion are not inferred |
| Battleground | Explicit zone boundaries, unknown result when unavailable | Synthetic zone handling | Pending; each advertised battleground needs labelled logs |
| Classic family | Flavour selection and supported encounter/zone events | Synthetic Classic fixture | Pending for each named client version and activity |

Synthetic fixtures are independently authored examples, not recordings from a game client. They establish behaviour for their input shapes and cannot prove that a current client emits those shapes. Unsupported or ambiguous outcomes remain unknown; this is not a claim of universal Classic/PvP coverage.

## Collecting release evidence

Record OS/build, desktop/session, OBS/WebSocket version, encoder/settings, WoW flavour/build, activity type and start/end outcome. Preserve a representative combat log with permission to use it; anonymise character names consistently if needed while keeping timestamps and event structure intact.

For each supported combination, record a visible clock and continuous audio signal across splits. Compare start/death/boss markers with the recording, including a delayed start, consecutive pulls, abandonment and a zone change. Verify live review and each audio track while capture continues. Test ownership by starting a recording manually, connecting/disconnecting the companion and confirming OBS remains recording. Exercise pause/resume, crash/restart, disk pressure and streaming concurrency in a disposable setup.

Keep expected results and measurements with each fixture. A successful build, API connection or synthetic clip does not complete these checks. See [validation](validation.md) for executed checks and [OBS setup](obs-setup.md) for configuration steps.

### Delve recording (experimental)

Enable **Delves** under **Settings → WoW installations → Automatic recording** for each existing Retail installation, then save preferences. Delves are off by default for new installations. Enable `/combatlog` before entering the delve; automatic logging add-ons must also have delve logging enabled.

Recording starts on a `ZONE_CHANGE` with difficulty 208 and ends on a valid zone change out of that delve, followed by the configured post-roll. Duplicate entry events do not restart it. Boss kills and wipes remain markers within the same recording. You can stop it early using the recording controls; the usual inactivity timeout also applies.

Delve difficulty 208 is identified by [LoggerHead's difficulty mapping](https://github.com/dratr/loggerhead/blob/master/LoggerHead.lua). This implementation uses the existing combat-log zone-event format and has synthetic coverage, but has not yet been validated with a real delve log. It does not use in-game add-on events such as `SCENARIO_COMPLETED` as though they were file events. A zone exit is saved with an unknown outcome, not a claimed completion. Tier, bountiful status and rewards are not available through this detection. Starting Companion or enabling logging after entry may miss the start; use **Record now** in that case.
