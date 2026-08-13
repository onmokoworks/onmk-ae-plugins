use pipl::*;

fn main() {
    // The `after-effects` crate wraps its `EffectMain` entry point in `catch_unwind`,
    // but only under `#[cfg(any(debug_assertions, catch_panics))]`. Without this flag
    // release builds have no guard around setup/teardown commands, and a panic
    // crossing the `extern "C"` boundary aborts After Effects instead of just failing
    // the effect. The macro expands into this crate, so the cfg belongs here rather
    // than in RUSTFLAGS (which would push it onto every dependency).
    //
    // This only works with `panic = "unwind"` (the default). Do not set
    // `panic = "abort"`.
    println!("cargo:rustc-cfg=catch_panics");
    println!("cargo:rustc-check-cfg=cfg(catch_panics)");

    pipl::plugin_build(vec![
        Property::Kind(PIPLType::AEEffect),
        Property::Name("PrismWarp"),
        Property::Category("Distort"),
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
            version: 1,
            subversion: 0,
            bugversion: 0,
            stage: Stage::Develop,
            build: 1,
        },
        Property::AE_Effect_Info_Flags(0),
        // DeepColorAware (16bpc) and FloatColorAware (32bpc) are backed by the
        // depth-aware conversion in `layer_to_image` / `write_row_to_layer`.
        // SupportsGpuRenderF32 is deliberately absent: there is no GPU kernel, and a
        // GPU world's pixels are not reachable through the CPU `buffer()` path.
        Property::AE_Effect_Global_OutFlags(OutFlags::DeepColorAware),
        Property::AE_Effect_Global_OutFlags_2(
            OutFlags2::FloatColorAware
                | OutFlags2::SupportsSmartRender
                | OutFlags2::SupportsThreadedRendering
                | OutFlags2::SupportsGetFlattenedSequenceData,
        ),
        Property::AE_Effect_Match_Name("ONMK_PrismWarp"),
        Property::AE_Reserved_Info(13),
        Property::AE_Effect_Support_URL(
            "https://github.com/onmokoworks/onmk-ae-plugins/tree/main/plugins/prism-warp",
        ),
    ]);
}
