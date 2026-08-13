#!/usr/bin/env bash
# Copy model packs from the repo into MediaCore/DepthONNX/models/.
# Re-run when manifests or ONNX files change. Requires sudo.
#
# Usage:
#   ./scripts/setup_models.sh              # all packs under model/*/
#   ./scripts/setup_models.sh depth_anything_v2_small

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE_ROOT="${REPO_ROOT}/model"
TARGET_ROOT="/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/DepthONNX/models"

if [[ ! -d "${SOURCE_ROOT}" ]]; then
  echo "error: ${SOURCE_ROOT} not found" >&2
  exit 1
fi

packs=()
if [[ $# -gt 0 ]]; then
  packs=("$@")
else
  for pack in "${SOURCE_ROOT}"/*/; do
    [[ -f "${pack}manifest.json" ]] || continue
    packs+=("$(basename "${pack}")")
  done
fi

if [[ ${#packs[@]} -eq 0 ]]; then
  echo "error: no model packs found under ${SOURCE_ROOT}" >&2
  exit 1
fi

echo "→ target: ${TARGET_ROOT}"
sudo mkdir -p "${TARGET_ROOT}"

for name in "${packs[@]}"; do
  src="${SOURCE_ROOT}/${name}"
  dst="${TARGET_ROOT}/${name}"
  if [[ ! -f "${src}/manifest.json" ]]; then
    echo "error: missing ${src}/manifest.json" >&2
    exit 1
  fi

  echo "→ syncing ${name}"
  sudo mkdir -p "${dst}"
  sudo cp "${src}/manifest.json" "${dst}/"

  missing=0
  while IFS= read -r file; do
    [[ -n "${file}" ]] || continue
    if [[ ! -f "${src}/${file}" ]]; then
      echo "  warn: ${file} not found in repo (export or copy it first)" >&2
      missing=1
      continue
    fi
    echo "  copy ${file}"
    sudo cp "${src}/${file}" "${dst}/"
  done < <(python3 - <<PY
import json, sys
from pathlib import Path
manifest = json.loads(Path("${src}/manifest.json").read_text())
for name in manifest.get("variants", {}).values():
    print(name)
PY
)

  if [[ "${missing}" -ne 0 ]]; then
    echo "  note: pack ${name} is incomplete until all variant ONNX files exist" >&2
  fi
done

echo
echo "Done. Model packs:"
sudo ls -la "${TARGET_ROOT}"
echo
echo "Restart After Effects to refresh the Model popup."
