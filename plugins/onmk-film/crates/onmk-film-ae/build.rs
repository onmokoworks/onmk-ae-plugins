use pipl::*;
fn main() {
    println!("cargo:rustc-env=PIPL_SUPPORT_URL=https://github.com/onmokoworks/onmk-ae-plugins/tree/main/plugins/onmk-film");
    println!("cargo:rustc-cfg=catch_panics");
    println!("cargo:rustc-check-cfg=cfg(catch_panics)");
    pipl::plugin_build(vec![
        Property::Kind(PIPLType::AEEffect),
        Property::Name("onmk Film"),
        Property::Category("onmk"),
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
            subversion: 5,
            bugversion: 0,
            stage: Stage::Develop,
            build: 1,
        },
        Property::AE_Effect_Info_Flags(0),
        Property::AE_Effect_Global_OutFlags(OutFlags::DeepColorAware),
        Property::AE_Effect_Global_OutFlags_2(
            OutFlags2::SupportsSmartRender
                | OutFlags2::SupportsThreadedRendering
                | OutFlags2::SupportsGetFlattenedSequenceData
                | OutFlags2::FloatColorAware
                | OutFlags2::ParamGroupStartCollapsedFlag,
        ),
        Property::AE_Effect_Match_Name("ONMK_Film"),
        Property::AE_Reserved_Info(8),
        Property::AE_Effect_Support_URL(
            "https://github.com/onmokoworks/onmk-ae-plugins/tree/main/plugins/onmk-film",
        ),
    ]);
}
