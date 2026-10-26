# Project website

Static GitHub Pages site for WoW players. No build step, analytics, remote fonts or JavaScript dependencies.

The workflow in `.github/workflows/pages.yml` publishes this directory on changes to `main`.
The public URL is https://jmylchreest.github.io/obs-wow-companion/.

## Captures

Run `python3 scripts/capture-showcase.py` after building the app.
It uses isolated Xvfb/WebKit sessions, generated silent video and fictional encounter data.
It never opens the user's library or OBS configuration. Screenshots show the real application.
`review-demo.mp4` demonstrates playback, seeking and half speed.
Do not replace these with private recordings or logs without permission.

Run `python3 scripts/site-smoke.py` to render desktop/mobile views and check local resources.
