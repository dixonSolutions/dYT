# Third-party components shipped with dYT

| Component | Where | License |
|-----------|-------|---------|
| [FFmpeg](https://ffmpeg.org) 9.0.2, built by `packaging/ffmpeg/build.sh` (remux only, no GPL parts) | The Flatpak (`/app/bin/ffmpeg`) and the APK (`libffmpeg.so`) | LGPL-2.1-or-later. Source: the release tarball named in the script; build flags in the script. |
| [yt-dlp/ejs](https://github.com/yt-dlp/ejs) 0.8.0 challenge solver | `crates/dyt-engine/js/` | Unlicense; bundles [meriyah](https://github.com/meriyah/meriyah) (ISC) and [astring](https://github.com/davidbonnet/astring) (MIT) |
| [QuickJS-ng](https://github.com/quickjs-ng/quickjs) via `rquickjs` | The engine | MIT |
| Rust crates and npm packages | Everywhere | See `Cargo.lock` and `pnpm-lock.yaml`; all permissive (MIT, Apache-2.0, BSD, ISC, MPL-2.0 for webpki roots) |
