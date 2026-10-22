use pipl::*;

fn main() {
    println!("cargo:rustc-cfg=catch_panics");
    println!("cargo:rustc-check-cfg=cfg(catch_panics)");
    println!("cargo:rerun-if-changed=shader.wgsl");
    pipl::plugin_build(vec![
        Property::Kind(PIPLType::AEEffect),
        Property::Name("onmk Inner Shadow"),
        Property::Category("onmk"),
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
            subversion: 2,
            bugversion: 0,
            stage: Stage::Develop,
            build: 1,
        },
        Property::AE_Effect_Info_Flags(0),
        // Neighbourhood operation: deliberately NOT PixIndependent.
        Property::AE_Effect_Global_OutFlags(OutFlags::DeepColorAware),
        // wgpu runs inside ordinary SmartRender. AE-native GPU rendering
        // (SupportsGpuRenderF32 / SmartRenderGPU) is not implemented, so it is
        // not advertised.
        Property::AE_Effect_Global_OutFlags_2(
            OutFlags2::FloatColorAware
                | OutFlags2::SupportsSmartRender
                | OutFlags2::SupportsThreadedRendering
                | OutFlags2::SupportsGetFlattenedSequenceData,
        ),
        Property::AE_Effect_Match_Name("ONMK_InnerShadow"),
        Property::AE_Reserved_Info(0),
        Property::AE_Effect_Support_URL(
            "https://github.com/onmokoworks/onmk-ae-plugins/tree/main/plugins/inner-shadow",
        ),
    ]);
}
