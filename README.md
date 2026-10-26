# OBS WoW Companion

**Record the pull. Learn from the replay.**

A free, open-source companion for World of Warcraft players who use OBS. Automatically record encounters, jump back to a death, slow down a boss mechanic, and share the moment with your group.

[Take a look](https://jmylchreest.github.io/obs-wow-companion/) · [OBS setup guide](docs/obs-setup.md) · [Compatibility](docs/compatibility.md) · [Report a problem](https://github.com/jmylchreest/obs-wow-companion/issues)

![The recording library and video review player, using fictional encounters and generated test footage](site/assets/review.png)

> **Development preview:** the app works in local Linux testing, but it is not yet a release-qualified product. Windows is a first-release target and still needs native validation. Check the [compatibility matrix](docs/compatibility.md) before relying on it for raid night. Screenshots use synthetic test data, not your logs or live gameplay.

## What you can do

- **Record automatically.** Supported encounters in your WoW combat log start OBS recording. Use **Record now** for manual clips.
- **Review the important moments.** Click a death to seek five seconds before it. Boss and combat markers show where the action happened.
- **Watch at your pace.** Fullscreen, slow playback, fine seeking, zoom and selectable audio tracks.
- **Find your runs.** Filter by key level, raid difficulty, outcome or date. Add labels, favourite pulls and protect recordings you want to keep.
- **Share a useful replay.** Pick a range and audio tracks, then export a ZIP containing the video, markers and a portable web player.
- **Keep disk use under control.** Separate recording and library budgets, with protected clips and unfinished work kept safe.

OBS still handles your scenes, game capture, microphone and encoding. Companion does not replace OBS or require an account.

## Get started

### 1. Get a working OBS recording

Install **OBS Studio** and **FFmpeg, including ffprobe**. Make sure OBS records the right WoW window and the sound you want.

Already use OBS? Keep your scenes. Starting fresh? There is an optional [OBS starter profile and scene collection](templates/obs/README.md). You still select your own window, audio devices and recording location.

Linux needs GTK 3 and WebKitGTK 4.1 runtime libraries. Windows needs WebView2 Runtime. OBS and FFmpeg are installed separately.

### 2. Launch Companion and follow setup

Use a [development build](#development-build). The setup wizard walks you through:

1. **OBS connection:** in OBS, open **Tools → WebSocket Server Settings**, enable the server and authentication, and enter its password in Companion.
2. **Recording settings:** choose the companion recording folder and check the recommended output settings.
3. **Picture and sound:** confirm your existing OBS setup captures what you want.
4. **WoW installs:** select detected installations, search another folder, or enter your WoW **Logs** directory manually. Multiple installs are supported, including Wine/Proton locations on Linux.

The detailed [OBS guide](docs/obs-setup.md) covers platform capture choices, multiple audio tracks, scene switching and troubleshooting.

### 3. Turn on combat logging and play

Enter `/combatlog` in WoW, or use your preferred logging addon. Leave OBS and Companion open with **Auto recording on**.

A fresh supported encounter starts recording automatically. After it finishes, Companion keeps the configured extra footage, closes capture it owns, and prepares your clip. You can review closed footage while a longer recording continues.

**Try a short recording first.** Verify the picture, game sound and microphone before a long session.

## Good to know

| Question | Answer |
| --- | --- |
| Does it record before a pull? | Only when that footage already exists in retained files. Starting OBS at the pull cannot recover earlier video. |
| What if OBS is already recording? | Companion observes your recording. It does not stop or split it, and clips may wait until you stop it in OBS. |
| Can I keep streaming? | Companion leaves streaming alone. Recording uses OBS's program output; it does not create a separate game-only recording composition. |
| What if I close OBS? | Companion reconnects automatically. Interrupted footage can be unavailable; the encounter is kept with a warning. |
| Do I need to wait before quitting? | No. Quit stops owned capture and preserves unfinished automatic clips for processing next launch. Interrupted sharing ZIP exports need retrying. |
| What game modes work? | Coverage varies by version and mode. The first-release scope includes raids, Mythic+, arenas, Solo Shuffle, battlegrounds and Classic. See the [tested compatibility matrix](docs/compatibility.md). |
| Are recordings uploaded anywhere? | No automatic uploads. Recordings stay on your machine. Exporting and sharing are your choice. |
| Is macOS supported? | Not in the first release. Windows x86-64 and Linux x86-64 are the release targets. |

## Sharing with your group

Open a recording, expand **Export**, select a range if needed, choose the audio tracks and save the ZIP. Extract it and open `index.html` to use the included player. If the browser blocks local video access, use its video-file picker. You can also host the extracted bundle on a static website.

[Sharing guide](docs/sharing.md) · [Watch the actual controls with demo footage](https://jmylchreest.github.io/obs-wow-companion/#demo)

## Development build

There is no release-qualified installer yet. Successful [GitHub Actions package builds](https://github.com/jmylchreest/obs-wow-companion/actions) provide development artifacts when available. Those are test builds, not a compatibility guarantee.

To build from source, install **Rust 1.95 or newer**, clone this repository, then run:

```sh
git clone https://github.com/jmylchreest/obs-wow-companion.git
cd obs-wow-companion
cargo run --locked --bin obs-wow-companion
```

On Ubuntu 24.04, install the build dependencies first:

```sh
sudo apt install build-essential pkg-config libgtk-3-dev libwebkit2gtk-4.1-dev libdbus-1-dev ffmpeg
```

On Windows, use the MSVC Rust toolchain, Visual Studio C++ Build Tools, and WebView2 Runtime. Install OBS and FFmpeg separately, with `ffmpeg` and `ffprobe` on your PATH or selected in Companion's Media tools settings.

Optional launch arguments:

- `--browser`: open the interface in your browser.
- `--no-open`: run the local service and print its private session link.
- `--data-dir /absolute/path`: use a separate library and settings directory.

## Help test it

[Open an issue](https://github.com/jmylchreest/obs-wow-companion/issues) with your operating system, OBS version, game version and a short description of what happened. The app's **Events** panel helps explain connection, recording and export problems. Remove passwords and personal information before sharing diagnostics.

For contributors: [implementation plan](docs/implementation-plan.md), [specification](docs/specification.md), [release audit](docs/release-audit.md), [validation evidence](docs/validation.md), [CI](docs/ci.md) and [release instructions](docs/releasing.md).

## License

[MIT](LICENSE). Bundled dependency notices are in [THIRD-PARTY-NOTICES.txt](THIRD-PARTY-NOTICES.txt).

An independent community project, not affiliated with Blizzard Entertainment or the OBS Project. World of Warcraft is a trademark of Blizzard Entertainment.
