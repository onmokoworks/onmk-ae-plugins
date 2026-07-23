use pipl::*;

fn main() {
    // `define_effect!` embeds this at compile time. Keep it local so the
    // plugin can still be built without a release website configured.
    println!("cargo:rustc-env=PIPL_SUPPORT_URL=https://github.com/anthonyhuey/CelGlow");
    pipl::plugin_build(vec![
        Property::Kind(PIPLType::AEEffect),
        Property::Name("CelGlow"),
        Property::Category("onmk"),
        Property::CodeWin64X86("EffectMain"),
        Property::AE_PiPL_Version { major: 2, minor: 0 },
        Property::AE_Effect_Spec_Version { major: 13, minor: 28 },
        Property::AE_Effect_Version { version: 0, subversion: 3, bugversion: 0, stage: Stage::Develop, build: 1 },
        Property::AE_Effect_Global_OutFlags(OutFlags::DeepColorAware | OutFlags::PixIndependent | OutFlags::UseOutputExtent),
        Property::AE_Effect_Global_OutFlags_2(OutFlags2::SupportsSmartRender | OutFlags2::FloatColorAware | OutFlags2::SupportsGpuRenderF32 | OutFlags2::SupportsThreadedRendering | OutFlags2::SupportsGetFlattenedSequenceData),
        Property::AE_Effect_Match_Name("ANTH CelGlow v2"),
        Property::AE_Reserved_Info(0),
    ]);
}
