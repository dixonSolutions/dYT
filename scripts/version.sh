#!/usr/bin/env bash
# Every commit on main is a release: v<major>.<minor>.<commits on main>.
set -euo pipefail
cd "$(dirname "$0")/.."
base=$(sed -n 's/^version = "\([0-9]*\.[0-9]*\)\..*/\1/p' Cargo.toml | head -1)
count=$(git rev-list --count HEAD)
echo "DYT_VERSION=$base.$count"
