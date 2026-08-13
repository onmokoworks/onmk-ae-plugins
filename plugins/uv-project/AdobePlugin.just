set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

TargetDir := env_var_or_default("CARGO_TARGET_DIR", "target")
export AESDK_ROOT := if env("AESDK_ROOT", "") == "" { justfile_directory() / "../../sdk/AfterEffectsSDK" } else { env_var("AESDK_ROOT") }
export PRSDK_ROOT := if env("PRSDK_ROOT", "") == "" { justfile_directory() / "../../sdk/Premiere Pro 22.0 C++ SDK" } else { env_var("PRSDK_ROOT") }

[windows]
build:
    cargo build
    if (-not $env:NO_INSTALL) { \
        Start-Process PowerShell -Verb runAs -ArgumentList "-command Set-Location '{{source_directory()}}'; Copy-Item -Force '{{TargetDir}}\debug\{{BinaryName}}.dll' 'C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\UVProject.aex'" \
    }

[windows]
release:
    cargo build --release
    Copy-Item -Force '{{TargetDir}}\release\{{BinaryName}}.dll' '{{TargetDir}}\release\UVProject.aex'
    if (-not $env:NO_INSTALL) { \
        Start-Process PowerShell -Verb runAs -ArgumentList "-command Set-Location '{{source_directory()}}'; Copy-Item -Force '{{TargetDir}}\release\{{BinaryName}}.dll' 'C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\UVProject.aex'" \
    }

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
    echo "Creating plugin bundle"
    rm -Rf {{TargetDir}}/{{profile}}/UVProject.plugin
    mkdir -p {{TargetDir}}/{{profile}}/UVProject.plugin/Contents/Resources
    mkdir -p {{TargetDir}}/{{profile}}/UVProject.plugin/Contents/MacOS

    if [ "{{profile}}" == "release" ]; then
        # Build universal binary (Intel + Apple Silicon)
        rustup target add aarch64-apple-darwin
        rustup target add x86_64-apple-darwin

        cargo build --release --target x86_64-apple-darwin
        cargo build --release --target aarch64-apple-darwin

        cp {{TargetDir}}/x86_64-apple-darwin/release/{{BinaryName}}.rsrc {{TargetDir}}/{{profile}}/UVProject.plugin/Contents/Resources/UVProject.rsrc
        cp {{TargetDir}}/x86_64-apple-darwin/release/{{BinaryName}}_PkgInfo {{TargetDir}}/{{profile}}/UVProject.plugin/Contents/PkgInfo
        cp {{TargetDir}}/x86_64-apple-darwin/release/{{BinaryName}}_Info.plist {{TargetDir}}/{{profile}}/UVProject.plugin/Contents/Info.plist
        lipo {{TargetDir}}/{x86_64,aarch64}-apple-darwin/release/lib{{BinaryName}}.dylib -create -output {{TargetDir}}/{{profile}}/UVProject.plugin/Contents/MacOS/UVProject.dylib
        mv {{TargetDir}}/{{profile}}/UVProject.plugin/Contents/MacOS/UVProject.dylib {{TargetDir}}/{{profile}}/UVProject.plugin/Contents/MacOS/UVProject
    else
        cp {{TargetDir}}/{{profile}}/{{BinaryName}}.rsrc {{TargetDir}}/{{profile}}/UVProject.plugin/Contents/Resources/UVProject.rsrc
        cp {{TargetDir}}/{{profile}}/{{BinaryName}}_PkgInfo {{TargetDir}}/{{profile}}/UVProject.plugin/Contents/PkgInfo
        cp {{TargetDir}}/{{profile}}/{{BinaryName}}_Info.plist {{TargetDir}}/{{profile}}/UVProject.plugin/Contents/Info.plist
        cp {{TargetDir}}/{{profile}}/lib{{BinaryName}}.dylib {{TargetDir}}/{{profile}}/UVProject.plugin/Contents/MacOS/UVProject
    fi
    /usr/libexec/PlistBuddy -c 'Set :CFBundleIdentifier "{{BundleIdentifier}}"' {{TargetDir}}/{{profile}}/UVProject.plugin/Contents/Info.plist

    DEV_CERT=$(security find-identity -v -p codesigning | grep -m 1 "Apple Development" | awk -F ' ' '{print $2}')

    # Perform signing - AE/PR 25.2+ require signed plugins on macOS
    if [ -n "$DEV_CERT" ]; then
       codesign --options runtime --timestamp -strict --sign "$DEV_CERT" {{TargetDir}}/{{profile}}/UVProject.plugin
    else
       codesign --options runtime --timestamp -strict --sign - {{TargetDir}}/{{profile}}/UVProject.plugin
       echo "Note: Using ad-hoc signature. For distribution, a valid Apple Developer certificate is recommended."
    fi

    # Install
    if [ -z "$NO_INSTALL" ]; then
        echo "Installing to /Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/"
        sudo cp -rf "{{TargetDir}}/{{profile}}/UVProject.plugin" "/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/"
    fi
