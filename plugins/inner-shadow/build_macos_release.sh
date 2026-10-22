#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "Run this script on macOS with Xcode Command Line Tools and Rust installed." >&2
  exit 1
fi
plugin_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$plugin_root"
# Native architecture by default; pass aarch64-apple-darwin or x86_64-apple-darwin.
target="${1:-$(rustc -vV | sed -n 's/^host: //p')}"
case "$target" in aarch64-apple-darwin|x86_64-apple-darwin) ;; *) echo "Unsupported target: $target" >&2; exit 1 ;; esac
export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-12.0}"
cargo build --locked --release --target "$target" --target-dir "$plugin_root/target"
profile="$plugin_root/target/$target/release"
bundle="$plugin_root/dist/$target/InnerShadow.plugin"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
# PiPL 0.1.1 uses CARGO_PKG_NAME (hyphen), while the dylib uses underscores.
cp "$profile/inner-shadow.rsrc" "$bundle/Contents/Resources/InnerShadow.rsrc"
cp "$profile/inner-shadow_PkgInfo" "$bundle/Contents/PkgInfo"
cp "$profile/inner-shadow_Info.plist" "$bundle/Contents/Info.plist"
cp "$profile/libinner_shadow.dylib" "$bundle/Contents/MacOS/InnerShadow"
plist="$bundle/Contents/Info.plist"
/usr/libexec/PlistBuddy -c 'Add :CFBundleExecutable string InnerShadow' "$plist"
/usr/libexec/PlistBuddy -c 'Add :CFBundleName string InnerShadow' "$plist"
/usr/libexec/PlistBuddy -c 'Set :CFBundleIdentifier work.onmk.inner-shadow' "$plist"
/usr/libexec/PlistBuddy -c 'Add :CFBundleShortVersionString string 0.2.0' "$plist"
/usr/libexec/PlistBuddy -c 'Add :CFBundleVersion string 0.2.0' "$plist"
plutil -lint "$plist"
# Local ad-hoc signature, not Developer ID signing or notarization.
codesign --force --sign - "$bundle"
codesign --verify --strict "$bundle"
file "$bundle/Contents/MacOS/InnerShadow"
shasum -a 256 "$bundle/Contents/MacOS/InnerShadow"
echo "Built $bundle (not installed)"
