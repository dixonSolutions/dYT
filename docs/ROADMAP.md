# Known gaps and next steps

What dYT doesn't do yet, as of 2026-11-02, roughly in order of how much it would matter.

## Gaps

- **Downloads don't survive an app restart.** Within a run a download rides out dropped
  connections, stalls and expired links (each chunk retries for ~2.5 minutes), but quitting or
  a crash discards the partial files. Resuming would need a small state file in the work
  directory (`.dyt-<id>-…`) listing finished byte ranges per stream.
- **Android: no foreground service.** A long download keeps running while dYT is in the
  background only as long as Android lets the process live. A foreground service with a
  progress notification would make it dependable.
- **Age-restricted videos that can't be embedded** need a signed-in account; dYT has no
  sign-in. Embeddable ones work (through the embedded client).
- **Live streams** are refused while live; they download once the recording is published.
- **The Flatpak is x86_64 only.** An aarch64 build needs an arm runner in `release.yml`.
- **The ffmpeg source checksum isn't pinned yet.** `packaging/ffmpeg/build.sh` prints it;
  add it to `packaging/ffmpeg/SHA256SUMS` from a CI log.
- **Headless end-to-end runs can't check preview playback.** On a headless Weston the video
  element never advances, so `tests/e2e` reports that step as a warning. Playback was checked
  on Android (video and audio within 0.15 s) and loads on WebKitGTK.

## Ideas

- Playlists and channels: queue every video with the same options.
- A per-download speed graph, and a global speed limit for metered connections.
- Subtitles (the player response lists caption tracks) as `.srt` beside the video.
- Chapter markers from the description as clip presets.
- Precise cuts that re-encode only the first GOP when the start isn't on a keyframe, for
  WebM output (MP4 output already starts exactly, through its edit list).
