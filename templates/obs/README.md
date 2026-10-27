# OBS starter configuration

Native profile: `OWC-Starter/`. Native scene collection: `OWC-Starter-scenes.json`. The older `recording-starter.json` is a checklist, not an OBS import.

## Import

1. Finish active recording/streaming; export existing settings for backup.
2. Use **Profile → Import**, navigate into the `OWC-Starter` directory and click **Choose**. Import the directory containing `basic.ini`, not its parent or an individual file. Then select **OWC Starter** from the Profile menu.
3. Use **Scene Collection → Import**. Automatic discovery is optional; choose **No** to browse manually. Add `OWC-Starter-scenes.json`, confirm the checked row is detected as OBS Studio, and click **Import**. Then select **OWC Starter** from the Scene Collection menu.
4. Add game capture and audio sources to the empty scene. Nothing is captured until you do this.
5. Choose a dedicated recording directory or use the companion’s **Change folder** while idle.
6. Enable authenticated WebSocket separately and connect through Setup.
7. Verify an actual recording and each audio track.

The profile uses software x264 CRF 20, one-second keyframes, AAC tracks 1 and 2, MKV, 1280 × 720 at 30 FPS, and manual file splitting. Configure source routing in Advanced Audio Properties. Capture permissions, physical devices and output paths must be selected locally. See the [setup guide](../../site/guide/obs-setup.html).

## Validation and rollback

These files derive from isolated OBS 32.2.2 native configuration with machine paths, credentials and physical devices removed. Native configuration loading, recording, splitting and companion export passed on Linux with two audio tracks. Menu-driven profile/scene imports and rollback passed on Linux, preserving the pre-existing resources. Windows imports remain a release gate. Run `python3 scripts/private-session.py python3 scripts/isolated-obs.py` after building the probe to reproduce the native-load test.

See the [tested menu and recording screenshots](../../site/guide/obs-setup.html#starter). Select your previous profile and scene collection to roll back. If import offers to overwrite an existing resource, cancel and rename the imported copy. Do not delete recordings during rollback.
