#!/usr/bin/env bash
# Run the end-to-end test against the real app, invisibly: a headless Weston, a throwaway
# data dir, a private D-Bus session (so an open dYT doesn't swallow the test's launch as a
# second instance), and tauri-driver + WebKitWebDriver. Needs a built app
# (pnpm tauri build --debug).
set -euo pipefail
if [ -z "${DYT_E2E_INNER:-}" ]; then
	exec env DYT_E2E_INNER=1 dbus-run-session -- "$0" "$@"
fi
cd "$(dirname "$0")/../.."
OUT=${OUT:-$PWD/.scratch/e2e}
rm -rf "${OUT:?}" && mkdir -p "$OUT/xdg-data" "$OUT/xdg-runtime"
chmod 700 "$OUT/xdg-runtime"

# Everything below, the app included, inherits this environment: only the headless
# compositor is reachable, never the desktop's Wayland or X11 display.
export XDG_DATA_HOME="$OUT/xdg-data" XDG_RUNTIME_DIR="$OUT/xdg-runtime"
export WAYLAND_DISPLAY=dyt-e2e GDK_BACKEND=wayland
unset DISPLAY
# ffmpeg/ffprobe from PATH, or Homebrew's when the toolbox has none. The app is started by
# WebKitWebDriver, which doesn't pass PATH on, so ffmpeg goes beside it (where the Flatpak
# keeps it too).
command -v ffmpeg >/dev/null || export PATH="$PATH:/home/linuxbrew/.linuxbrew/bin"
APP=${APP:-$PWD/target/debug/dyt}
[ -e "$(dirname "$APP")/ffmpeg" ] || ln -s "$(command -v ffmpeg)" "$(dirname "$APP")/ffmpeg"
# No sound device in the private session: a silent sink that still keeps time.
export GST_PLUGIN_FEATURE_RANK="fakeaudiosink:MAX,pulsesink:NONE,pipewiresink:NONE,alsasink:NONE"

weston --backend=headless --renderer=${RENDERER:-pixman} --debug --shell=kiosk --width=1000 --height=780 \
	--socket=dyt-e2e --idle-time=0 >"$OUT/weston.log" 2>&1 &
WESTON=$!
sleep 1.5
tauri-driver --port 4444 >"$OUT/driver.log" 2>&1 &
DRIVER=$!
trap 'kill $DRIVER $WESTON 2>/dev/null || true' EXIT
sleep 1
OUT="$OUT" timeout 1200 node tests/e2e/app.e2e.mjs
