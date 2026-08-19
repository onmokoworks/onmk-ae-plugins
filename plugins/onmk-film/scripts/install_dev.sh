#!/bin/bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
: "${AESDK_ROOT:?Set AESDK_ROOT to the AfterEffectsSDK directory}"
cd "$ROOT"
NO_INSTALL=1 just build
DEST="$HOME/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore"
mkdir -p "$DEST"
BUNDLE="target/debug/onmkFilm.plugin"
if [[ ! -d "$BUNDLE" ]]; then
  echo "Missing $BUNDLE" >&2
  exit 1
fi
ditto "$BUNDLE" "$DEST/onmkFilm.plugin"
echo "Installed onmkFilm.plugin to $DEST"
