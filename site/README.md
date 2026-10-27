# Project website

Static GitHub Pages site for WoW players. No build step, analytics, remote fonts or JavaScript dependencies.

The workflow in `.github/workflows/pages.yml` publishes this directory on changes to `main`.
The public URL is https://jmylchreest.github.io/obs-wow-companion/.

## Captures

Build the app, then run `python3 scripts/private-session.py python3 scripts/capture-showcase.py` for generated test footage, or add `--sanitised-video /path/to/reviewed-demo.mp4` for an already reviewed, silent 30-second clip with two muted audio streams.

The script uses a private Xvfb/WebKit session, a disposable library and fictional encounter data. It does not read the live library or change OBS. `review-demo.mp4` shows playback, a death jump, half speed and selecting an export range.

The current public assets use a real Ruby Life Pools +17 excerpt (Kokia Blazehoof), provided with permission on 27 October 2026. The source was cropped, broadly softened to obscure floating player names, and muted. This is labelled in the video and public copy. The selected dungeon and key level match the source footage. Other library entries, characters, markers and labels are demonstration data. No original footage, private combat logs or user configuration is included.

Review the video and screenshots before publishing replacements. Run `python3 scripts/private-session.py python3 scripts/site-smoke.py` to render desktop/mobile views and check local resources.
