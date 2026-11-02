#!/usr/bin/env bash
# Release notes: how to install, and the commits since the previous release.
#   scripts/release-notes.sh <version>
set -euo pipefail
version=${1:?version}
prev=$(git describe --tags --abbrev=0 HEAD^ 2>/dev/null || true)
cat <<NOTES
## Install

**Linux (Flatpak)**, updates arrive with \`flatpak update\`:

\`\`\`sh
flatpak install --user https://dixonsolutions.github.io/dYT/dyt.flatpakref
\`\`\`

or add the remote once: \`flatpak remote-add --user --if-not-exists dyt https://dixonsolutions.github.io/dYT/dyt.flatpakrepo\`.
The \`.flatpak\` file below installs offline: \`flatpak install --user dYT-$version-x86_64.flatpak\`.

**Android**: install \`dYT-$version.apk\` below (arm64, armv7 and x86_64 in one APK).

## Changes
NOTES
if [ -n "$prev" ]; then
	git log --no-merges --format='- %s' "$prev..HEAD"
else
	git log --no-merges --format='- %s' HEAD
fi | grep -v '\[skip ci\]' | head -60
