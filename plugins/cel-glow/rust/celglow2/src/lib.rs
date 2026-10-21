//! CelGlow v2 AE front-end. The parameter stream is deliberately independent
//! from the frozen v1 `rust/celglow` crate.

use after_effects as ae;
use std::panic::{catch_unwind, AssertUnwindSafe};

mod gpu;

#[derive(Clone, Copy, Debug)]
struct CelGlowPlane {
    input_plane: ae::Rect,
}

// SmartFX checkout IDs are not parameter ordinals. Adobe requires each ID to
// be positive and unique within a pre-render/render pair. Parameter index 0 is
// still the effect input; ID 1 is how we retrieve that checkout in SmartRender.
const INPUT_CHECKOUT_ID: u32 = 1;

/// Stable AE parameter ordinals.
///
/// The first parameter slot exposed by the PF host is 1 (slot 0 is the input
/// layer), while this enum remains zero-based.  Do not insert a variant in the
/// middle of this list: AE serialises effect parameters by ordinal and an
/// insertion would reinterpret every value in existing projects.  New
/// parameters must be appended at the end and added after the existing calls
/// in `params_setup`.
#[repr(u16)]
#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    SourceChannel = 0,
    SourceGain = 1,
    SourceGamma = 2,
    SourceSpread = 3,
    SourceFieldGamma = 4,
    RingCount = 5,
    RingDistribution = 6,
    RingLineWidth = 7,
    RingHardness = 8,
    RingCoreLevel = 9,
    RingCoreSoftness = 10,
    RingOuterFalloff = 11,
    RingBrightness = 12,
    RingScramble = 13,
    RingSeed = 14,
    RingPhase = 15,
    RingShowOutermost = 16,
    WobbleAmount = 17,
    WobbleScale = 18,
    WobbleComplexity = 19,
    WobbleEvolution = 20,
    TextureEdgeType = 21,
    TextureBorder = 22,
    TextureInfluence = 23,
    TextureScale = 24,
    TextureSharpness = 25,
    TextureComplexity = 26,
    TextureEvolution = 27,
    UnevenAmount = 28,
    UnevenScale = 29,
    UnevenEvolution = 30,
    ColorMode = 31,
    FillColor = 32,
    InnerColor = 33,
    OuterColor = 34,
    RainbowCycles = 35,
    RainbowSaturation = 36,
    Opacity = 37,
    BlendMode = 38,
    Placement = 39,
    GlowAlpha = 40,
    Quality = 41,
    SourceThreshold = 42,
    SourceThresholdSoftness = 43,
    View = 44,

    // UI-only topic identifiers used by the opt-in CelGlow v3 build. They
    // are appended so the default CelGlow v2 stream remains unchanged.
    #[cfg(feature = "grouped-ui")]
    GroupQuickStart = 45,
    #[cfg(feature = "grouped-ui")]
    GroupQuickEnd = 46,
    #[cfg(feature = "grouped-ui")]
    GroupSourceStart = 47,
    #[cfg(feature = "grouped-ui")]
    GroupSourceEnd = 48,
    #[cfg(feature = "grouped-ui")]
    GroupRingsStart = 49,
    #[cfg(feature = "grouped-ui")]
    GroupRingsEnd = 50,
    #[cfg(feature = "grouped-ui")]
    GroupWobbleStart = 51,
    #[cfg(feature = "grouped-ui")]
    GroupWobbleEnd = 52,
    #[cfg(feature = "grouped-ui")]
    GroupRoughenStart = 53,
    #[cfg(feature = "grouped-ui")]
    GroupRoughenEnd = 54,
    #[cfg(feature = "grouped-ui")]
    GroupHazeStart = 55,
    #[cfg(feature = "grouped-ui")]
    GroupHazeEnd = 56,
    #[cfg(feature = "grouped-ui")]
    GroupColorStart = 57,
    #[cfg(feature = "grouped-ui")]
    GroupColorEnd = 58,
    #[cfg(feature = "grouped-ui")]
    GroupDiagnosticsStart = 59,
    #[cfg(feature = "grouped-ui")]
    GroupDiagnosticsEnd = 60,
}

#[cfg(test)]
mod parameter_order_tests {
    use super::Params;

    #[test]
    fn ordinals_are_contiguous_and_view_is_last_current_slot() {
        // Keep this list explicit: adding a parameter in the middle should be
        // an intentional compatibility decision, not an accidental enum edit.
        let ordinals = [
            Params::SourceChannel,
            Params::SourceGain,
            Params::SourceGamma,
            Params::SourceSpread,
            Params::SourceFieldGamma,
            Params::RingCount,
            Params::RingDistribution,
            Params::RingLineWidth,
            Params::RingHardness,
            Params::RingCoreLevel,
            Params::RingCoreSoftness,
            Params::RingOuterFalloff,
            Params::RingBrightness,
            Params::RingScramble,
            Params::RingSeed,
            Params::RingPhase,
            Params::RingShowOutermost,
            Params::WobbleAmount,
            Params::WobbleScale,
            Params::WobbleComplexity,
            Params::WobbleEvolution,
            Params::TextureEdgeType,
            Params::TextureBorder,
            Params::TextureInfluence,
            Params::TextureScale,
            Params::TextureSharpness,
            Params::TextureComplexity,
            Params::TextureEvolution,
            Params::UnevenAmount,
            Params::UnevenScale,
            Params::UnevenEvolution,
            Params::ColorMode,
            Params::FillColor,
            Params::InnerColor,
            Params::OuterColor,
            Params::RainbowCycles,
            Params::RainbowSaturation,
            Params::Opacity,
            Params::BlendMode,
            Params::Placement,
            Params::GlowAlpha,
            Params::Quality,
            Params::SourceThreshold,
            Params::SourceThresholdSoftness,
            Params::View,
        ];
        assert_eq!(ordinals.len(), 45);
        for (expected, ordinal) in ordinals.iter().enumerate() {
            assert_eq!(*ordinal as usize, expected);
        }
    }
}

#[derive(Default)]
struct Plugin;

ae::define_effect!(Plugin, (), Params);

macro_rules! slider {
    ($params:expr, $id:ident, $label:expr, $min:expr, $max:expr, $default:expr, $precision:expr) => {
        $params.add(Params::$id, $label, ae::FloatSliderDef::setup(|f| {
            f.set_valid_min($min); f.set_valid_max($max); f.set_slider_min($min); f.set_slider_max($max);
            f.set_default($default); f.set_precision($precision);
        }))?;
    };
}
macro_rules! popup {
    ($params:expr, $id:ident, $label:expr, $options:expr, $default:expr) => {
        $params.add(Params::$id, $label, ae::PopupDef::setup(|f| { f.set_options($options); f.set_default($default); }))?;
    };
}

