# Inviting early testers

Use the GitHub preview as the main destination: downloads, setup instructions and reports should stay together. Start with a few players who already use OBS, including someone on Windows, before sharing more widely.

## GitHub announcement draft

**OBS WoW Companion — looking for a few early testers**

If you already record WoW with OBS, Companion organises your footage around raid pulls and keys. It follows the combat log, adds boss and death markers, and lets you jump back a few seconds before a death to see what happened. You can also export a clip with a small web player to share with your group.

It uses your existing OBS setup. There are Linux and Windows preview builds; Linux is my main development platform, and I would particularly appreciate feedback from Windows players. This is an early preview, so please try a short recording before trusting it with a whole raid night.

The most useful first test is simple: set it up, record a pull or key, review a death, then export a short clip. Let me know what worked, what was confusing, and anything that went wrong. Successful tests help too.

- [Preview downloads](https://github.com/jmylchreest/obs-wow-companion/releases)
- [Quick start](quick-start.md)
- [Screenshots and short demo](https://jmylchreest.github.io/obs-wow-companion/)
- [Send a test report](https://github.com/jmylchreest/obs-wow-companion/issues/new/choose)

You will need OBS and FFmpeg installed. Delves are experimental and off by default; broader Classic and PvP testing is still needed. There is no macOS build yet.

Development has used AI assistance for code, testing and documentation. The demo uses real gameplay with identifying details obscured and audio removed; its library entries and markers are fictional demonstration data.

## Reddit: points to cover in your own words

For r/wow, write the post yourself. Its [rules](https://www.reddit.com/r/wow/wiki/rules/) currently prohibit AI-written posts, require disclosure of AI assistance in tool development, and ask how a tool differs from existing options. They also limit self-promotion to once every seven days and expect community participation. Checked 27 October 2026; check again before posting. Other communities have their own rules.

Cover these points, using your own experience:

- Why you wanted encounter recordings while keeping your existing OBS setup and playing on Linux.
- What the app actually does: combat-log recording, a local library, death and boss markers, replay controls and a shareable player ZIP.
- How it differs from a standalone recorder: OBS remains the capture application, with Windows and Linux preview packages.
- What you have personally tested, and what you still need help testing. Avoid describing every supported parser as fully tested in game.
- The early-preview status, separate OBS/FFmpeg requirements and a link to the quick start.
- The kinds of feedback you want: setup friction, missed encounters, audio, reconnecting OBS and exports.
- All AI assistance used in development, testing, planning, documentation and preparation of the announcement.

Lead with the problem it solves for players. A short clip showing a death marker and the replay is more useful than a long feature list. Avoid presenting it as an official wow-recorder port or implying affiliation.

## Before sending

Confirm the preview download is public and both platform package and installer checks are green. Use the release link, not a temporary workflow artifact. Keep original gameplay, private session links and personal logs out of the post. Stay available to answer the first reports and record fixes in the changelog.
