#!/usr/bin/env bash
set -euo pipefail

plugin_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
plugin_name="EdgeSmith"
binary_name="edge_smith"
resource_name="edge-smith"
profile_dir="$plugin_root/target/release"
bundle_dir="$profile_dir/$plugin_name.plugin"

cd "$plugin_root"
cargo build --release

rm -rf "$bundle_dir"
mkdir -p "$bundle_dir/Contents/Resources" "$bundle_dir/Contents/MacOS"
cp "$profile_dir/${resource_name}.rsrc" "$bundle_dir/Contents/Resources/${plugin_name}.rsrc"
cp "$profile_dir/${resource_name}_PkgInfo" "$bundle_dir/Contents/PkgInfo"
cp "$profile_dir/${resource_name}_Info.plist" "$bundle_dir/Contents/Info.plist"
cp "$profile_dir/lib${binary_name}.dylib" "$bundle_dir/Contents/MacOS/$plugin_name"
/usr/libexec/PlistBuddy -c "Set :CFBundleIdentifier work.onmk.edge-smith" "$bundle_dir/Contents/Info.plist"
codesign --force --options runtime --timestamp -s - "$bundle_dir"

echo "Built $bundle_dir"
