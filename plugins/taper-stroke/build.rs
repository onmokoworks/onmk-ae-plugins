use pipl::*;

fn main() {
    println!("cargo:rustc-cfg=catch_panics");
    println!("cargo:rustc-check-cfg=cfg(catch_panics)");
    let properties = vec![
        Property::Kind(PIPLType::AEEffect),
        Property::Name(if cfg!(feature = "coordinate-trace") {
            "TaperStroke Coordinate Trace"
        } else {
            "TaperStroke"
        }),
        Property::Category("ONMK"),
        #[cfg(target_os = "windows")]
        Property::CodeWin64X86("EffectMain"),
        #[cfg(target_os = "macos")]
        Property::CodeMacIntel64("EffectMain"),
        #[cfg(target_os = "macos")]
        Property::CodeMacARM64("EffectMain"),
        Property::AE_PiPL_Version { major: 2, minor: 0 },
        Property::AE_Effect_Spec_Version {
            major: 13,
            minor: 28,
        },
        Property::AE_Effect_Version {
            version: 0,
            subversion: 3,
            bugversion: 1,
            stage: Stage::Develop,
            build: 1,
        },
        Property::AE_Effect_Info_Flags(0),
        Property::AE_Effect_Global_OutFlags(OutFlags::DeepColorAware | OutFlags::NonParamVary),
        Property::AE_Effect_Global_OutFlags_2(
            OutFlags2::SupportsSmartRender
                | OutFlags2::FloatColorAware
                | OutFlags2::DependsOnUnreferencedMasks
                | OutFlags2::RevealsZeroAlpha
                | OutFlags2::SupportsThreadedRendering
                | OutFlags2::SupportsGetFlattenedSequenceData,
        ),
        Property::AE_Effect_Match_Name(if cfg!(feature = "coordinate-trace") {
            "ONMK_TaperStroke_CoordinateTrace"
        } else {
            "ONMK_TaperStroke"
        }),
        Property::AE_Reserved_Info(8),
        Property::AE_Effect_Support_URL(
            "https://github.com/onmokoworks/onmk-ae-plugins/tree/main/plugins/taper-stroke",
        ),
    ];
    #[cfg(not(target_os = "windows"))]
    pipl::plugin_build(properties);
    #[cfg(target_os = "windows")]
    windows_resource(properties);
}

#[cfg(target_os = "windows")]
fn windows_resource(properties: Vec<Property>) {
    // The pinned pipl crate uses RC string literals for binary bytes. On a CP932
    // machine, e.g. 0x88 becomes 0x81,0x45 and corrupts the property offsets.
    // Emit the same macro metadata, but let RC include an opaque binary file.
    for flag in [
        "with_premiere",
        "does_dialog",
        "uses_audio",
        "sends_update_params_ui",
        "gpu_render",
        "smart_render",
        "threaded_rendering",
    ] {
        println!("cargo:rustc-check-cfg=cfg({flag})");
    }
    println!("cargo:rustc-cfg=smart_render");
    println!("cargo:rustc-cfg=threaded_rendering");
    for property in &properties {
        match property {
            Property::Kind(v) => println!(
                "cargo:rustc-env=PIPL_KIND={}",
                u32::from_le_bytes(v.as_bytes())
            ),
            Property::Name(v) => println!("cargo:rustc-env=PIPL_NAME={v}"),
            Property::Category(v) => println!("cargo:rustc-env=PIPL_CATEGORY={v}"),
            Property::CodeWin64X86(v) => println!("cargo:rustc-env=PIPL_ENTRYPOINT={v}"),
            Property::AE_Effect_Match_Name(v) => println!("cargo:rustc-env=PIPL_MATCH_NAME={v}"),
            Property::AE_Effect_Support_URL(v) => println!("cargo:rustc-env=PIPL_SUPPORT_URL={v}"),
            Property::AE_Effect_Spec_Version { major, minor } => {
                println!("cargo:rustc-env=PIPL_AE_SPEC_VER_MAJOR={major}");
                println!("cargo:rustc-env=PIPL_AE_SPEC_VER_MINOR={minor}");
            }
            Property::AE_Effect_Version {
                version,
                subversion,
                bugversion,
                stage,
                build,
            } => println!(
                "cargo:rustc-env=PIPL_VERSION={}",
                pipl::pf_version(*version, *subversion, *bugversion, *stage, *build)
            ),
            Property::AE_Reserved_Info(v) => println!("cargo:rustc-env=PIPL_AE_RESERVED={v}"),
            Property::AE_Effect_Global_OutFlags(v) => {
                println!("cargo:rustc-env=PIPL_OUTFLAGS={}", v.bits())
            }
            Property::AE_Effect_Global_OutFlags_2(v) => {
                println!("cargo:rustc-env=PIPL_OUTFLAGS2={}", v.bits())
            }
            _ => {}
        }
    }
    let file = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("plugin.pipl");
    std::fs::write(&file, pipl::build_pipl(properties).unwrap()).unwrap();
    let mut resource = winres::WindowsResource::new();
    resource.append_rc_content(&format!(
        "16000 PiPL DISCARDABLE \"{}\"",
        file.to_string_lossy().replace('\\', "/")
    ));
    resource.compile().unwrap();
}
