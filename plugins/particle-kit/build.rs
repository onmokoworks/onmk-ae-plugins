use pipl::*;

fn main() {
    println!("cargo:rustc-cfg=catch_panics");
    println!("cargo:rustc-check-cfg=cfg(catch_panics)");
    pipl::plugin_build(vec![
        Property::Kind(PIPLType::AEEffect),
        Property::Name("Particle Kit"),
        Property::Category("Particle"),
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
            version: 2,
            subversion: 0,
            bugversion: 0,
            stage: Stage::Develop,
            build: 1,
        },
        Property::AE_Effect_Info_Flags(0),
        Property::AE_Effect_Global_OutFlags(
            OutFlags::IExpandBuffer
                | OutFlags::SendUpdateParamsUI
                | OutFlags::NonParamVary
                | OutFlags::SequenceDataNeedsFlattening,
        ),
        Property::AE_Effect_Global_OutFlags_2(
            OutFlags2::ParamGroupStartCollapsedFlag
                | OutFlags2::IUse3DCamera
                | OutFlags2::SupportsSmartRender
                | OutFlags2::SupportsGetFlattenedSequenceData,
        ),
        Property::AE_Effect_Match_Name("ParticleKit"),
        Property::AE_Reserved_Info(12),
        Property::AE_Effect_Support_URL(
            "https://github.com/onmokoworks/onmk-ae-plugins/tree/main/plugins/particle-kit",
        ),
    ]);
}
