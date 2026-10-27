# Validation evidence

Reviewed 27 October 2026. The public `0.1.0-dev.1` preview passed all automated publication gates at `b223f592a01d7113fa3d7f8614686c0b5e25a063`. It is not a stable-release qualification for every desktop or WoW version.

## Current baseline

| Check | Evidence | Limit |
| --- | --- | --- |
| Linux and Windows quality | [Tagged release run](https://github.com/jmylchreest/obs-wow-companion/actions/runs/40552480545): formatting, strict Clippy, nextest, doc tests and policy passed | Opt-in environment tests run separately; synthetic tests do not establish gameplay coverage |
| Native packages | Both extracted packages passed service/media, native desktop and process-crash recovery; Linux WebKit playback passed | Real Windows OBS/game capture and audio still need qualification |
| Installers | Both rejected a changed archive, installed the genuine package and passed installed-service checks | OS runtime dependencies remain separate |
| Public downloads | Both archives were downloaded from GitHub Releases and matched the tested checksums | Packages are unsigned |
| Linux OBS integration | Isolated capture, splits, two tracks, delayed full/partial recovery, reconnect and owned shutdown passed; [hosted evidence](evidence/remote-obs-2026-10-26.json) | Synthetic scene; broader encoders and Wayland desktops remain unqualified |
| Marker and split timing | [Measured frame matching](evidence/marker-alignment-2026-10-26.json) on private Linux OBS | Short synthetic sample, not long-session or Windows calibration |
| Disk exhaustion | [Native OBS evidence](evidence/obs-disk-full-2026-10-26.json), plus private media/database failure tests | Interrupted output may be empty; full cross-platform qualification remains |
| UI and public site | Isolated WebKit interaction, paused-frame tab return, large lists, control geometry and desktop/mobile site checks passed | Native GPU/desktop combinations can differ |
| Real footage recovery | Two reported runs recovered from a read-only backup and copied sources using normal APIs | Personal media and logs are not public fixtures |

The [release audit](release-audit.md) lists remaining gates. The [player compatibility page](../site/guide/compatibility.html) is the public support statement. Older validation diaries remain in Git history.

## Reproduce checks

Install the build dependencies in [development](development.md), then run:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo nextest run --workspace --all-features --locked --profile ci
cargo test --workspace --doc --locked
python3 scripts/check-docs.py
python3 -m unittest discover -s scripts -p 'test_*.py'
```

Native tests must use disposable data and configuration. Never point them at a personal library or live OBS profile. On Linux, enter through the private-session wrapper:

```sh
cargo build -p owc-app --locked
python3 scripts/private-session.py python3 scripts/desktop-smoke.py
python3 scripts/private-session.py python3 scripts/browser-smoke.py
python3 scripts/private-session.py python3 scripts/site-smoke.py
```

For the OBS probe, build `cargo build -p owc-app --example obs_probe --locked`, then run `python3 scripts/private-session.py python3 scripts/isolated-obs.py`. It requires OBS, FFmpeg, Xvfb and WebKitWebDriver. Credential, disk-full and reconnect probes are opt-in; consult their script headers for the required isolated environment.

Package and installer checks are defined in [CI](ci.md). Set `OWC_BINARY` to an extracted binary to test that exact package. A successful source build alone is not package validation. Preserve test reports and record the commit, OS/desktop, OBS version, media settings and WoW client build when adding gameplay evidence.

## Documentation, hooks and event-log follow-up — 27 October

The HTML guide and decisions-only cargo-husky hook were checked alongside Clear log in Events. Workspace nextest: 113 passed, seven opt-in skips. Formatting, strict workspace Clippy, doc tests, cargo-deny, actionlint, 15 Python tests and the documentation link/template checks passed. Isolated WebKit verified Clear log and existing player flows; desktop/mobile site checks covered all six guide pages without horizontal overflow or broken local resources. This is local follow-up evidence, not additional platform/gameplay qualification.
