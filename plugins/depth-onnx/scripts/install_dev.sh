#!/usr/bin/env bash
# Stage the freshly-built Rust bundle in /tmp (no TCC blocks), then copy into the
# system AE/MediaCore plug-in dir as root. AE 2026 doesn't reliably follow
# symlinks for plug-ins, so we always copy.
#
# Re-run after every rebuild. Requires sudo (target dir is root-owned).

set -euo pipefail

PLUGIN_NAME="DepthONNX"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUILT_BUNDLE="${REPO_ROOT}/target/release/${PLUGIN_NAME}.plugin"
STAGED="/tmp/${PLUGIN_NAME}.plugin"
INSTALL_DIR="/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore"
INSTALL_PATH="${INSTALL_DIR}/${PLUGIN_NAME}.plugin"
USER_MODELS_DIR="${INSTALL_DIR}/${PLUGIN_NAME}/models"
LEGACY_PATH="${INSTALL_DIR}/DepthAnythingV2.plugin"
MANIFEST_TEMPLATE="${REPO_ROOT}/model/depth_anything_v2_small/manifest.json"

if [[ ! -d "${BUILT_BUNDLE}" ]]; then
    echo "error: build first — ${BUILT_BUNDLE} not found" >&2
    echo "       export AESDK_ROOT=/path/to/AfterEffectsSDK" >&2
    echo "       export CARGO_TARGET_DIR=${REPO_ROOT}/target" >&2
    echo "       cd ${REPO_ROOT}/crates/depth-onnx-ae && just release-bundle" >&2
    exit 1
fi

echo "→ staging to ${STAGED} (so root can read it past TCC)"
rm -rf "${STAGED}"
ditto --norsrc --noextattr --noacl --noqtn "${BUILT_BUNDLE}" "${STAGED}"
xattr -cr "${STAGED}"

echo "→ installing to ${INSTALL_PATH} (sudo, will prompt)"
sudo rm -rf "${INSTALL_PATH}"
if [[ -d "${LEGACY_PATH}" ]]; then
    echo "→ removing legacy ${LEGACY_PATH}"
    sudo rm -rf "${LEGACY_PATH}"
fi
sudo ditto --norsrc --noextattr --noacl --noqtn "${STAGED}" "${INSTALL_PATH}"
sudo xattr -cr "${INSTALL_PATH}"

echo "→ ensuring user model directory ${USER_MODELS_DIR}"
sudo mkdir -p "${USER_MODELS_DIR}"
if [[ -f "${MANIFEST_TEMPLATE}" ]]; then
  sudo mkdir -p "${USER_MODELS_DIR}/depth_anything_v2_small"
  sudo cp "${MANIFEST_TEMPLATE}" "${USER_MODELS_DIR}/depth_anything_v2_small/manifest.json"
fi

echo
echo "Installed. Quit AE if it's running, then relaunch and look under"
echo "Effects > Depth > Depth ONNX."
echo
echo "Model packs (not bundled): run"
echo "  ./scripts/setup_models.sh"
echo "or copy ONNX files into"
echo "  ${USER_MODELS_DIR}/<pack>/"
