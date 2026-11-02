#!/usr/bin/env bash
# The GitHub Pages landing page beside the Flatpak repository.
#   scripts/site.sh <pages-dir> <version> <owner/repo>
set -euo pipefail
pages=${1:?pages dir}
version=${2:?version}
repo=${3:?owner/repo}
root=$(cd "$(dirname "$0")/.." && pwd)
cp "$root/assets/icon.svg" "$pages/icon.svg"
# The client list installed apps pick up at startup (see update_clients in src-tauri).
cp "$root/packaging/clients.json" "$pages/clients.json"
sed -e "s|@VERSION@|$version|g" -e "s|@REPO@|$repo|g" "$root/site/index.html" > "$pages/index.html"
