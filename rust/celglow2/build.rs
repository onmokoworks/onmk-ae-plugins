use pipl::*;

fn main() {
    // `define_effect!` embeds this at compile time. Keep it local so the
    // plugin can still be built without a release website configured.
    println!("cargo:rustc-env=PIPL_SUPPORT_URL=https://github.com/anthonyhuey/CelGlow");
    let grouped_ui = std::env::var_os("CARGO_FEATURE_GROUPED_UI").is_some();
    let display_name = if grouped_ui { "CelGlow v3" } else { "CelGlow" };
    let match_name = if grouped_ui { "ANTH CelGlow v3" } else { "ANTH CelGlow v2" };
    let out_flags_2 = OutFlags2::SupportsSmartRender
        | OutFlags2::FloatColorAware
        | OutFlags2::SupportsGpuRenderF32
        | OutFlags2::SupportsThreadedRendering
        | OutFlags2::SupportsGetFlattenedSequenceData
        | if grouped_ui { OutFlags2::ParamGroupStartCollapsedFlag } else { OutFlags2::None };
    pipl::plugin_build(vec![
        Property::Kind(PIPLType::AEEffect),
        Property::Name(display_name),
        Property::Category("onmk"),
        Property::CodeWin64X86("EffectMain"),
        Property::AE_PiPL_Version { major: 2, minor: 0 },
        Property::AE_Effect_Spec_Version { major: 13, minor: 28 },
        Property::AE_Effect_Version { version: 0, subversion: 3, bugversion: 0, stage: Stage::Develop, build: 1 },
        Property::AE_Effect_Global_OutFlags(OutFlags::DeepColorAware | OutFlags::PixIndependent | OutFlags::UseOutputExtent),
        Property::AE_Effect_Global_OutFlags_2(out_flags_2),
        Property::AE_Effect_Match_Name(match_name),
        Property::AE_Reserved_Info(0),
    ]);
}
