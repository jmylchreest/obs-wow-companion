# GLib compatibility backport

`glib/` is the crates.io glib 0.18.5 source archive (see Cargo.lock before the
path override for its registry checksum), with the two-line upstream correction
from https://github.com/gtk-rs/gtk-rs-core/pull/1343 applied to
`src/variant_iter.rs`: make the output pointer mutable and pass `&mut p`.
This fixes RUSTSEC-2024-0429 / GHSA-wrw7-89jp-8q8g.

Tao/Wry still require GTK3 and GLib 0.18. There is no patched compatible registry
release; GLib 0.20 cannot be substituted into that dependency graph. Remove this
patch when the desktop stack permits a fixed upstream version. Preserve upstream
licence files and include this dependency in generated third-party notices.
Cargo registry bookkeeping files and Cargo.toml.orig are omitted; runtime source
changes are limited to the upstream correction. The app's Linux release-mode
regression exercises forward, reverse and indexed string-array iteration.
