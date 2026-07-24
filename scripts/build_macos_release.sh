#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
rust_dir="$repo_root/rust"
plugin_name="MedianPro"
binary_name="median_pro"
profile_dir="$rust_dir/target/release"
bundle_dir="$profile_dir/$plugin_name.plugin"

cd "$rust_dir"
cargo build --release

rm -rf "$bundle_dir"
mkdir -p "$bundle_dir/Contents/Resources"
mkdir -p "$bundle_dir/Contents/MacOS"

cp "$profile_dir/${binary_name}.rsrc" "$bundle_dir/Contents/Resources/${plugin_name}.rsrc"
cp "$profile_dir/${binary_name}_PkgInfo" "$bundle_dir/Contents/PkgInfo"
cp "$profile_dir/${binary_name}_Info.plist" "$bundle_dir/Contents/Info.plist"
cp "$profile_dir/lib${binary_name}.dylib" "$bundle_dir/Contents/MacOS/${plugin_name}"

/usr/libexec/PlistBuddy -c "Set :CFBundleIdentifier work.onmk.MedianPro" "$bundle_dir/Contents/Info.plist"
codesign --force --options runtime --timestamp -s - "$bundle_dir"

echo "Built $bundle_dir"
