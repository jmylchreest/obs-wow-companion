# Preparing a release

The repository has a working development build. A successful package build alone does not satisfy the [validation gates](validation.md).

## Local checks

Install cargo-nextest 0.9.149, cargo-deny 0.20.2 and cargo-dist 0.32.0 using `cargo install … --version … --locked`. Run formatting, strict Clippy, nextest, doc tests, `cargo deny check` and `python3 scripts/check-docs.py`. Run the isolated OBS and desktop checks documented in validation.

Build artifacts with `dist build --artifacts=local --target x86_64-unknown-linux-gnu` on Linux and the equivalent `x86_64-pc-windows-msvc` target on Windows. Artifacts include documentation, OBS templates and SHA-256 checksums. OBS and FFmpeg are separate installations. Linux also needs GTK3 and WebKitGTK 4.1 runtime libraries; Windows needs WebView2 Runtime.

For an extracted Linux archive, set `OWC_BINARY` to the absolute path of its `obs-wow-companion` binary when running `scripts/desktop-smoke.py`, `scripts/browser-smoke.py` and `scripts/recovery-smoke.py`. These probes use private data/config directories.

On Ubuntu 24.04, build dependencies are `libgtk-3-dev libwebkit2gtk-4.1-dev libdbus-1-dev pkg-config ffmpeg`. The package currently targets x86-64. Other architectures and macOS are not advertised.

## Versions and publication

Use [release-plz](https://release-plz.dev/docs/config) to prepare version/changelog changes in a clean committed checkout. Its configuration disables registry publication, tag creation and GitHub release creation; cargo-dist owns binary distribution. Review the root CHANGELOG and keep workspace versions together.

Before tagging, finish the validation matrix, verify native imports preserve existing OBS profiles, test both platform artifacts, and review third-party license notices. The explicit release workflow requires successful tag validation, quality checks and a complete verified package bundle. CI has not yet been run remotely in this repository.

Pushing a `v` tag matching the workspace version invokes the release workflow, which uses cargo-dist artifacts and publishes only after all prerequisites succeed. Preparing files locally does not push a tag or publish a release. No release has been published by this implementation task.

Workflows are maintained explicitly; do not overwrite them with `dist generate`. The narrow `allow-dirty = ["ci"]` setting permits custom orchestration while cargo-dist still builds packages/installers. It does not relax tests or checksums. Update immutable action pins in the workflow/composite files and keep matching cargo-dist metadata pins consistent. See [CI operation](ci.md) for triggers, artifacts and activation.

## Dependency policy

The lockfile fixes the dependency graph. cargo-deny checks the actual Linux/Windows targets, rejects unknown registries and licenses outside the allowlist, and checks advisories. The single maintenance exception documents `proc-macro-error` through GTK3 build macros; revisit it when updating Tao/Wry. Duplicated transitive major versions remain warnings. Dependencies for excluded platforms are not part of the advertised support policy.
