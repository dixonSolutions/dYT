#!/usr/bin/env bash
# Write the files that point Flatpak at the repository: dyt.flatpakrepo (`flatpak remote-add`)
# and dyt.flatpakref (one-step install). The GPG key comes from packaging/flatpak/dyt.gpg.
#   packaging/flatpak/remote-files.sh <pages-dir> <base-url>
set -euo pipefail
pages=${1:?pages dir}
base=${2:?base url}
here=$(cd "$(dirname "$0")" && pwd)
key=$(base64 -w0 "$here/dyt.gpg")
cat > "$pages/dyt.flatpakrepo" <<REPO
[Flatpak Repo]
Title=dYT
Url=$base/repo/
Homepage=https://github.com/dixonSolutions/dYT
Comment=Fast, reliable YouTube downloads
Description=dYT, published from github.com/dixonSolutions/dYT
Icon=$base/icon.svg
GPGKey=$key
REPO
cat > "$pages/dyt.flatpakref" <<REF
[Flatpak Ref]
Name=io.github.dixonsolutions.dyt
Branch=stable
Title=dYT
Url=$base/repo/
SuggestRemoteName=dyt
Homepage=https://github.com/dixonSolutions/dYT
Icon=$base/icon.svg
RuntimeRepo=https://dl.flathub.org/repo/flathub.flatpakrepo
IsRuntime=false
GPGKey=$key
REF
