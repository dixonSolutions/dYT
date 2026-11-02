<p align="center"><img src="assets/icon.svg" width="96" alt=""></p>

<h1 align="center">dYT</h1>

<p align="center">Fast, reliable YouTube downloads for Linux and Android.</p>

dYT downloads videos, audio or exact clips from YouTube with a native Rust engine and a
Svelte interface (shadcn-svelte), packaged with Tauri as a Flatpak and an Android app.

- **Full speed.** Each download is split across many connections at once, with idle
  connections taking over the slowest remaining work, so it runs as fast as the line allows.
- **Reliable.** Dropped connections resume from the last byte; expired links are refreshed
  in the middle of a download; a stalled connection is replaced after a few seconds.
- **Exact clips.** Pick a start and an end (mark the start, play, stop to mark the end, or
  drag both points). dYT reads the stream's own index and downloads only the segments that
  cover the clip, then cuts it on the exact timestamp. A 20-second clip of a 4K film is a few MB.
- **Any mix.** Video and audio, video only or audio only, at the quality you choose, or the
  raw streams exactly as YouTube serves them.
- **History.** Everything you open is kept; open it again with one tap.
- **Native look.** Follows the system's light or dark style as it changes, in red.

## Install

**Linux (Flatpak).** Updates come with `flatpak update`:

```sh
flatpak install --user https://dixonsolutions.github.io/dYT/dyt.flatpakref
```

**Android.** Install the APK from the [latest release](https://github.com/dixonSolutions/dYT/releases/latest).
Share any YouTube link to dYT, or open it with dYT.

Every push to `main` is a release (`.github/workflows/release.yml`).

## How it works

```
src/                    Svelte UI (shadcn-svelte, Tailwind)
src-tauri/              Tauri app: commands, history, settings, system theme
  gen/android/          Android project (share target, media store, system bars)
crates/dyt-engine/      The engine, plain Rust, also usable as the `dyt-cli` CLI
packaging/              Flatpak manifest, minimal ffmpeg build
tests/                  UI screenshots (mock backend) and end-to-end tests of the real app
```

The engine asks YouTube's InnerTube API for the streams, trying clients in order: one that
returns plain stream URLs first, then clients whose URLs need YouTube's JavaScript challenges
solved (made-for-kids and embeddable age-restricted videos), which the engine solves itself in
an embedded QuickJS. Streams are fetched with `&range=` requests by a pool of HTTP/1.1
connections and written in place. Video and audio are merged, and clips cut, by a small
remux-only ffmpeg, without re-encoding.

## Develop

```sh
pnpm install
pnpm tauri dev                     # the app
pnpm dev                           # the UI alone, in a browser, with a mock backend
cargo run -p dyt-engine --bin dyt-cli -- get <link> --clip 1:00-1:30   # the engine alone
```

Tests:

```sh
cargo test --workspace                                           # unit and fault-injection tests
DYT_LIVE=1 cargo test -p dyt-engine --test live                  # real downloads from YouTube
pnpm check && pnpm test                                          # the UI
node tests/ui/shots.mjs                                          # UI screenshots (with pnpm dev)
pnpm tauri build --debug --no-bundle && tests/e2e/run.sh         # the real app, end to end
```

## License

MIT or Apache-2.0, at your option. Bundled third-party parts are listed in
[THIRD_PARTY.md](THIRD_PARTY.md).

Download only what you have the right to download.
