use pipl::*;

fn main() {
    pipl::plugin_build(vec![
        Property::Kind(PIPLType::AEEffect),
        Property::Name("onmkFlare"),
        Property::Category("Light Effects"),
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
            build: 4,
        },
        Property::AE_Effect_Info_Flags(0),
        Property::AE_Effect_Global_OutFlags(OutFlags::DeepColorAware),
        // The renderer currently needs full-frame coordinates for optical
        // source extraction and ghost placement. Advertising Smart Render
        // made AE hand us partial-width tiles, which were incorrectly treated
        // as complete frames. Use the reliable full-frame Render command until
        // tile origins are explicitly supported.
        Property::AE_Effect_Global_OutFlags_2(OutFlags2::empty()),
        Property::AE_Effect_Match_Name("onmkFlare"),
        Property::AE_Reserved_Info(12),
        Property::AE_Effect_Support_URL("https://github.com"),
    ]);
}