// These parameters were appended after the original effect was released.
// Tell AE how to initialise their slots when loading an older project. This
// is important for View: otherwise the popup can display the new value while
// the render callback still sees an uninitialised/legacy parameter state.
macro_rules! appended_slider {
    ($params:expr, $id:ident, $label:expr, $min:expr, $max:expr, $default:expr, $precision:expr) => {
        $params.add_with_flags(
            Params::$id,
            $label,
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min($min); f.set_valid_max($max);
                f.set_slider_min($min); f.set_slider_max($max);
                f.set_default($default); f.set_precision($precision);
            }),
            ae::ParamFlag::USE_VALUE_FOR_OLD_PROJECTS,
            ae::ParamUIFlags::NONE,
        )?;
    };
}

macro_rules! color {
    ($params:expr, $id:ident, $label:expr, $r:expr, $g:expr, $b:expr) => {
        $params.add(Params::$id, $label, ae::ColorDef::setup(|f| { f.set_default(ae::Pixel8 { red:$r, green:$g, blue:$b, alpha:255 }); }))?;
    };
}

impl AdobePluginGlobal for Plugin {
    #[allow(unreachable_code)]
    fn params_setup(&self, p: &mut ae::Parameters<Params>, _i: ae::InData, _o: ae::OutData) -> Result<(), ae::Error> {
        #[cfg(feature = "grouped-ui")]
        {
            p.add_group(Params::GroupQuickStart, Params::GroupQuickEnd, "Quick Controls", false, |p| {
                popup!(p, SourceChannel, "Source Channel", &["Alpha", "Luma", "Luma + Alpha"], 3);
                slider!(p, SourceSpread, "Source Spread (px)", 0.0, 500.0, 260.0, 1);
                slider!(p, RingCount, "Ring Count", 1.0, 64.0, 12.0, 0);
                slider!(p, RingLineWidth, "Line Width (%)", 5.0, 95.0, 30.0, 1);
                slider!(p, RingCoreLevel, "Core Level (%)", 10.0, 100.0, 70.0, 1);
                slider!(p, Opacity, "Glow Opacity (%)", 0.0, 200.0, 75.0, 1);
                Ok(())
            })?;
            p.add_group(Params::GroupSourceStart, Params::GroupSourceEnd, "Source / Threshold", false, |p| {
                slider!(p, SourceGain, "Source Gain (%)", 0.0, 400.0, 100.0, 1);
                slider!(p, SourceGamma, "Source Gamma", 0.2, 3.0, 1.0, 2);
                slider!(p, SourceFieldGamma, "Field Gamma", 0.2, 3.0, 1.0, 2);
                slider!(p, SourceThreshold, "Source Threshold (%)", 0.0, 100.0, 0.0, 1);
                slider!(p, SourceThresholdSoftness, "Threshold Softness (%)", 0.0, 100.0, 0.0, 1);
                Ok(())
            })?;
            p.add_group(Params::GroupRingsStart, Params::GroupRingsEnd, "Bands / Rings", true, |p| {
                slider!(p, RingDistribution, "Ring Distribution", 0.3, 3.0, 1.0, 2);
                slider!(p, RingHardness, "Line Hardness (%)", 0.0, 100.0, 80.0, 1);
                slider!(p, RingCoreSoftness, "Core Softness (%)", 0.0, 50.0, 10.0, 1);
                slider!(p, RingOuterFalloff, "Outer Falloff", 0.0, 4.0, 0.0, 2);
                slider!(p, RingBrightness, "Brightness Variation (%)", 0.0, 100.0, 0.0, 1);
                slider!(p, RingScramble, "Color Scramble (%)", 0.0, 100.0, 0.0, 1);
                slider!(p, RingSeed, "Random Seed", 0.0, 65535.0, 1.0, 0);
                slider!(p, RingPhase, "Ring Phase (deg)", -360.0, 360.0, 0.0, 1);
                p.add(Params::RingShowOutermost, "Show Outermost", ae::CheckBoxDef::setup(|f| { f.set_default(false); f.set_label("On"); }))?;
                Ok(())
            })?;
            p.add_group(Params::GroupWobbleStart, Params::GroupWobbleEnd, "Turbulent Displace", true, |p| {
                slider!(p, WobbleAmount, "Amount (px)", 0.0, 100.0, 3.0, 1);
                slider!(p, WobbleScale, "Scale (px)", 2.0, 200.0, 24.0, 1);
                slider!(p, WobbleComplexity, "Complexity", 1.0, 4.0, 1.0, 0);
                slider!(p, WobbleEvolution, "Evolution (deg)", 0.0, 360.0, 0.0, 1);
                Ok(())
            })?;
            p.add_group(Params::GroupRoughenStart, Params::GroupRoughenEnd, "Roughen Edges", true, |p| {
                popup!(p, TextureEdgeType, "Edge Type", &["Cut", "Roughen"], 1);
                slider!(p, TextureBorder, "Border (px)", 0.0, 50.0, 4.0, 1);
                slider!(p, TextureInfluence, "Fractal Influence (px)", 0.0, 50.0, 3.5, 1);
                slider!(p, TextureScale, "Scale (px)", 2.0, 200.0, 3.5, 1);
                slider!(p, TextureSharpness, "Sharpness", 0.0, 20.0, 20.0, 1);
                slider!(p, TextureComplexity, "Complexity", 1.0, 4.0, 3.0, 0);
                slider!(p, TextureEvolution, "Evolution (deg)", 0.0, 360.0, 0.0, 1);
                Ok(())
            })?;
            p.add_group(Params::GroupHazeStart, Params::GroupHazeEnd, "Fractal Haze", true, |p| {
                slider!(p, UnevenAmount, "Amount (%)", 0.0, 100.0, 28.0, 1);
                slider!(p, UnevenScale, "Scale (px)", 50.0, 20000.0, 430.0, 1);
                slider!(p, UnevenEvolution, "Evolution (deg)", 0.0, 360.0, 0.0, 1);
                Ok(())
            })?;
            p.add_group(Params::GroupColorStart, Params::GroupColorEnd, "Color / Composite", true, |p| {
                popup!(p, ColorMode, "Color Mode", &["Fill", "Inner - Outer", "Rainbow", "Source Color"], 1);
                color!(p, FillColor, "Fill Color", 51, 92, 158);
                color!(p, InnerColor, "Inner Color", 255, 255, 255);
                color!(p, OuterColor, "Outer Color", 128, 204, 255);
                slider!(p, RainbowCycles, "Rainbow Cycles", 0.25, 8.0, 1.0, 2);
                slider!(p, RainbowSaturation, "Rainbow Saturation (%)", 0.0, 100.0, 70.0, 1);
                popup!(p, BlendMode, "Blend Mode", &["Add", "Screen", "Normal"], 1);
                popup!(p, Placement, "Placement", &["Behind Source", "In Front"], 1);
                p.add(Params::GlowAlpha, "Glow Alpha", ae::CheckBoxDef::setup(|f| { f.set_default(true); f.set_label("On"); }))?;
                popup!(p, Quality, "Quality", &["Draft (GPU)", "Normal", "Best"], 2);
                Ok(())
            })?;
            p.add_group(Params::GroupDiagnosticsStart, Params::GroupDiagnosticsEnd, "Diagnostics", true, |p| {
                p.add_with_flags(
                    Params::View,
                    "View",
                    ae::PopupDef::setup(|f| {
                        f.set_options(&["Result", "Input", "Field", "Radius", "Rings", "Texture Mask"]);
                        f.set_default(1);
                    }),
                    ae::ParamFlag::USE_VALUE_FOR_OLD_PROJECTS,
                    ae::ParamUIFlags::NONE,
                )?;
                Ok(())
            })?;
            return Ok(());
        }
        popup!(p, SourceChannel, "Source Channel", &["Alpha", "Luma", "Luma + Alpha"], 3);
        slider!(p, SourceGain, "Source Gain (%)", 0.0, 400.0, 100.0, 1);
        slider!(p, SourceGamma, "Source Gamma", 0.2, 3.0, 1.0, 2);
        slider!(p, SourceSpread, "Source Spread (px)", 0.0, 500.0, 260.0, 1);
        slider!(p, SourceFieldGamma, "Field Gamma", 0.2, 3.0, 1.0, 2);
        slider!(p, RingCount, "Ring Count", 1.0, 64.0, 12.0, 0);
        slider!(p, RingDistribution, "Ring Distribution", 0.3, 3.0, 1.0, 2);
        slider!(p, RingLineWidth, "Line Width (%)", 5.0, 95.0, 30.0, 1);
        slider!(p, RingHardness, "Line Hardness (%)", 0.0, 100.0, 80.0, 1);
        slider!(p, RingCoreLevel, "Core Level (%)", 10.0, 100.0, 70.0, 1);
        slider!(p, RingCoreSoftness, "Core Softness (%)", 0.0, 50.0, 10.0, 1);
        slider!(p, RingOuterFalloff, "Outer Falloff", 0.0, 4.0, 0.0, 2);
        slider!(p, RingBrightness, "Brightness Variation (%)", 0.0, 100.0, 0.0, 1);
        slider!(p, RingScramble, "Color Scramble (%)", 0.0, 100.0, 0.0, 1);
        slider!(p, RingSeed, "Random Seed", 0.0, 65535.0, 1.0, 0);
        slider!(p, RingPhase, "Ring Phase (deg)", -360.0, 360.0, 0.0, 1);
        p.add(Params::RingShowOutermost, "Show Outermost", ae::CheckBoxDef::setup(|f| { f.set_default(false); f.set_label("On"); }))?;
        slider!(p, WobbleAmount, "Wobble Amount (px)", 0.0, 100.0, 3.0, 1);
        slider!(p, WobbleScale, "Wobble Scale (px)", 2.0, 200.0, 24.0, 1);
        slider!(p, WobbleComplexity, "Wobble Complexity", 1.0, 4.0, 1.0, 0);
        slider!(p, WobbleEvolution, "Wobble Evolution (deg)", 0.0, 360.0, 0.0, 1);
        popup!(p, TextureEdgeType, "Edge Type", &["Cut", "Roughen"], 1);
        slider!(p, TextureBorder, "Roughen Border (px)", 0.0, 50.0, 4.0, 1);
        slider!(p, TextureInfluence, "Fractal Influence (px)", 0.0, 50.0, 3.5, 1);
        slider!(p, TextureScale, "Roughen Scale (px)", 2.0, 200.0, 3.5, 1);
        slider!(p, TextureSharpness, "Edge Sharpness", 0.0, 20.0, 20.0, 1);
        slider!(p, TextureComplexity, "Roughen Complexity", 1.0, 4.0, 3.0, 0);
        slider!(p, TextureEvolution, "Roughen Evolution (deg)", 0.0, 360.0, 0.0, 1);
        slider!(p, UnevenAmount, "Unevenness Amount (%)", 0.0, 100.0, 28.0, 1);
        slider!(p, UnevenScale, "Unevenness Scale (px)", 50.0, 20000.0, 430.0, 1);
        slider!(p, UnevenEvolution, "Unevenness Evolution (deg)", 0.0, 360.0, 0.0, 1);
        popup!(p, ColorMode, "Color Mode", &["Fill", "Inner - Outer", "Rainbow", "Source Color"], 1);
        color!(p, FillColor, "Fill Color", 51, 92, 158);
        color!(p, InnerColor, "Inner Color", 255, 255, 255);
        color!(p, OuterColor, "Outer Color", 128, 204, 255);
        slider!(p, RainbowCycles, "Rainbow Cycles", 0.25, 8.0, 1.0, 2);
        slider!(p, RainbowSaturation, "Rainbow Saturation (%)", 0.0, 100.0, 70.0, 1);
        slider!(p, Opacity, "Glow Opacity (%)", 0.0, 200.0, 75.0, 1);
        popup!(p, BlendMode, "Blend Mode", &["Add", "Screen", "Normal"], 1);
        popup!(p, Placement, "Placement", &["Behind Source", "In Front"], 1);
        p.add(Params::GlowAlpha, "Glow Alpha", ae::CheckBoxDef::setup(|f| { f.set_default(true); f.set_label("On"); }))?;
        popup!(p, Quality, "Quality", &["Draft (GPU)", "Normal", "Best"], 2);
        // Appended to keep existing AE parameter streams stable.
        appended_slider!(p, SourceThreshold, "Source Threshold (%)", 0.0, 100.0, 0.0, 1);
        appended_slider!(p, SourceThresholdSoftness, "Threshold Softness (%)", 0.0, 100.0, 0.0, 1);
        p.add_with_flags(
            Params::View,
            "View",
            ae::PopupDef::setup(|f| {
                f.set_options(&["Result", "Input", "Field", "Radius", "Rings", "Texture Mask"]);
                f.set_default(1);
            }),
            ae::ParamFlag::USE_VALUE_FOR_OLD_PROJECTS,
            ae::ParamUIFlags::NONE,
        )?;
        Ok(())
    }

