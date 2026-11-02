#!/usr/bin/env bash
# Upload the release signing keys (kept outside the repository, in ~/.local/share/dyt-signing)
# to the GitHub repository's Actions secrets. Without them the release workflow still builds and
# tests everything, but publishes nothing.
set -euo pipefail
K=${DYT_SIGNING_DIR:-$HOME/.local/share/dyt-signing}
REPO=${1:-dixonSolutions/dYT}
GNUPGHOME=$K/gnupg gpg --armor --export-secret-keys | gh secret set DYT_GPG_PRIVATE_KEY -R "$REPO"
base64 -w0 "$K/release.keystore" | gh secret set ANDROID_KEYSTORE_B64 -R "$REPO"
gh secret set ANDROID_KEYSTORE_PASSWORD -R "$REPO" < "$K/release.pass"
gh secret set ANDROID_KEY_PASSWORD -R "$REPO" < "$K/release.pass"
printf dyt | gh secret set ANDROID_KEY_ALIAS -R "$REPO"
gh secret list -R "$REPO"
