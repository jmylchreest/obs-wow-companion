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
