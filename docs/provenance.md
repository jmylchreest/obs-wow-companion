# Implementation provenance

The Rust source, UI and synthetic log fixtures were independently authored for this repository. Warcraft Recorder, NoOBS and wow-recorder-py were inspected as behavioural references; their code and assets were not copied into the implementation.

The feature comparison uses Warcraft Recorder revision `2643938980b9fe2031aa17275d3ee528bd2fd20e`. References and scope are in the [specification](specification.md). Differing upstream licence declarations are a reason to preserve independent implementation rather than assume that all upstream files share one licence.

OBS starter configuration derives from native OBS 32.2.2 serialisation in an isolated test installation. It contains an empty scene, portable output settings and no physical devices, recording paths or authentication material. This is configuration data, not redistributed OBS implementation code.

The repository's own code is MIT-licenced. Rust dependencies retain their individual licences, checked by cargo-deny. System WebKitGTK/GTK, WebView2, OBS and FFmpeg are separate dependencies with their own distribution terms. Include third-party licence notices when preparing binary distributions; the repository licence does not replace those notices.

## Video.js playback UI

The offline UI includes the official Video.js classic 8.24.1 HTML5 core distribution, pinned and verified against the npm package SHA-512 integrity value recorded in `crates/app/ui/vendor/README.md`. Apache-2.0 notice text is included in generated third-party notices. Its embedded WOFF is extracted and served locally to keep the existing Content Security Policy; streaming plugins and network/CDN assets are not included. The signpost overlay is companion code attached to Video.js's seek-bar element.
