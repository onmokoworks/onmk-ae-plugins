TargetDir := env_var_or_default("CARGO_TARGET_DIR", "target")
export AESDK_ROOT := if env("AESDK_ROOT", "") == "" { justfile_directory() / "../../sdk/AfterEffectsSDK" } else { env_var("AESDK_ROOT") }
export PRSDK_ROOT := if env("PRSDK_ROOT", "") == "" { justfile_directory() / "../../sdk/Premiere Pro 22.0 C++ SDK" } else { env_var("PRSDK_ROOT") }

[windows]
build:
    cargo build

[windows]
release:
    cargo build --release
    Copy-Item -Force '{{TargetDir}}\release\{{BinaryName}}.dll' '{{TargetDir}}\release\{{PluginName}}.aex'

[macos]
build:
    cargo build
    just -f {{justfile()}} create_bundle debug {{TargetDir}}

[macos]
release:
    cargo build --release
    just -f {{justfile()}} create_bundle release {{TargetDir}}

[macos]
create_bundle profile TargetDir:
    #!/bin/bash
    set -e
    bundle="{{TargetDir}}/{{profile}}/{{PluginName}}.plugin"
    rm -Rf "$bundle"
    mkdir -p "$bundle/Contents/Resources" "$bundle/Contents/MacOS"
    cp "{{TargetDir}}/{{profile}}/{{ResourceName}}.rsrc" "$bundle/Contents/Resources/{{PluginName}}.rsrc"
    cp "{{TargetDir}}/{{profile}}/{{ResourceName}}_PkgInfo" "$bundle/Contents/PkgInfo"
    cp "{{TargetDir}}/{{profile}}/{{ResourceName}}_Info.plist" "$bundle/Contents/Info.plist"
    cp "{{TargetDir}}/{{profile}}/lib{{BinaryName}}.dylib" "$bundle/Contents/MacOS/{{PluginName}}"
    /usr/libexec/PlistBuddy -c 'Set :CFBundleIdentifier "{{BundleIdentifier}}"' "$bundle/Contents/Info.plist"
    codesign --force --sign - "$bundle"