    fn handle_command(&self, cmd: ae::Command, i: ae::InData, mut o: ae::OutData, p: &mut ae::Parameters<Params>) -> Result<(), ae::Error> {
        match cmd {
            ae::Command::About => {
                o.set_return_msg(if cfg!(feature = "grouped-ui") { "CelGlow v3" } else { "CelGlow v2" });
                Ok(())
            }
            ae::Command::GlobalSetup => {
                o.set_out_flag(ae::OutFlags::DeepColorAware, true);
                o.set_out_flag(ae::OutFlags::PixIndependent, true);
                o.set_out_flag(ae::OutFlags::UseOutputExtent, true);
                o.set_out_flag2(ae::OutFlags2::SupportsSmartRender, true);
                o.set_out_flag2(ae::OutFlags2::FloatColorAware, true);
                o.set_out_flag2(ae::OutFlags2::SupportsGpuRenderF32, true);
                o.set_out_flag2(ae::OutFlags2::SupportsThreadedRendering, true);
                o.set_out_flag2(ae::OutFlags2::SupportsGetFlattenedSequenceData, true);
                #[cfg(feature = "grouped-ui")]
                o.set_out_flag2(ae::OutFlags2::ParamGroupStartCollapsedFlag, true);
                Ok(())
            }
            ae::Command::UserChangedParam { .. } => Ok(()),
            ae::Command::SmartPreRender { mut extra } => {
                let sref = i.width().min(i.height()).max(1) as f64;
                let spread = p.get(Params::SourceSpread)?.as_float_slider()?.value();
                let wobble = p.get(Params::WobbleAmount)?.as_float_slider()?.value();
                let border = p.get(Params::TextureBorder)?.as_float_slider()?.value();
                let influence = p.get(Params::TextureInfluence)?.as_float_slider()?.value();
                let pad = (spread + wobble + border + influence + 16.0)
                    .max(0.0)
                    .min(sref * 4.0)
                    .ceil() as i32;
                let mut request = extra.output_request();
                request.rect = inflate_rect(request.rect.into(), pad).into();
                let result = extra.callbacks().checkout_layer(
                    0,
                    INPUT_CHECKOUT_ID as i32,
                    &request,
                    i.current_time(),
                    i.time_step(),
                    i.time_scale(),
                )?;
                // The wgpu path is available for AE GPU frames. If the host
                // cannot provide a compatible frame, SmartRenderGpu falls
                // back to the exact CPU implementation.
                let gpu_quality = p.get(Params::Quality)?.as_popup()?.value() as i32 == 1
                    && p.get(Params::View)?.as_popup()?.value() as i32 == 1;
                extra.set_gpu_render_possible(gpu_quality);
                extra.set_returns_extra_pixels(true);
                extra.set_result_rect(result.result_rect.into());
                extra.set_max_result_rect(result.max_result_rect.into());
                extra.set_pre_render_data(CelGlowPlane { input_plane: result.result_rect.into() });
                Ok(())
            }
            ae::Command::SmartRender { extra } => {
                smart_render(&extra, &i, p)?;
                Ok(())
            }
            ae::Command::SmartRenderGpu { extra } => {
                smart_render_gpu(&extra, &i, p)?;
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

fn inflate_rect(mut r: ae::Rect, pad: i32) -> ae::Rect {
    r.left -= pad;
    r.top -= pad;
    r.right += pad;
    r.bottom += pad;
    r
}

fn read_core_params(p: &ae::Parameters<Params>) -> Result<celglow_core::Params, ae::Error> {
    use celglow_core::params::{BlendMode, ColorMode, Placement, Quality, RoughenEdgeType, SourceChannel};
    let mut c = celglow_core::Params::default();
    let popup = |id| -> Result<i32, ae::Error> { Ok(p.get(id)?.as_popup()?.value() as i32) };
    let slider = |id| -> Result<f32, ae::Error> { Ok(p.get(id)?.as_float_slider()?.value() as f32) };
    let color = |id| -> Result<[f32; 3], ae::Error> {
        let v = p.get(id)?.as_color()?.float_value()?;
        Ok([v.red as f32, v.green as f32, v.blue as f32])
    };
    c.source.channel = match popup(Params::SourceChannel)? {
        1 => SourceChannel::Alpha,
        2 => SourceChannel::Luma,
        _ => SourceChannel::LumaXAlpha,
    };
    c.source.gain = slider(Params::SourceGain)?;
    c.source.gamma = slider(Params::SourceGamma)?;
    c.source.spread = slider(Params::SourceSpread)?;
    c.source.field_gamma = slider(Params::SourceFieldGamma)?;
    c.source.threshold = slider(Params::SourceThreshold)?;
    c.source.threshold_softness = slider(Params::SourceThresholdSoftness)?;
    c.rings.ring_count = slider(Params::RingCount)?.round().clamp(1.0, 255.0) as u8;
    c.rings.distribution = slider(Params::RingDistribution)?;
    c.rings.line_width = slider(Params::RingLineWidth)?;
    c.rings.line_hardness = slider(Params::RingHardness)?;
    c.rings.core_level = slider(Params::RingCoreLevel)?;
    c.rings.core_softness = slider(Params::RingCoreSoftness)?;
    c.rings.outer_falloff = slider(Params::RingOuterFalloff)?;
    c.rings.brightness_variation = slider(Params::RingBrightness)?;
    c.rings.color_scramble = slider(Params::RingScramble)?;
    c.rings.seed = slider(Params::RingSeed)?.round().clamp(0.0, 65535.0) as u16;
    c.rings.phase = slider(Params::RingPhase)?;
    c.rings.show_outermost = p.get(Params::RingShowOutermost)?.as_checkbox()?.value();
    c.wobble.amount = slider(Params::WobbleAmount)?;
    c.wobble.scale = slider(Params::WobbleScale)?;
    c.wobble.complexity = slider(Params::WobbleComplexity)?.round().clamp(1.0, 255.0) as u8;
    c.wobble.evolution = slider(Params::WobbleEvolution)?;
    c.texture.edge_type = if popup(Params::TextureEdgeType)? == 1 { RoughenEdgeType::Cut } else { RoughenEdgeType::Roughen };
    c.texture.border = slider(Params::TextureBorder)?;
    c.texture.influence = slider(Params::TextureInfluence)?;
    c.texture.scale = slider(Params::TextureScale)?;
    c.texture.sharpness = slider(Params::TextureSharpness)?;
    c.texture.complexity = slider(Params::TextureComplexity)?.round().clamp(1.0, 255.0) as u8;
    c.texture.evolution = slider(Params::TextureEvolution)?;
    c.unevenness.amount = slider(Params::UnevenAmount)?;
    c.unevenness.scale = slider(Params::UnevenScale)?;
    c.unevenness.evolution = slider(Params::UnevenEvolution)?;
    c.output.color_mode = match popup(Params::ColorMode)? {
        2 => ColorMode::InnerOuter,
        3 => ColorMode::Rainbow,
        4 => ColorMode::SourceColor,
        _ => ColorMode::Fill,
    };
    c.output.fill_color = color(Params::FillColor)?;
    c.output.inner_color = color(Params::InnerColor)?;
    c.output.outer_color = color(Params::OuterColor)?;
    c.output.rainbow_cycles = slider(Params::RainbowCycles)?;
    c.output.rainbow_saturation = slider(Params::RainbowSaturation)?;
    c.output.opacity = slider(Params::Opacity)?;
    c.output.blend_mode = match popup(Params::BlendMode)? {
        2 => BlendMode::Screen,
        3 => BlendMode::Normal,
        _ => BlendMode::Add,
    };
    c.output.placement = if popup(Params::Placement)? == 2 { Placement::InFront } else { Placement::BehindSource };
    c.output.write_glow_alpha = p.get(Params::GlowAlpha)?.as_checkbox()?.value();
    c.output.quality = match popup(Params::Quality)? {
        1 => Quality::Draft,
        3 => Quality::Best,
        _ => Quality::Normal,
    };
    Ok(c)
}

fn read_view(p: &ae::Parameters<Params>) -> Result<celglow_core::DebugView, ae::Error> {
    Ok(match p.get(Params::View)?.as_popup()?.value() as i32 {
        2 => celglow_core::DebugView::Source,
        3 => celglow_core::DebugView::Field,
        4 => celglow_core::DebugView::Radius,
        5 => celglow_core::DebugView::Rings,
        6 => celglow_core::DebugView::TextureMask,
        _ => celglow_core::DebugView::Result,
    })
}

fn depth_from_format(format: ae::PixelFormat) -> Result<i16, ae::Error> {
    match format {
        ae::PixelFormat::Argb32 => Ok(8),
        ae::PixelFormat::Argb64 => Ok(16),
        ae::PixelFormat::Argb128 => Ok(32),
        _ => Err(ae::Error::BadCallbackParameter),
    }
}

#[test]
fn cpu_world_formats_are_explicit() {
    assert_eq!(depth_from_format(ae::PixelFormat::Argb32).unwrap(), 8);
    assert_eq!(depth_from_format(ae::PixelFormat::Argb64).unwrap(), 16);
    assert_eq!(depth_from_format(ae::PixelFormat::Argb128).unwrap(), 32);
    assert!(depth_from_format(ae::PixelFormat::GpuBgra128).is_err());
}

fn checked_layout(w:usize,h:usize,row_bytes:isize,bpp:usize)->Result<usize,ae::Error> {
    if w>i32::MAX as usize || h>i32::MAX as usize || row_bytes==i32::MIN as isize {
        return Err(ae::Error::BadCallbackParameter);
    }
    let stride=row_bytes.unsigned_abs();
    let row=w.checked_mul(bpp).ok_or(ae::Error::OutOfMemory)?;
    let length=stride.checked_mul(h).ok_or(ae::Error::OutOfMemory)?;
    if w!=0 && h!=0 && (stride<row || length>isize::MAX as usize) {
        return Err(ae::Error::BadCallbackParameter);
    }
    Ok(stride)
}

fn checked_world_layout(layer:&ae::Layer,depth:i16)->Result<usize,ae::Error> {
    let stride=checked_layout(layer.width(),layer.height(),layer.row_bytes(),depth as usize/2)?;
    if layer.width()!=0 && layer.height()!=0 && unsafe{layer.data_ptr()}.is_null() {
        return Err(ae::Error::BadCallbackParameter);
    }
    Ok(stride)
}

fn physical_row(y:usize,h:usize,bottom_up:bool)->usize {
    if bottom_up {h-1-y} else {y}
}

#[test]
fn world_layout_rejects_short_rows_and_maps_negative_stride() {
    for bpp in [4,8,16] {
        let stride=17*bpp+7;
        assert_eq!(checked_layout(17,31,stride as isize,bpp),Ok(stride));
        assert_eq!(checked_layout(17,31,-(stride as isize),bpp),Ok(stride));
        assert!(checked_layout(17,31,(17*bpp-1) as isize,bpp).is_err());
    }
    assert!(checked_layout(1,1,i32::MIN as isize,4).is_err());
    assert!(checked_layout(i32::MAX as usize+1,1,4,4).is_err());
    assert!(checked_layout(1,2,isize::MAX,4).is_err());
    assert_eq!((0..3).map(|y|physical_row(y,3,true)).collect::<Vec<_>>(),[2,1,0]);
    assert_eq!((0..3).map(|y|physical_row(y,3,false)).collect::<Vec<_>>(),[0,1,2]);
}

fn layer_to_rgba_f32(layer: &ae::Layer) -> Result<(Vec<f32>, usize, usize), ae::Error> {
    let w = layer.width() as usize;
    let h = layer.height() as usize;
    if w == 0 || h == 0 { return Ok((Vec::new(), 0, 0)); }
    let depth = depth_from_format(layer.pixel_format()?)?;
    let stride = checked_world_layout(layer, depth)?;
    let bottom_up = layer.row_bytes() < 0;
    let buf = layer.buffer();
    let count=w.checked_mul(h).and_then(|n|n.checked_mul(4)).ok_or(ae::Error::OutOfMemory)?;
    let mut out = Vec::new();
    out.try_reserve_exact(count).map_err(|_|ae::Error::OutOfMemory)?;
    out.resize(count,0.0f32);
    let (bytes_per_pixel, scale) = match depth { 32 => (16, 1.0), 16 => (8, 1.0 / 32768.0), _ => (4, 1.0 / 255.0) };
    for y in 0..h {
        for x in 0..w {
            let si = physical_row(y,h,bottom_up) * stride + x * bytes_per_pixel;
            let di = (y * w + x) * 4;
            if si + bytes_per_pixel <= buf.len() {
                // PF_Pixel is laid out as alpha, red, green, blue. The core
                // uses the conventional RGBA order, so swap explicitly here.
                let read = |ae_ch: usize| -> f32 {
                    match depth {
                        32 => f32::from_ne_bytes([
                            buf[si + ae_ch * 4], buf[si + ae_ch * 4 + 1],
                            buf[si + ae_ch * 4 + 2], buf[si + ae_ch * 4 + 3],
                        ]),
                        16 => u16::from_ne_bytes([buf[si + ae_ch * 2], buf[si + ae_ch * 2 + 1]]) as f32 * scale,
                        _ => buf[si + ae_ch] as f32 * scale,
                    }
                };
                let alpha = read(0).clamp(0.0, 1.0);
                // Preserve RGB for zero-alpha samples if the host supplies
                // them. This is useful for adjustment-layer composites and
                // safe for ordinary premultiplied input, where an empty pixel
                // normally has zero RGB as well.
                let unpremultiply = if alpha > 1.0e-6 { 1.0 / alpha } else { 1.0 };
                out[di] = read(1) * unpremultiply;
                out[di + 1] = read(2) * unpremultiply;
                out[di + 2] = read(3) * unpremultiply;
                out[di + 3] = alpha;
            }
        }
    }
    Ok((out, w, h))
}

fn sanitize_output(values: &mut [f32], depth: i16) {
    for value in values {
        *value = if !value.is_finite() { 0.0 }
            else if depth == 32 { *value }
            else { value.clamp(0.0, 1.0) };
    }
}

fn premultiplied_output(rgb: f32, alpha: f32, depth: i16) -> f32 {
    let value = rgb * alpha;
    if depth == 32 { value } else { value.clamp(0.0, 1.0) }
}

#[test]
fn output_boundary_preserves_float_hdr_and_sanitizes_nonfinite() {
    let mut rgba = [8.0, -2.0, f32::NAN, 0.5];
    sanitize_output(&mut rgba, 32);
    assert_eq!(rgba, [8.0, -2.0, 0.0, 0.5]);
    assert_eq!(premultiplied_output(rgba[0], rgba[3], 32), 4.0);
    assert_eq!(premultiplied_output(rgba[1], rgba[3], 32), -1.0);
    for depth in [8, 16] {
        let mut legacy = [8.0, -2.0, f32::INFINITY, 0.5];
        sanitize_output(&mut legacy, depth);
        assert_eq!(legacy, [1.0, 0.0, 0.0, 0.5]);
        assert_eq!(premultiplied_output(legacy[0], legacy[3], depth), 0.5);
    }
}

fn write_rgba_f32_to_layer(flat: &[f32], layer: &mut ae::Layer, w: usize, h: usize) -> Result<(), ae::Error> {
    let depth = depth_from_format(layer.pixel_format()?)?;
    if (w,h)!=(layer.width(),layer.height())
        || Some(flat.len())!=w.checked_mul(h).and_then(|n|n.checked_mul(4)) {
        return Err(ae::Error::BadCallbackParameter);
    }
    if w==0 || h==0 {return Ok(());}
    let stride = checked_world_layout(layer, depth)?;
    let bottom_up = layer.row_bytes() < 0;
    let buf = layer.buffer_mut();
    for y in 0..h {
        for x in 0..w {
            let si = (y * w + x) * 4;
            if si + 3 >= flat.len() { continue; }
            let (bytes_per_pixel, scale) = match depth { 32 => (16, 1.0), 16 => (8, 32768.0), _ => (4, 255.0) };
            let di = physical_row(y,h,bottom_up) * stride + x * bytes_per_pixel;
            if di + bytes_per_pixel > buf.len() { continue; }
            // PF_Pixel is alpha, red, green, blue while `flat` is RGBA.
            let alpha = flat[si + 3].clamp(0.0, 1.0);
            for (ae_ch, src_ch) in [3usize, 0, 1, 2].into_iter().enumerate() {
                // AE PF_Pixel buffers are premultiplied. The core operates on
                // straight RGBA, so multiply RGB by the final alpha here.
                let v = if src_ch == 3 {
                    alpha
                } else {
                    premultiplied_output(flat[si + src_ch], alpha, depth)
                };
                match depth {
                    32 => { let b = v.to_ne_bytes(); buf[di + ae_ch * 4..di + ae_ch * 4 + 4].copy_from_slice(&b); }
                    16 => { let b = (v * scale).round() as u16; buf[di + ae_ch * 2..di + ae_ch * 2 + 2].copy_from_slice(&b.to_ne_bytes()); }
                    _ => { buf[di + ae_ch] = (v * scale).round() as u8; }
                }
            }
        }
    }
    Ok(())
}

// The caller already owns an input checkout. Transfer that responsibility only
// when an output exists; otherwise release it even if output checkout failed.
fn checkout_output_or_release<T, E>(
    checkout: impl FnOnce() -> Result<Option<T>, E>,
    release: impl FnOnce() -> Result<(), E>,
) -> Result<Option<T>, E> {
    match checkout() {
        Ok(Some(output)) => Ok(Some(output)),
        result => {
            let cleanup = release();
            result.and_then(|none| cleanup.map(|()| none))
        }
    }
}

#[test]
fn output_checkout_failure_releases_input_once_and_preserves_primary_error() {
    use std::cell::Cell;
    for output in [Ok(Some(7)), Ok(None), Err("checkout")] {
        for cleanup in [Ok(()), Err("checkin")] {
            let calls = Cell::new(0);
            let result = checkout_output_or_release(|| output, || {
                calls.set(calls.get() + 1);
                cleanup
            });
            if output == Ok(Some(7)) {
                assert_eq!(calls.get(), 0);
                assert_eq!(result, output);
            } else {
                assert_eq!(calls.get(), 1);
                assert_eq!(result, output.and_then(|none| cleanup.map(|()| none)));
            }
        }
    }
}

fn smart_render(extra: &ae::pf::SmartRenderExtra, in_data: &ae::InData, params: &ae::Parameters<Params>) -> Result<(), ae::Error> {
    let cb = extra.callbacks();
    let Some(input_world) = cb.checkout_layer_pixels(INPUT_CHECKOUT_ID)? else { return Ok(()); };
    let Some(mut output_world) = checkout_output_or_release(
        || cb.checkout_output(), || cb.checkin_layer_pixels(INPUT_CHECKOUT_ID),
    )? else { return Ok(()); };
    let (rgba, iw, ih) = match layer_to_rgba_f32(&input_world) {
        Ok(frame) => frame,
        Err(err) => { let _ = cb.checkin_layer_pixels(INPUT_CHECKOUT_ID); return Err(err); }
    };
    let ow = output_world.width() as usize;
    let oh = output_world.height() as usize;
    if iw == 0 || ih == 0 || ow == 0 || oh == 0 { let _ = cb.checkin_layer_pixels(INPUT_CHECKOUT_ID); return Ok(()); }
    let plane = extra.pre_render_data::<CelGlowPlane>().map(|v| v.input_plane).unwrap_or(ae::Rect { left: 0, top: 0, right: iw as i32, bottom: ih as i32 });
    let output_origin = (in_data.output_origin().h, in_data.output_origin().v);
    // The diagnostic view is still a real core render.  The old adapter used
    // to short-circuit here and copy the input world for every non-Result
    // view.  That made View appear to work only when the host happened to
    // show a cached result, and was especially confusing on adjustment layers
    // whose checked-out input is not the same as the visible composite.
    // Read the same parameter snapshot for every view so Field/Radius/Rings/
    // Texture Mask use the exact source and distance-field path as Result.
    // Keep a default fallback for old serialized instances while their
    // appended controls are being initialized by AE.
    let core = read_core_params(params).unwrap_or_default();
    let mut out = match output_plane(ow, oh) {
        Ok(out) => out,
        Err(err) => { let _ = cb.checkin_layer_pixels(INPUT_CHECKOUT_ID); return Err(err); }
    };
    let view = read_view(params).unwrap_or(celglow_core::DebugView::Result);
    let rendered = core_render_outcome(catch_unwind(AssertUnwindSafe(|| celglow_core::render(
        celglow_core::FrameBuf { w: iw, h: ih, rgba: &rgba },
        (plane.left, plane.top),
        output_origin,
        (ow, oh),
        &core,
        view,
        &mut out,
    ))));
    let rendered = match rendered {
        Ok(rendered) => rendered,
        Err(err) => { let _ = cb.checkin_layer_pixels(INPUT_CHECKOUT_ID); return Err(err); }
    };
    if !rendered {
        // ROI can be clipped by AE in edge cases; preserve the source instead
        // of leaving an uninitialised output surface.
        if let Err(err) = copy_plane_region(&rgba, (iw,ih), (plane.left,plane.top),
            &mut out, (ow,oh), output_origin) {
            let _ = cb.checkin_layer_pixels(INPUT_CHECKOUT_ID); return Err(err);
        }
    }
    // AEXCompat (and AE's float compositor) reject unwritten/non-finite
    // samples. Keep the native render contract closed even when a malformed
    // source pixel or an extreme parameter produces NaN/Inf in the core.
    let depth = match output_world.pixel_format().and_then(depth_from_format) {
        Ok(depth) => depth,
        Err(err) => { let _ = cb.checkin_layer_pixels(INPUT_CHECKOUT_ID); return Err(err); }
    };
    sanitize_output(&mut out, depth);
    let result = write_rgba_f32_to_layer(&out, &mut output_world, ow, oh);
    result.and(cb.checkin_layer_pixels(INPUT_CHECKOUT_ID))
}

fn copy_plane_region(src: &[f32], input: (usize,usize), input_origin: (i32,i32),
    dst: &mut [f32], output: (usize,usize), output_origin: (i32,i32)) -> Result<(),ae::Error> {
    let (iw,ih) = input;
    let (ow,oh) = output;
    for ((w,h),len) in [(input,src.len()),(output,dst.len())] {
        if w > i32::MAX as usize || h > i32::MAX as usize
            || w.checked_mul(h).and_then(|n|n.checked_mul(4)) != Some(len) {
            return Err(ae::Error::BadCallbackParameter);
        }
    }
    let dx=i64::from(output_origin.0)-i64::from(input_origin.0);
    let dy=i64::from(output_origin.1)-i64::from(input_origin.1);
    dst.fill(0.0);
    let left=0i64.max(-dx);
    let right=(ow as i64).min(iw as i64-dx);
    let top=0i64.max(-dy);
    let bottom=(oh as i64).min(ih as i64-dy);
    if left >= right || top >= bottom { return Ok(()); }
    let count=(right-left) as usize*4;
    for y in top..bottom {
        let si=((y+dy) as usize*iw+(left+dx) as usize)*4;
        let di=(y as usize*ow+left as usize)*4;
        dst[di..di+count].copy_from_slice(&src[si..si+count]);
    }
    Ok(())
}

#[test]
fn plane_copy_uses_global_intersection() {
    let src:Vec<f32>=(1..=6).flat_map(|v|[v as f32;4]).collect();
    let mut dst=[99.0;24];
    copy_plane_region(&src,(3,2),(10,-4),&mut dst,(3,2),(11,-3)).unwrap();
    assert_eq!(dst,[5.,5.,5.,5.,6.,6.,6.,6.,0.,0.,0.,0.,0.,0.,0.,0.,0.,0.,0.,0.,0.,0.,0.,0.]);
}

#[test]
fn plane_copy_extreme_origins_and_invalid_buffers() {
    for (a,b) in [(i32::MIN,i32::MAX),(i32::MAX,i32::MIN)] {
        let mut dst=[99.;4];
        copy_plane_region(&[2.;4],(1,1),(a,a),&mut dst,(1,1),(b,b)).unwrap();
        assert_eq!(dst,[0.;4]);
        copy_plane_region(&[2.;4],(1,1),(a,a),&mut dst,(1,1),(a,a)).unwrap();
        assert_eq!(dst,[2.;4]);
    }
    let mut dst=[99.;4];
    for (size,src) in [((1,1),&[1.;3][..]),((usize::MAX,2),&[][..])] {
        assert_eq!(copy_plane_region(src,size,(0,0),&mut dst,(1,1),(0,0)),Err(ae::Error::BadCallbackParameter));
        assert_eq!(dst,[99.;4]);
    }
    let mut expanded=[99.;36];
    copy_plane_region(&[2.;4],(1,1),(-1,-1),&mut expanded,(3,3),(-2,-2)).unwrap();
    for (i,p) in expanded.chunks_exact(4).enumerate() { assert_eq!(p,if i==4 { &[2.;4] } else { &[0.;4] }); }
}

fn output_plane(w: usize, h: usize) -> Result<Vec<f32>, ae::Error> {
    let count = w.checked_mul(h).and_then(|n| n.checked_mul(4))
        .ok_or(ae::Error::BadCallbackParameter)?;
    count.checked_mul(std::mem::size_of::<f32>())
        .filter(|&n| n <= isize::MAX as usize)
        .ok_or(ae::Error::BadCallbackParameter)?;
    let mut out = Vec::new();
    out.try_reserve_exact(count).map_err(|_| ae::Error::OutOfMemory)?;
    out.resize(count, 0.0);
    Ok(out)
}

#[test]
fn output_plane_checks_geometry_before_allocation() {
    for (w,h) in [(usize::MAX,2),(usize::MAX/4+1,1),(isize::MAX as usize/16+1,1)] {
        assert_eq!(output_plane(w,h),Err(ae::Error::BadCallbackParameter));
    }
    for (w,h) in [(0,7),(7,0),(1,1),(1,31),(37,1),(17,31)] {
        let out = output_plane(w,h).unwrap();
        assert_eq!(out.len(),w*h*4);
        assert!(out.iter().all(|&v|v==0.0));
    }
}

fn core_render_outcome(
    result: std::thread::Result<Result<(),celglow_core::RenderError>>,
) -> Result<bool,ae::Error> {
    use celglow_core::RenderError;
    match result {
        Ok(Ok(())) => Ok(true),
        // Retain the existing clipped-ROI source fallback only for that error.
        Ok(Err(RenderError::OutputOutsideInput {..})) => Ok(false),
        Ok(Err(RenderError::FieldAllocation)) => Err(ae::Error::OutOfMemory),
        Ok(Err(RenderError::FieldRadius)) => Err(ae::Error::InvalidParms),
        Ok(Err(_)) => Err(ae::Error::BadCallbackParameter),
        Err(_) => Err(ae::Error::InternalStructDamaged),
    }
}

#[test]
fn core_errors_are_not_silently_reported_as_passthrough() {
    use celglow_core::RenderError;
    assert_eq!(core_render_outcome(Ok(Ok(()))),Ok(true));
    assert_eq!(core_render_outcome(Ok(Err(RenderError::FieldAllocation))),Err(ae::Error::OutOfMemory));
    assert_eq!(core_render_outcome(Ok(Err(RenderError::FieldRadius))),Err(ae::Error::InvalidParms));
    assert_eq!(core_render_outcome(Ok(Err(RenderError::FieldDimensions))),Err(ae::Error::BadCallbackParameter));
    assert_eq!(core_render_outcome(Ok(Err(RenderError::InputLength {actual:3,expected:4}))),Err(ae::Error::BadCallbackParameter));
    assert_eq!(core_render_outcome(Ok(Err(RenderError::OutputLength {actual:3,expected:4}))),Err(ae::Error::BadCallbackParameter));
    assert_eq!(core_render_outcome(Err(Box::new("injected panic"))),Err(ae::Error::InternalStructDamaged));
    assert_eq!(core_render_outcome(Ok(Err(RenderError::OutputOutsideInput {
        input_origin:(0,0),input_size:(1,1),output_origin:(1,0),output_size:(1,1),
    }))),Ok(false));
}

fn smart_render_gpu(extra: &ae::pf::SmartRenderExtra, in_data: &ae::InData, params: &ae::Parameters<Params>) -> Result<(), ae::Error> {
    if p_view_is_debug(params)? {
        return smart_render(extra, in_data, params);
    }
    let cb = extra.callbacks();
    let Some(input_world) = cb.checkout_layer_pixels(INPUT_CHECKOUT_ID)? else { return Ok(()); };
    let Some(mut output_world) = checkout_output_or_release(
        || cb.checkout_output(), || cb.checkin_layer_pixels(INPUT_CHECKOUT_ID),
    )? else { return Ok(()); };
    let (rgba, iw, ih) = match layer_to_rgba_f32(&input_world) {
        Ok(frame) => frame,
        Err(err) => { let _ = cb.checkin_layer_pixels(INPUT_CHECKOUT_ID); return Err(err); }
    };
    let ow = output_world.width() as usize;
    let oh = output_world.height() as usize;
    if iw == 0 || ih == 0 || ow == 0 || oh == 0 { let _ = cb.checkin_layer_pixels(INPUT_CHECKOUT_ID); return Ok(()); }
    let plane = extra.pre_render_data::<CelGlowPlane>().map(|v| v.input_plane).unwrap_or(ae::Rect { left: 0, top: 0, right: iw as i32, bottom: ih as i32 });
    let core = match read_core_params(params) {
        Ok(core) => core,
        Err(_) => { let _ = cb.checkin_layer_pixels(INPUT_CHECKOUT_ID); return Err(ae::Error::InvalidParms); }
    };
    let gpu = gpu::render(celglow_core::FrameBuf { w: iw, h: ih, rgba: &rgba }, &core);
    let Ok(full) = gpu else {
        // GPU setup or interop can be unavailable on a host/device. Re-run the
        // exact CPU path after checking the layer back in.
        cb.checkin_layer_pixels(INPUT_CHECKOUT_ID)?;
        return smart_render(extra, in_data, params);
    };
    let ox = in_data.output_origin().h;
    let oy = in_data.output_origin().v;
    let mut out = match output_plane(ow, oh) {
        Ok(out) => out,
        Err(err) => { let _ = cb.checkin_layer_pixels(INPUT_CHECKOUT_ID); return Err(err); }
    };
    if let Err(err) = copy_plane_region(&full,(iw,ih),(plane.left,plane.top),
        &mut out,(ow,oh),(ox,oy)) {
        let _ = cb.checkin_layer_pixels(INPUT_CHECKOUT_ID); return Err(err);
    }
    let result = write_rgba_f32_to_layer(&out, &mut output_world, ow, oh);
    result.and(cb.checkin_layer_pixels(INPUT_CHECKOUT_ID))
}

fn p_view_is_debug(params: &ae::Parameters<Params>) -> Result<bool, ae::Error> {
    Ok(params.get(Params::View)?.as_popup()?.value() as i32 != 1)
}
