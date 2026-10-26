#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "Run this script on macOS with Xcode Command Line Tools and Rust installed." >&2
  exit 1
fi
plugin_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$plugin_root"
# Universal (Apple Silicon + Intel) by default; pass one target to build a single architecture.
if [[ $# -gt 0 ]]; then targets=("$@"); else targets=(aarch64-apple-darwin x86_64-apple-darwin); fi
for target in "${targets[@]}"; do
  case "$target" in aarch64-apple-darwin|x86_64-apple-darwin) ;; *) echo "Unsupported target: $target" >&2; exit 1 ;; esac
done
export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-12.0}"
target_dir="${CARGO_TARGET_DIR:-$plugin_root/target}"

# bundle <cargo package> <crate output name> <bundle name> <bundle id> <Cargo.toml>
bundle() {
  local package="$1" output="$2" name="$3" identifier="$4" manifest="$5"
  local dylibs=() target
  for target in "${targets[@]}"; do
    cargo build --locked --release -p "$package" --target "$target" --target-dir "$target_dir"
    dylibs+=("$target_dir/$target/release/lib$output.dylib")
  done
  local profile="$target_dir/${targets[0]}/release"
  local bundle="$plugin_root/dist/$name.plugin"
  rm -rf "$bundle"
  mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
  cp "$profile/$output.rsrc" "$bundle/Contents/Resources/$name.rsrc"
  cp "$profile/${output}_PkgInfo" "$bundle/Contents/PkgInfo"
  cp "$profile/${output}_Info.plist" "$bundle/Contents/Info.plist"
  lipo -create "${dylibs[@]}" -output "$bundle/Contents/MacOS/$name"
  local version plist="$bundle/Contents/Info.plist"
  version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$manifest" | head -1)"
  /usr/libexec/PlistBuddy -c "Add :CFBundleExecutable string $name" "$plist"
  /usr/libexec/PlistBuddy -c "Add :CFBundleName string $name" "$plist"
  /usr/libexec/PlistBuddy -c "Set :CFBundleIdentifier $identifier" "$plist"
  /usr/libexec/PlistBuddy -c "Add :CFBundleShortVersionString string $version" "$plist"
  /usr/libexec/PlistBuddy -c "Add :CFBundleVersion string $version" "$plist"
  plutil -lint "$plist"
  # Local ad-hoc signature, not Developer ID signing or notarization.
  codesign --force --sign - "$bundle"
  codesign --verify --strict "$bundle"
  lipo -archs "$bundle/Contents/MacOS/$name"
  shasum -a 256 "$bundle/Contents/MacOS/$name"
  echo "Built $bundle (not installed)"
}

bundle particlekit particlekit ParticleKit work.onmk.particle-kit Cargo.toml
bundle lattice_lab_plugin lattice_lab_plugin LatticeLab work.onmk.lattice-lab crates/lattice_lab_plugin/Cargo.toml
