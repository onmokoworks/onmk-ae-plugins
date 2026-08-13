use pipl::*;

const PF_PLUG_IN_VERSION: u16 = 13;
const PF_PLUG_IN_SUBVERS: u16 = 28;

#[rustfmt::skip]
fn main() {
    let manifest_dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let ort_dylib = manifest_dir
        .join("../../third_party/onnxruntime-osx-arm64/lib/libonnxruntime.1.24.4.dylib");
    if ort_dylib.exists() {
        println!("cargo:rustc-env=ORT_DYLIB_PATH={}", ort_dylib.display());
    }

    println!("cargo:rustc-cfg=catch_panics");

    let model_root = manifest_dir.join("../../model");
    if model_root.is_dir() {
        println!(
            "cargo:rustc-env=AE_DEPTH_ONNX_DEV_MODEL_DIR={}",
            model_root.display()
        );
    }

    pipl::plugin_build(vec![
        Property::Kind(PIPLType::AEEffect),
        Property::Name("Depth ONNX"),
        Property::Category("Depth"),

        #[cfg(target_os = "windows")]
        Property::CodeWin64X86("EffectMain"),
        #[cfg(target_os = "macos")]
        Property::CodeMacIntel64("EffectMain"),
        #[cfg(target_os = "macos")]
        Property::CodeMacARM64("EffectMain"),

        Property::AE_PiPL_Version { major: 2, minor: 0 },
        Property::AE_Effect_Spec_Version { major: PF_PLUG_IN_VERSION, minor: PF_PLUG_IN_SUBVERS },
        Property::AE_Effect_Version {
            version: 0,
            subversion: 9,
            bugversion: 0,
            stage: Stage::Develop,
            build: 1,
        },
        Property::AE_Effect_Info_Flags(0),
        Property::AE_Effect_Global_OutFlags(
            OutFlags::PixIndependent |
            OutFlags::UseOutputExtent |
            OutFlags::DeepColorAware
        ),
        Property::AE_Effect_Global_OutFlags_2(
            OutFlags2::SupportsSmartRender |
            OutFlags2::FloatColorAware |
            OutFlags2::SupportsThreadedRendering
        ),
        Property::AE_Effect_Match_Name("ANTH DepthONNX"),
        Property::AE_Reserved_Info(0),
        Property::AE_Effect_Support_URL("https://github.com/onmokoworks/depth-onnx-Ae"),
    ]);

    #[cfg(target_os = "macos")]
    write_macos_bundle_metadata();
}

#[cfg(target_os = "macos")]
fn write_macos_bundle_metadata() {
    use std::fs;
    use std::path::PathBuf;

    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".into());
    let pkg_name = std::env::var("CARGO_PKG_NAME").unwrap();
    let mut out = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("target"));
    if let Ok(target) = std::env::var("TARGET") {
        if target != std::env::var("HOST").unwrap_or_default() {
            out.push(target);
        }
    }
    out.push(&profile);

    let _ = fs::create_dir_all(&out);
    let _ = fs::write(out.join(format!("{pkg_name}_PkgInfo")), b"eFKTFXTC");

    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>DepthONNX</string>
    <key>CFBundleIdentifier</key>
    <string>com.onmk.ae.depthonnx</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>DepthONNX</string>
    <key>CFBundlePackageType</key>
    <string>eFKT</string>
    <key>CFBundleShortVersionString</key>
    <string>0.9</string>
    <key>CFBundleSignature</key>
    <string>FXTC</string>
    <key>CFBundleSupportedPlatforms</key>
    <array>
        <string>MacOSX</string>
    </array>
    <key>CFBundleVersion</key>
    <string>0.9</string>
    <key>LSMinimumSystemVersion</key>
    <string>13.0</string>
    <key>LSRequiresCarbon</key>
    <true/>
    <key>NSHumanReadableCopyright</key>
    <string>© 2026 onmk. MIT License.</string>
</dict>
</plist>
"#
    );
    let _ = fs::write(out.join(format!("{pkg_name}_Info.plist")), plist);
}
