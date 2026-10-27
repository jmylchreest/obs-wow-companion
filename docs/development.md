# Development

Use Rust 1.95 or newer (edition 2024), Python 3 and Git. OBS, FFmpeg and ffprobe are separate runtime tools. The [player guide](../site/guide/quick-start.html) covers using a built app.

On Ubuntu 24.04:

```sh
sudo apt install build-essential pkg-config libgtk-3-dev libwebkit2gtk-4.1-dev libdbus-1-dev ffmpeg
cargo run --locked --bin obs-wow-companion
```

On Windows, use the MSVC Rust toolchain, Visual Studio C++ Build Tools and WebView2 Runtime. Keep `ffmpeg` and `ffprobe` on PATH or select their paths in Settings → Advanced.

Useful launch options:

- `--browser`: open the interface in a browser.
- `--no-open`: run the service and print its private session URL.
- `--data-dir /absolute/path`: use a separate library/configuration directory.

Treat the session URL as a credential. Do not put it in reports or screenshots.

## Git hooks and aide decisions

The application crate uses [cargo-husky](https://github.com/rhysd/cargo-husky) with custom user hooks. Building tests (`cargo test -p owc-app --no-run`) installs `.cargo-husky/hooks/pre-commit`. Git for Windows supplies the shell; the hook uses Python 3 on both platforms. Existing hooks installed by another tool are not replaced. If a custom `core.hooksPath` or existing hook prevents installation, explicitly add `python scripts/shared-context.py export` to that hook.

Before a commit, the hook runs `aide share export --decisions`, checks the exported files for developer-specific absolute paths, and stages only `.aide/shared`. It follows aide's own decisions-export workflow. Memories, local databases, credentials and the rest of `.aide` remain ignored. Without an aide binary or local store, export is skipped and any existing shared context is still checked. Export/check failures block the commit.

The exporter prefers `.aide/bin/aide` (or `aide.exe`), then PATH. Run it manually with `python3 scripts/shared-context.py export`; validate committed files with `python3 scripts/shared-context.py check`. Fix a rejected decision at its source and export again. Review new shared decisions before pushing. CI runs the shared-context check too.

Use `aide share import --decisions` to bring committed project decisions into a local aide store. There is no automatic memory export or post-merge import. `CARGO_HUSKY_DONT_INSTALL_HOOKS=true` disables hook installation in an environment which must not install hooks.

## Checks and documentation

See [validation](validation.md) for test commands and [CI](ci.md) for package/release gates. Native probes must use disposable directories and the private-session wrapper. Never run them against a personal library or live OBS profile.

Player guides are authored directly as HTML under `site/guide/`, with shared CSS and no build step. Keep their navigation consistent. Run `python3 scripts/check-docs.py` after editing docs, then the isolated site smoke check for HTML/CSS changes. Update the changelog for user-visible changes. Historical evidence belongs in the archive; current instructions belong in the guide or focused contributor references.

## Profiling and benchmarks

Normal builds do not enable timing traces. For an investigation, build with symbols
and the optional profiler, then use a separate test library:

```sh
cargo build -p owc-app --profile profiling --features profiling --locked
mkdir -p target/profiles
target/profiling/obs-wow-companion --no-open --data-dir /tmp/owc-profile-data --profile-trace target/profiles/session.json
```

Quit through the app so the trace is flushed. Open the resulting JSON in a local
Chrome trace viewer or Perfetto. Spans cover snapshots, event batches, commands,
ticks, database reads/writes and media operations. They omit arguments and log
events. Trace files still reveal timings and local activity patterns; inspect them
before sharing. The `profiling` Cargo profile also retains symbols for a sampling
profiler. Async span timings show work on each poll, not the full elapsed time of
an external FFmpeg job; compare those with the app's media progress events.

For repeatable measurements, use the synthetic fixtures rather than a personal
library or live OBS profile:

```sh
cargo bench -p owc-wow-log --bench ingestion --locked -- --save-baseline before
cargo bench -p owc-library --bench library --locked -- --save-baseline before
# After a change, replace --save-baseline before with --baseline before.
```

Criterion reports live under `target/criterion`; CI uploads these with test reports.
Compare runs on the same idle machine. On 27 October 2026, the 1,000-record
checkpoint fixture improved from 6.44 ms to 1.12 ms by preparing its SQL once per
batch. Library reads remained around 2.44 ms. These are synthetic measurements,
not a claim that every user interaction is 83% faster.

## Dependency maintenance

Dependabot groups Rust workspace updates and checks Actions and Python requirements.
Security alerts and security-update PRs are enabled. The weekly **Maintenance audit**
also checks pinned Cargo tools, actionlint and the maintained Video.js v8 channel.
Run it locally with `python3 scripts/maintenance-audit.py`; it reports updates without
installing them. Video.js v10 requires a separate player migration; see the
[upstream migration guide](https://videojs.org/docs/framework/html/guides/migrate-from-video-js-8).

After changing Cargo dependencies, run `python3 scripts/third-party-notices.py` and
commit the regenerated notices with the lockfile. CI keeps the licence check strict
and uploads generated notices for reviewing Dependabot changes. Missing upstream
licences need an exact source commit and checksum in `licenses/upstream/manifest.json`.
Do not disable the check or accept licence exceptions just to merge a version bump.
