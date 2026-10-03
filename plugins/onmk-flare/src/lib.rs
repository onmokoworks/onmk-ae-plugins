use after_effects as ae;

mod flare;
mod gpu;
mod lens;
mod ocular;
mod physical;
mod source_optics;
mod viewer;

use std::sync::OnceLock;

static GPU: OnceLock<Option<gpu::GpuProcessor>> = OnceLock::new();

fn get_gpu() -> Option<&'static gpu::GpuProcessor> {
    GPU.get_or_init(
        || match std::panic::catch_unwind(|| gpu::GpuProcessor::new()) {
            Ok(g) => Some(g),
            Err(_) => None,
        },
    )
    .as_ref()
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    FlareStudio,
    StylePreset,
    GhostComplexity,
    UnifiedDiffraction,
    // Light Source
    LightSource,
    GlobalBrightness,
    GlobalScale,
    FlareAngle,
    // Hotspot
    HotspotIntensity,
    HotspotSize,
    // Glow
    GlowIntensity,
    GlowRadius,
    GlowFalloff,
    GlowColor,
    // Streak
    StreakIntensity,
    StreakLength,
    StreakWidth,
    StreakCount,
    StreakRotation,
    StreakColor,
    // Stripe (anamorphic)
    StripeIntensity,
    StripeLength,
    StripeWidth,
    StripeColor,
    // Ring
    RingIntensity,
    RingRadius,
    RingWidth,
    RingColor,
    RingChromatic,
    RingSpectrum,
    // Starburst
    StarburstIntensity,
    StarburstRadius,
    StarburstBlades,
    StarburstColor,
    // Ghost
    GhostIntensity,
    GhostCount,
    GhostSpread,
    GhostSize,
    GhostColor,
    GhostChromatic,
    PhysicalEnabled,
    LensPreset,
    RayGrid,
    SourceMode,
    SourceThreshold,
    SourceDownsample,
    PhysicalGain,
    GhostNormalize,
    MaxAreaBoost,
    PhysicalGhostBlur,
    PhysicalBlurPasses,
    BloomStrength,
    BloomRadius,
    BloomPasses,
    BloomOctaves,
    BloomChromatic,
    // Human eye glare
    OcularEnabled,
    OcularPupil,
    OcularAge,
    OcularSeed,
    OcularCorona,
    OcularCoronaRadius,
    OcularHalo,
    OcularHaloRadius,
    OcularVeil,
    OcularVeilRadius,
    OcularTear,
    OcularPhase,
    OcularSquint,
    OcularSquintLength,
    OcularSquintCurve,
    // Edge Trigger
    EdgeWidth,
    EdgeBrightness,
    EdgeScale,
    // Atmosphere
    AtmosphereAmount,
    AtmosphereScale,
    // Global effects
    ChromaticAmount,
    FlickerAmount,
    FlickerPhase,
    // Output
    SourceOpacity,
    FlareOpacity,
    TransferMode,
}

#[derive(Default)]
struct Plugin;

ae::define_effect!(Plugin, (), Params);

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        in_data: ae::InData,
        _out_data: ae::OutData,
    ) -> Result<(), ae::Error> {
        // `after-effects` 0.3.0 transfers its global-data PF_Handle with
        // Handle::into_raw(), which forgets both the allocation owner and the
        // PF Handle Suite guard. The allocation is intentionally transferred,
        // but the suite acquisition must not remain live for the whole plugin
        // session. Balance that guard here; later commands acquire their own
        // short-lived Handle Suite guards when accessing global_data.
        unsafe {
            let basic = in_data.pica_basic_suite_ptr();
            if !basic.is_null() {
                if let Some(release_suite) = (*basic).ReleaseSuite {
                    release_suite(
                        ae::sys::kPFHandleSuite.as_ptr() as *const i8,
                        ae::sys::kPFHandleSuiteVersion1 as i32,
                    );
                }
            }
        }

        // ---- Light Source ----
        params.add(
            Params::FlareStudio,
            "Flare Studio",
            ae::ButtonDef::setup(|f| {
                f.set_label("Open Flare Studio...");
            }),
        )?;
        params.add(
            Params::StylePreset,
            "Character",
            ae::PopupDef::setup(|f| {
                f.set_options(&[
                    "Natural",
                    "Cinematic",
                    "Anamorphic",
                    "Vintage",
                    "Dream",
                    "Legacy / Custom",
                    "Legacy / Anamorphic Blue",
                    "Legacy / Golden Cinema",
                    "Legacy / Sci-Fi Prism",
                    "Legacy / Vintage 35mm",
                    "Legacy / Dream Bloom",
                    "Legacy / Ocular Night",
                    "Legacy / Solar Blast",
                ]);
                f.set_default(2);
            }),
        )?;
        params.add(
            Params::GhostComplexity,
            "Ghost Complexity",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_default(60.0);
                f.set_precision(1);
            }),
        )?;
        params.add(
            Params::UnifiedDiffraction,
            "Diffraction",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_default(18.0);
                f.set_precision(1);
            }),
        )?;
        params.add(
            Params::LightSource,
            "Light Source",
            ae::PointDef::setup(|f| {
                f.set_default((36.0, 43.0));
                f.set_restrict_bounds(false);
            }),
        )?;
        params.add(
            Params::GlobalBrightness,
            "Global Brightness",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(500.0);
                f.set_slider_min(0.0);
                f.set_slider_max(500.0);
                f.set_default(100.0);
                f.set_precision(1);
            }),
        )?;
        params.add(
            Params::GlobalScale,
            "Global Scale",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(500.0);
                f.set_slider_min(0.0);
                f.set_slider_max(500.0);
                f.set_default(100.0);
                f.set_precision(1);
            }),
        )?;
        params.add(
            Params::FlareAngle,
            "Flare Angle",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(-360.0);
                f.set_valid_max(360.0);
                f.set_slider_min(-180.0);
                f.set_slider_max(180.0);
                f.set_default(0.0);
                f.set_precision(1);
            }),
        )?;

        // ---- Hotspot ----
        params.add(
            Params::HotspotIntensity,
            "Hotspot Intensity",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(500.0);
                f.set_slider_min(0.0);
                f.set_slider_max(500.0);
                f.set_default(160.0);
                f.set_precision(1);
            }),
        )?;
        params.add(
            Params::HotspotSize,
            "Hotspot Size",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(50.0);
                f.set_default(8.0);
                f.set_precision(1);
            }),
        )?;

        // ---- Glow ----
        params.add(
            Params::GlowIntensity,
            "Glow Intensity",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(200.0);
                f.set_slider_min(0.0);
                f.set_slider_max(200.0);
                f.set_default(85.0);
                f.set_precision(1);
            }),
        )?;
        params.add(
            Params::GlowRadius,
            "Glow Radius",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(1000.0);
                f.set_slider_min(0.0);
                f.set_slider_max(500.0);
                f.set_default(100.0);
                f.set_precision(1);
            }),
        )?;
        params.add(
            Params::GlowFalloff,
            "Glow Falloff",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.5);
                f.set_valid_max(10.0);
                f.set_slider_min(0.5);
                f.set_slider_max(5.0);
                f.set_default(2.0);
                f.set_precision(2);
            }),
        )?;
        params.add(
            Params::GlowColor,
            "Glow Color",
            ae::ColorDef::setup(|f| {
                f.set_default(ae::Pixel8 {
                    alpha: 255,
                    red: 255,
                    green: 214,
                    blue: 164,
                });
            }),
        )?;

        // ---- Streak ----
        params.add(
            Params::StreakIntensity,
            "Streak Intensity",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(200.0);
                f.set_slider_min(0.0);
                f.set_slider_max(200.0);
                f.set_default(65.0);
                f.set_precision(1);
            }),
        )?;
        params.add(
            Params::StreakLength,
            "Streak Length",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(2000.0);
                f.set_slider_min(0.0);
                f.set_slider_max(1000.0);
                f.set_default(300.0);
                f.set_precision(1);
            }),
        )?;
        params.add(
            Params::StreakWidth,
            "Streak Width",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.5);
                f.set_valid_max(100.0);
                f.set_slider_min(0.5);
                f.set_slider_max(50.0);
                f.set_default(3.0);
                f.set_precision(1);
            }),
        )?;
        params.add(
            Params::StreakCount,
            "Streak Count",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(1.0);
                f.set_valid_max(16.0);
                f.set_slider_min(1.0);
                f.set_slider_max(16.0);
                f.set_default(6.0);
                f.set_precision(0);
            }),
        )?;
        params.add(
            Params::StreakRotation,
            "Streak Rotation",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(-180.0);
                f.set_valid_max(180.0);
                f.set_slider_min(-180.0);
                f.set_slider_max(180.0);
                f.set_default(0.0);
                f.set_precision(1);
            }),
        )?;
        params.add(
            Params::StreakColor,
            "Streak Color",
            ae::ColorDef::setup(|f| {
                f.set_default(ae::Pixel8 {
                    alpha: 255,
                    red: 255,
                    green: 225,
                    blue: 190,
                });
            }),
        )?;

        // ---- Stripe (anamorphic) ----
        params.add(
            Params::StripeIntensity,
            "Stripe Intensity",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(200.0);
                f.set_slider_min(0.0);
                f.set_slider_max(200.0);
                f.set_default(75.0);
                f.set_precision(1);
            }),
        )?;
        params.add(
            Params::StripeLength,
            "Stripe Length",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(1000000.0);
                f.set_slider_min(0.0);
                f.set_slider_max(2000.0);
                f.set_default(800.0);
                f.set_precision(1);
            }),
        )?;
        params.add(
            Params::StripeWidth,
            "Stripe Width",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.5);
                f.set_valid_max(50.0);
                f.set_slider_min(0.5);
                f.set_slider_max(20.0);
                f.set_default(2.0);
                f.set_precision(1);
            }),
        )?;
        params.add(
            Params::StripeColor,
            "Stripe Color",
            ae::ColorDef::setup(|f| {
                f.set_default(ae::Pixel8 {
                    alpha: 255,
                    red: 92,
                    green: 170,
                    blue: 255,
                });
            }),
        )?;

        // ---- Ring ----
        params.add(
            Params::RingIntensity,
            "Ring Intensity",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(200.0);
                f.set_slider_min(0.0);
                f.set_slider_max(200.0);
                f.set_default(32.0);
                f.set_precision(1);
            }),
        )?;
        params.add(
            Params::RingRadius,
            "Ring Radius",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(1000.0);
                f.set_slider_min(0.0);
                f.set_slider_max(500.0);
                f.set_default(120.0);
                f.set_precision(1);
            }),
        )?;
        params.add(
            Params::RingWidth,
            "Ring Width",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.5);
                f.set_valid_max(200.0);
                f.set_slider_min(0.5);
                f.set_slider_max(100.0);
                f.set_default(8.0);
                f.set_precision(1);
            }),
        )?;
        params.add(
            Params::RingColor,
            "Ring Color",
            ae::ColorDef::setup(|f| {
                f.set_default(ae::Pixel8 {
                    alpha: 255,
                    red: 200,
                    green: 220,
                    blue: 255,
                });
            }),
        )?;
        params.add(
            Params::RingChromatic,
            "Ring Chromatic",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_default(55.0);
                f.set_precision(1);
            }),
        )?;
        params.add(
            Params::RingSpectrum,
            "Ring Spectrum",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(false);
                f.set_label("Rainbow");
            }),
        )?;

        // ---- Starburst ----
        params.add(
            Params::StarburstIntensity,
            "Starburst Intensity",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(200.0);
                f.set_slider_min(0.0);
                f.set_slider_max(200.0);
                f.set_default(18.0);
                f.set_precision(1);
            }),
        )?;
        params.add(
            Params::StarburstRadius,
            "Starburst Radius",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(500.0);
                f.set_slider_min(0.0);
                f.set_slider_max(300.0);
                f.set_default(80.0);
                f.set_precision(1);
            }),
        )?;
        params.add(
            Params::StarburstBlades,
            "Starburst Blades",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(3.0);
                f.set_valid_max(16.0);
                f.set_slider_min(3.0);
                f.set_slider_max(16.0);
                f.set_default(6.0);
                f.set_precision(0);
            }),
        )?;
        params.add(
            Params::StarburstColor,
            "Starburst Color",
            ae::ColorDef::setup(|f| {
                f.set_default(ae::Pixel8 {
                    alpha: 255,
                    red: 230,
                    green: 240,
                    blue: 255,
                });
            }),
        )?;

        // ---- Ghost ----
        params.add(
            Params::GhostIntensity,
            "Ghost Intensity",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(200.0);
                f.set_slider_min(0.0);
                f.set_slider_max(200.0);
                f.set_default(100.0);
                f.set_precision(1);
            }),
        )?;
        params.add(
            Params::GhostCount,
            "Ghost Count",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(12.0);
                f.set_slider_min(0.0);
                f.set_slider_max(12.0);
                f.set_default(7.0);
                f.set_precision(0);
            }),
        )?;
        params.add(
            Params::GhostSpread,
            "Ghost Spread",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(300.0);
                f.set_slider_min(0.0);
                f.set_slider_max(200.0);
                f.set_default(100.0);
                f.set_precision(1);
            }),
        )?;
        params.add(
            Params::GhostSize,
            "Ghost Size",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(1.0);
                f.set_valid_max(200.0);
                f.set_slider_min(1.0);
                f.set_slider_max(100.0);
                f.set_default(25.0);
                f.set_precision(1);
            }),
        )?;
        params.add(
            Params::GhostColor,
            "Ghost Color",
            ae::ColorDef::setup(|f| {
                f.set_default(ae::Pixel8 {
                    alpha: 255,
                    red: 112,
                    green: 178,
                    blue: 255,
                });
            }),
        )?;
        params.add(
            Params::GhostChromatic,
            "Ghost Chromatic",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_default(55.0);
                f.set_precision(1);
            }),
        )?;

        params.add(
            Params::PhysicalEnabled,
            "Physical Renderer",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(true);
            }),
        )?;
        params.add(
            Params::LensPreset,
            "Lens Model",
            ae::PopupDef::setup(|f| {
                f.set_options(&[
                    "Cooke Triplet",
                    "Double Gauss",
                    "ARRI Master Prime 50mm",
                    "Canon EF 200-400mm",
                ]);
                f.set_default(2);
            }),
        )?;
        params.add(
            Params::RayGrid,
            "Ray Grid",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(4.0);
                f.set_valid_max(96.0);
                f.set_slider_min(4.0);
                f.set_slider_max(64.0);
                f.set_default(40.0);
                f.set_precision(0);
            }),
        )?;
        params.add(
            Params::SourceMode,
            "Bright Sources",
            ae::PopupDef::setup(|f| {
                f.set_options(&["Manual", "Highlights", "Manual + Highlights"]);
                f.set_default(1);
            }),
        )?;
        params.add(
            Params::SourceThreshold,
            "Source Threshold",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(50.0);
                f.set_slider_max(100.0);
                f.set_default(92.0);
                f.set_precision(1);
            }),
        )?;
        params.add(
            Params::SourceDownsample,
            "Source Downsample",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(1.0);
                f.set_valid_max(32.0);
                f.set_slider_min(1.0);
                f.set_slider_max(16.0);
                f.set_default(8.0);
                f.set_precision(0);
            }),
        )?;
        params.add(
            Params::PhysicalGain,
            "Optical Gain",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(20000.0);
                f.set_slider_min(0.0);
                f.set_slider_max(10000.0);
                f.set_default(3000.0);
                f.set_precision(0);
            }),
        )?;
        params.add(
            Params::GhostNormalize,
            "Area Normalize",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(true);
            }),
        )?;
        params.add(
            Params::MaxAreaBoost,
            "Max Area Boost",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(1.0);
                f.set_valid_max(20.0);
                f.set_slider_min(1.0);
                f.set_slider_max(20.0);
                f.set_default(8.0);
                f.set_precision(1);
            }),
        )?;
        params.add(
            Params::PhysicalGhostBlur,
            "Optical Ghost Blur %",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(5.0);
                f.set_slider_min(0.0);
                f.set_slider_max(2.0);
                f.set_default(0.5);
                f.set_precision(2);
            }),
        )?;
        params.add(
            Params::PhysicalBlurPasses,
            "Ghost Blur Passes",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(1.0);
                f.set_valid_max(5.0);
                f.set_slider_min(1.0);
                f.set_slider_max(5.0);
                f.set_default(3.0);
                f.set_precision(0);
            }),
        )?;
        params.add(
            Params::BloomStrength,
            "Chromatic Bloom",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(200.0);
                f.set_slider_min(0.0);
                f.set_slider_max(200.0);
                f.set_default(60.0);
                f.set_precision(1);
            }),
        )?;
        params.add(
            Params::BloomRadius,
            "Bloom Radius %",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(20.0);
                f.set_slider_min(0.0);
                f.set_slider_max(10.0);
                f.set_default(1.8);
                f.set_precision(2);
            }),
        )?;
        params.add(
            Params::BloomPasses,
            "Bloom Passes",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(1.0);
                f.set_valid_max(5.0);
                f.set_slider_min(1.0);
                f.set_slider_max(5.0);
                f.set_default(3.0);
                f.set_precision(0);
            }),
        )?;
        params.add(
            Params::BloomOctaves,
            "Bloom Octaves",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(1.0);
                f.set_valid_max(6.0);
                f.set_slider_min(1.0);
                f.set_slider_max(6.0);
                f.set_default(4.0);
                f.set_precision(0);
            }),
        )?;
        params.add(
            Params::BloomChromatic,
            "Bloom Chromatic",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(true);
            }),
        )?;

        // ---- Human Eye Glare ----
        params.add(
            Params::OcularEnabled,
            "Human Eye Glare",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(false);
            }),
        )?;
        params.add(
            Params::OcularPupil,
            "Eye Pupil mm",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(2.0);
                f.set_valid_max(8.0);
                f.set_slider_min(2.0);
                f.set_slider_max(8.0);
                f.set_default(6.0);
                f.set_precision(2);
            }),
        )?;
        params.add(
            Params::OcularAge,
            "Eye Age",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(10.0);
                f.set_valid_max(100.0);
                f.set_slider_min(10.0);
                f.set_slider_max(90.0);
                f.set_default(30.0);
                f.set_precision(0);
            }),
        )?;
        params.add(
            Params::OcularSeed,
            "Eye Seed",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(9999.0);
                f.set_slider_min(0.0);
                f.set_slider_max(999.0);
                f.set_default(17.0);
                f.set_precision(0);
            }),
        )?;
        for (id, name, default, max) in [
            (Params::OcularCorona, "Ciliary Corona", 75.0, 300.0),
            (Params::OcularHalo, "Lenticular Halo", 18.0, 200.0),
            (Params::OcularVeil, "Veiling Glare", 28.0, 200.0),
            (Params::OcularTear, "Tear Film Motion", 20.0, 100.0),
            (Params::OcularSquint, "Squint / Eyelash", 0.0, 300.0),
        ] {
            params.add(
                id,
                name,
                ae::FloatSliderDef::setup(|f| {
                    f.set_valid_min(0.0);
                    f.set_valid_max(max);
                    f.set_slider_min(0.0);
                    f.set_slider_max(max.min(200.0));
                    f.set_default(default);
                    f.set_precision(1);
                }),
            )?;
        }
        for (id, name, default, max) in [
            (Params::OcularCoronaRadius, "Corona Radius %", 22.0, 100.0),
            (Params::OcularHaloRadius, "Halo Radius %", 14.0, 100.0),
            (Params::OcularVeilRadius, "Veil Radius %", 65.0, 200.0),
            (Params::OcularSquintLength, "Squint Length %", 85.0, 200.0),
            (Params::OcularSquintCurve, "Squint Curve", 28.0, 100.0),
        ] {
            params.add(
                id,
                name,
                ae::FloatSliderDef::setup(|f| {
                    f.set_valid_min(0.0);
                    f.set_valid_max(max);
                    f.set_slider_min(0.0);
                    f.set_slider_max(max.min(100.0));
                    f.set_default(default);
                    f.set_precision(1);
                }),
            )?;
        }
        params.add(
            Params::OcularPhase,
            "Eye Motion Phase",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(-36000.0);
                f.set_valid_max(36000.0);
                f.set_slider_min(0.0);
                f.set_slider_max(360.0);
                f.set_default(0.0);
                f.set_precision(2);
            }),
        )?;

        // ---- Edge Trigger ----
        params.add(
            Params::EdgeWidth,
            "Edge Trigger Width",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(50.0);
                f.set_default(35.0);
                f.set_precision(1);
            }),
        )?;
        params.add(
            Params::EdgeBrightness,
            "Edge Trigger Brightness",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(500.0);
                f.set_slider_min(0.0);
                f.set_slider_max(300.0);
                f.set_default(150.0);
                f.set_precision(1);
            }),
        )?;
        params.add(
            Params::EdgeScale,
            "Edge Trigger Scale",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(500.0);
                f.set_slider_min(0.0);
                f.set_slider_max(300.0);
                f.set_default(120.0);
                f.set_precision(1);
            }),
        )?;

        // ---- Atmosphere ----
        params.add(
            Params::AtmosphereAmount,
            "Atmosphere Amount",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_default(0.0);
                f.set_precision(1);
            }),
        )?;
        params.add(
            Params::AtmosphereScale,
            "Atmosphere Scale",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.1);
                f.set_valid_max(20.0);
                f.set_slider_min(0.1);
                f.set_slider_max(10.0);
                f.set_default(3.0);
                f.set_precision(2);
            }),
        )?;

        // ---- Global effects ----
        params.add(
            Params::ChromaticAmount,
            "Chromatic Aberration",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_default(15.0);
                f.set_precision(1);
            }),
        )?;
        params.add(
            Params::FlickerAmount,
            "Flicker Amount",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_default(0.0);
                f.set_precision(1);
            }),
        )?;
        params.add(
            Params::FlickerPhase,
            "Flicker Phase",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(3600.0);
                f.set_slider_min(0.0);
                f.set_slider_max(360.0);
                f.set_default(0.0);
                f.set_precision(1);
            }),
        )?;

        // ---- Output ----
        params.add(
            Params::SourceOpacity,
            "Source Opacity",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_default(100.0);
                f.set_precision(1);
            }),
        )?;
        params.add(
            Params::FlareOpacity,
            "Flare Opacity",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_default(100.0);
                f.set_precision(1);
            }),
        )?;
        params.add(
            Params::TransferMode,
            "Transfer Mode",
            ae::PopupDef::setup(|f| {
                f.set_options(&[
                    "None",
                    "Normal",
                    "(-",
                    "Add",
                    "Screen",
                    "(-",
                    "Overlay",
                    "Soft Light",
                ]);
                f.set_default(4);
            }),
        )?;

        Ok(())
    }

    fn handle_command(
        &mut self,
        cmd: ae::Command,
        in_data: ae::InData,
        mut out_data: ae::OutData,
        params: &mut ae::Parameters<Params>,
    ) -> Result<(), ae::Error> {
        match cmd {
            ae::Command::About => {
                out_data.set_return_msg(
                    "onmkFlare v2.0\rCinematic lens flare generator.\rHotspot, glow, streaks, stripe, rings,\rstarburst, ghosts, edge trigger, atmosphere.\rGPU-accelerated. Written in Rust.",
                );
            }
            ae::Command::UserChangedParam { param_index } => {
                if params.type_at(param_index) == Params::FlareStudio {
                    if let Err(err) = viewer::open() {
                        out_data.set_return_msg(&format!("Flare Studioを開けませんでした: {err}"));
                        out_data.set_out_flag(ae::OutFlags::DisplayErrorMessage, true);
                    }
                }
            }

            ae::Command::Render {
                in_layer,
                mut out_layer,
            } => {
                let ep = get_params(params, &in_data)?;
                let (original, w, h) = layer_to_flat(&in_layer);
                let result = render_flare(&ep, &original, w, h);
                flat_to_layer(&result, &mut out_layer, w, h);
            }

            ae::Command::SmartPreRender { mut extra } => {
                let req = extra.output_request();
                let cb = extra.callbacks();
                let in_result = cb.checkout_layer(
                    0,
                    0,
                    &req,
                    in_data.current_time(),
                    in_data.time_step(),
                    in_data.time_scale(),
                )?;
                let req_rect: ae::Rect = req.rect.into();
                let mut res: ae::Rect = in_result.result_rect.into();
                let mut max_res: ae::Rect = in_result.max_result_rect.into();
                res.left = res.left.max(req_rect.left);
                res.top = res.top.max(req_rect.top);
                res.right = res.right.min(req_rect.right);
                res.bottom = res.bottom.min(req_rect.bottom);
                max_res.left = max_res.left.max(req_rect.left);
                max_res.top = max_res.top.max(req_rect.top);
                max_res.right = max_res.right.min(req_rect.right);
                max_res.bottom = max_res.bottom.min(req_rect.bottom);
                extra.set_result_rect(res);
                extra.set_max_result_rect(max_res);
            }

            ae::Command::SmartRender { extra } | ae::Command::SmartRenderGpu { extra } => {
                let ep = get_params(params, &in_data)?;
                let cb = extra.callbacks();
                let input_world = cb.checkout_layer_pixels(0)?.ok_or(ae::Error::Generic)?;
                let mut output_world = cb.checkout_output()?.ok_or(ae::Error::Generic)?;
                let (original, w, h) = layer_to_flat(&input_world);
                let result = render_flare(&ep, &original, w, h);
                flat_to_layer(&result, &mut output_world, w, h);
                cb.checkin_layer_pixels(0)?;
            }

            _ => {}
        }
        Ok(())
    }
}

// ---- Parameters ----

#[cfg_attr(test, derive(Default))]
#[derive(Clone, PartialEq)]
pub struct EffectParams {
    pub style_preset: i32,
    pub ghost_complexity: f64,
    pub unified_diffraction: f64,
    pub light_x: f64,
    pub light_y: f64,
    pub global_brightness: f64,
    pub global_scale: f64,
    pub flare_angle: f64,
    // Hotspot
    pub hotspot_intensity: f64,
    pub hotspot_size: f64,
    // Glow
    pub glow_intensity: f64,
    pub glow_radius: f64,
    pub glow_falloff: f64,
    pub glow_color: [f64; 3],
    // Streak
    pub streak_intensity: f64,
    pub streak_length: f64,
    pub streak_width: f64,
    pub streak_count: i32,
    pub streak_rotation: f64,
    pub streak_color: [f64; 3],
    // Stripe
    pub stripe_intensity: f64,
    pub stripe_length: f64,
    pub stripe_width: f64,
    pub stripe_color: [f64; 3],
    // Ring
    pub ring_intensity: f64,
    pub ring_radius: f64,
    pub ring_width: f64,
    pub ring_color: [f64; 3],
    pub ring_chromatic: f64,
    pub ring_spectrum: bool,
    // Starburst
    pub starburst_intensity: f64,
    pub starburst_radius: f64,
    pub starburst_blades: i32,
    pub starburst_color: [f64; 3],
    // Ghost
    pub ghost_intensity: f64,
    pub ghost_count: i32,
    pub ghost_spread: f64,
    pub ghost_size: f64,
    pub ghost_color: [f64; 3],
    pub ghost_chromatic: f64,
    pub physical_enabled: bool,
    pub lens_preset: usize,
    pub ray_grid: usize,
    pub source_mode: i32,
    pub source_threshold: f32,
    pub source_downsample: usize,
    pub physical_gain: f32,
    pub ghost_normalize: bool,
    pub max_area_boost: f32,
    pub physical_ghost_blur: f32,
    pub physical_blur_passes: usize,
    pub bloom_strength: f32,
    pub bloom_radius: f32,
    pub bloom_passes: usize,
    pub bloom_octaves: usize,
    pub bloom_chromatic: bool,
    pub ocular_enabled: bool,
    pub ocular_pupil_mm: f32,
    pub ocular_age: f32,
    pub ocular_seed: i32,
    pub ocular_corona: f32,
    pub ocular_corona_radius: f32,
    pub ocular_halo: f32,
    pub ocular_halo_radius: f32,
    pub ocular_veil: f32,
    pub ocular_veil_radius: f32,
    pub ocular_tear: f32,
    pub ocular_phase: f32,
    pub ocular_squint: f32,
    pub ocular_squint_length: f32,
    pub ocular_squint_curve: f32,
    // Edge Trigger
    pub edge_width: f64,
    pub edge_brightness: f64,
    pub edge_scale: f64,
    // Atmosphere
    pub atmosphere_amount: f64,
    pub atmosphere_scale: f64,
    // Global effects
    pub chromatic_amount: f64,
    pub flicker_amount: f64,
    pub flicker_phase: f64,
    // Output
    pub source_opacity: f64,
    pub flare_opacity: f64,
    pub transfer_mode: i32,
}

fn color_to_f64(p: ae::Pixel8) -> [f64; 3] {
    [
        p.red as f64 / 255.0,
        p.green as f64 / 255.0,
        p.blue as f64 / 255.0,
    ]
}

fn get_params(
    params: &ae::Parameters<Params>,
    _in_data: &ae::InData,
) -> Result<EffectParams, ae::Error> {
    let (px, py) = params.get(Params::LightSource)?.as_point()?.value();
    let mut effect = EffectParams {
        style_preset: params.get(Params::StylePreset)?.as_popup()?.value() as i32,
        ghost_complexity: params
            .get(Params::GhostComplexity)?
            .as_float_slider()?
            .value()
            / 100.0,
        unified_diffraction: params
            .get(Params::UnifiedDiffraction)?
            .as_float_slider()?
            .value()
            / 100.0,
        // Point parameters are already returned in layer pixel coordinates.
        // Scaling them again by the frame size pushed the emitter far outside
        // the image and made the edge trigger erase the flare.
        light_x: px as f64,
        light_y: py as f64,
        global_brightness: params
            .get(Params::GlobalBrightness)?
            .as_float_slider()?
            .value()
            / 100.0,
        global_scale: params.get(Params::GlobalScale)?.as_float_slider()?.value() / 100.0,
        flare_angle: params.get(Params::FlareAngle)?.as_float_slider()?.value(),
        hotspot_intensity: params
            .get(Params::HotspotIntensity)?
            .as_float_slider()?
            .value()
            / 100.0,
        hotspot_size: params.get(Params::HotspotSize)?.as_float_slider()?.value(),
        glow_intensity: params
            .get(Params::GlowIntensity)?
            .as_float_slider()?
            .value()
            / 100.0,
        glow_radius: params.get(Params::GlowRadius)?.as_float_slider()?.value(),
        glow_falloff: params.get(Params::GlowFalloff)?.as_float_slider()?.value(),
        glow_color: color_to_f64(params.get(Params::GlowColor)?.as_color()?.value()),
        streak_intensity: params
            .get(Params::StreakIntensity)?
            .as_float_slider()?
            .value()
            / 100.0,
        streak_length: params.get(Params::StreakLength)?.as_float_slider()?.value(),
        streak_width: params.get(Params::StreakWidth)?.as_float_slider()?.value(),
        streak_count: params.get(Params::StreakCount)?.as_float_slider()?.value() as i32,
        streak_rotation: params
            .get(Params::StreakRotation)?
            .as_float_slider()?
            .value(),
        streak_color: color_to_f64(params.get(Params::StreakColor)?.as_color()?.value()),
        stripe_intensity: params
            .get(Params::StripeIntensity)?
            .as_float_slider()?
            .value()
            / 100.0,
        stripe_length: params.get(Params::StripeLength)?.as_float_slider()?.value(),
        stripe_width: params.get(Params::StripeWidth)?.as_float_slider()?.value(),
        stripe_color: color_to_f64(params.get(Params::StripeColor)?.as_color()?.value()),
        ring_intensity: params
            .get(Params::RingIntensity)?
            .as_float_slider()?
            .value()
            / 100.0,
        ring_radius: params.get(Params::RingRadius)?.as_float_slider()?.value(),
        ring_width: params.get(Params::RingWidth)?.as_float_slider()?.value(),
        ring_color: color_to_f64(params.get(Params::RingColor)?.as_color()?.value()),
        ring_chromatic: params
            .get(Params::RingChromatic)?
            .as_float_slider()?
            .value()
            / 100.0,
        ring_spectrum: params.get(Params::RingSpectrum)?.as_checkbox()?.value(),
        starburst_intensity: params
            .get(Params::StarburstIntensity)?
            .as_float_slider()?
            .value()
            / 100.0,
        starburst_radius: params
            .get(Params::StarburstRadius)?
            .as_float_slider()?
            .value(),
        starburst_blades: params
            .get(Params::StarburstBlades)?
            .as_float_slider()?
            .value() as i32,
        starburst_color: color_to_f64(params.get(Params::StarburstColor)?.as_color()?.value()),
        ghost_intensity: params
            .get(Params::GhostIntensity)?
            .as_float_slider()?
            .value()
            / 100.0,
        ghost_count: params.get(Params::GhostCount)?.as_float_slider()?.value() as i32,
        ghost_spread: params.get(Params::GhostSpread)?.as_float_slider()?.value() / 100.0,
        ghost_size: params.get(Params::GhostSize)?.as_float_slider()?.value(),
        ghost_color: color_to_f64(params.get(Params::GhostColor)?.as_color()?.value()),
        ghost_chromatic: params
            .get(Params::GhostChromatic)?
            .as_float_slider()?
            .value()
            / 100.0,
        physical_enabled: params.get(Params::PhysicalEnabled)?.as_checkbox()?.value(),
        lens_preset: params
            .get(Params::LensPreset)?
            .as_popup()?
            .value()
            .saturating_sub(1) as usize,
        ray_grid: params.get(Params::RayGrid)?.as_float_slider()?.value() as usize,
        source_mode: params.get(Params::SourceMode)?.as_popup()?.value() as i32,
        source_threshold: (params
            .get(Params::SourceThreshold)?
            .as_float_slider()?
            .value()
            / 100.0) as f32,
        source_downsample: params
            .get(Params::SourceDownsample)?
            .as_float_slider()?
            .value() as usize,
        physical_gain: params
            .get(Params::PhysicalGain)?
            .as_float_slider()?
            .value()
            .clamp(0.0, 20_000.0) as f32,
        ghost_normalize: params.get(Params::GhostNormalize)?.as_checkbox()?.value(),
        max_area_boost: params
            .get(Params::MaxAreaBoost)?
            .as_float_slider()?
            .value()
            .clamp(1.0, 20.0) as f32,
        physical_ghost_blur: (params
            .get(Params::PhysicalGhostBlur)?
            .as_float_slider()?
            .value()
            / 100.0) as f32,
        physical_blur_passes: params
            .get(Params::PhysicalBlurPasses)?
            .as_float_slider()?
            .value() as usize,
        bloom_strength: (params
            .get(Params::BloomStrength)?
            .as_float_slider()?
            .value()
            .clamp(0.0, 200.0)
            / 100.0) as f32,
        bloom_radius: (params.get(Params::BloomRadius)?.as_float_slider()?.value() / 100.0) as f32,
        bloom_passes: params.get(Params::BloomPasses)?.as_float_slider()?.value() as usize,
        bloom_octaves: params.get(Params::BloomOctaves)?.as_float_slider()?.value() as usize,
        bloom_chromatic: params.get(Params::BloomChromatic)?.as_checkbox()?.value(),
        ocular_enabled: params.get(Params::OcularEnabled)?.as_checkbox()?.value(),
        ocular_pupil_mm: params.get(Params::OcularPupil)?.as_float_slider()?.value() as f32,
        ocular_age: params.get(Params::OcularAge)?.as_float_slider()?.value() as f32,
        ocular_seed: params.get(Params::OcularSeed)?.as_float_slider()?.value() as i32,
        ocular_corona: (params.get(Params::OcularCorona)?.as_float_slider()?.value() / 100.0)
            as f32,
        ocular_corona_radius: (params
            .get(Params::OcularCoronaRadius)?
            .as_float_slider()?
            .value()
            / 100.0) as f32,
        ocular_halo: (params.get(Params::OcularHalo)?.as_float_slider()?.value() / 100.0) as f32,
        ocular_halo_radius: (params
            .get(Params::OcularHaloRadius)?
            .as_float_slider()?
            .value()
            / 100.0) as f32,
        ocular_veil: (params.get(Params::OcularVeil)?.as_float_slider()?.value() / 100.0) as f32,
        ocular_veil_radius: (params
            .get(Params::OcularVeilRadius)?
            .as_float_slider()?
            .value()
            / 100.0) as f32,
        ocular_tear: (params.get(Params::OcularTear)?.as_float_slider()?.value() / 100.0) as f32,
        ocular_phase: params.get(Params::OcularPhase)?.as_float_slider()?.value() as f32,
        ocular_squint: (params.get(Params::OcularSquint)?.as_float_slider()?.value() / 100.0)
            as f32,
        ocular_squint_length: (params
            .get(Params::OcularSquintLength)?
            .as_float_slider()?
            .value()
            / 100.0) as f32,
        ocular_squint_curve: (params
            .get(Params::OcularSquintCurve)?
            .as_float_slider()?
            .value()
            / 100.0) as f32,
        edge_width: params.get(Params::EdgeWidth)?.as_float_slider()?.value() / 100.0,
        edge_brightness: params
            .get(Params::EdgeBrightness)?
            .as_float_slider()?
            .value()
            / 100.0,
        edge_scale: params.get(Params::EdgeScale)?.as_float_slider()?.value() / 100.0,
        atmosphere_amount: params
            .get(Params::AtmosphereAmount)?
            .as_float_slider()?
            .value()
            / 100.0,
        atmosphere_scale: params
            .get(Params::AtmosphereScale)?
            .as_float_slider()?
            .value(),
        chromatic_amount: params
            .get(Params::ChromaticAmount)?
            .as_float_slider()?
            .value()
            / 100.0,
        flicker_amount: params
            .get(Params::FlickerAmount)?
            .as_float_slider()?
            .value()
            / 100.0,
        flicker_phase: params.get(Params::FlickerPhase)?.as_float_slider()?.value(),
        source_opacity: params
            .get(Params::SourceOpacity)?
            .as_float_slider()?
            .value()
            / 100.0,
        flare_opacity: params.get(Params::FlareOpacity)?.as_float_slider()?.value() / 100.0,
        transfer_mode: params.get(Params::TransferMode)?.as_popup()?.value() as i32,
    };
    apply_style_preset(&mut effect);
    Ok(effect)
}

fn apply_style_preset(p: &mut EffectParams) {
    if p.style_preset >= 6 {
        apply_legacy_preset(p);
        return;
    }
    // One optical pipeline: character changes its balance, never the renderer.
    p.physical_enabled = true;
    let complexity = p.ghost_complexity.clamp(0.0, 1.0);
    let diffraction = p.unified_diffraction.clamp(0.0, 1.0);
    // Triangle reconstruction preserves the aperture silhouette at a much
    // lower sampling density than the old point splats.  Spend samples on
    // shape, not redundant rays.
    p.ray_grid = (14.0 + 20.0 * complexity) as usize;
    // Keep the user-facing optical gain authoritative.  Character presets
    // only shape its balance; replacing it with six-figure values caused the
    // reconstructed ghost surfaces to saturate into frame-sized polygons.
    p.physical_gain *= (0.7 + 0.8 * complexity) as f32;
    p.physical_ghost_blur = (0.009 - 0.0055 * complexity) as f32;
    p.ghost_intensity = 1.25 + 2.75 * complexity;
    p.bloom_strength *= (0.7 + 0.6 * complexity) as f32;
    p.bloom_octaves = (3.0 + 3.0 * complexity) as usize;
    p.ocular_enabled = false;
    p.ocular_corona = (0.12 + 0.72 * diffraction) as f32;
    p.ocular_veil = (0.06 + 0.24 * diffraction) as f32;
    p.ocular_halo = (0.01 + 0.09 * diffraction) as f32;
    p.ocular_tear = (0.05 + 0.24 * diffraction) as f32;
    p.ocular_squint = 0.0;
    // Legacy geometric elements become restrained diffraction accents.
    p.ring_intensity *= 0.16;
    p.starburst_intensity = 0.02 + 0.10 * diffraction;
    p.streak_intensity = 0.04 + 0.15 * diffraction;
    match p.style_preset {
        1 => {
            // Natural
            p.glow_intensity *= 0.72;
            p.stripe_intensity *= 0.22;
            p.chromatic_amount *= 0.55;
            p.ocular_squint *= 0.45;
        }
        2 => {
            // Cinematic
            p.glow_intensity *= 1.12;
            p.glow_color = [1.0, 0.48, 0.12];
            p.stripe_intensity *= 0.4;
            p.bloom_chromatic = true;
        }
        3 => {
            // Anamorphic
            p.stripe_intensity = 0.72;
            p.stripe_length = 720.0;
            p.stripe_width = 1.7;
            p.stripe_color = [0.16, 0.43, 1.0];
            p.glow_color = [0.34, 0.58, 1.0];
            p.chromatic_amount = 0.7;
        }
        4 => {
            // Vintage
            p.glow_intensity *= 1.28;
            p.glow_color = [1.0, 0.56, 0.25];
            p.physical_gain *= 1.35;
            p.physical_ghost_blur *= 1.5;
            p.atmosphere_amount = p.atmosphere_amount.max(0.12);
        }
        5 => {
            // Dream
            p.glow_intensity *= 1.65;
            p.glow_radius *= 1.35;
            p.glow_color = [1.0, 0.55, 0.82];
            p.bloom_strength *= 1.4;
            p.bloom_radius *= 1.5;
            p.ghost_intensity *= 0.72;
            p.ocular_veil *= 1.25;
        }
        _ => {}
    }
}

fn apply_legacy_preset(p: &mut EffectParams) {
    match p.style_preset {
        7 => {
            p.global_brightness = 1.2;
            p.glow_intensity = 0.72;
            p.streak_intensity = 0.45;
            p.stripe_intensity = 1.65;
            p.stripe_length = 720.0;
            p.stripe_width = 2.2;
            p.stripe_color = [0.18, 0.48, 1.0];
            p.ghost_intensity = 0.8;
            p.bloom_strength = 1.8;
            p.chromatic_amount = 0.72;
        }
        8 => {
            p.global_brightness = 1.12;
            p.glow_intensity = 1.2;
            p.glow_color = [1.0, 0.48, 0.12];
            p.streak_intensity = 0.32;
            p.ring_intensity = 0.62;
            p.ghost_intensity = 1.25;
            p.bloom_strength = 2.8;
            p.bloom_chromatic = true;
        }
        9 => {
            p.global_brightness = 1.25;
            p.ring_intensity = 1.15;
            p.ring_spectrum = true;
            p.ring_chromatic = 1.0;
            p.ghost_intensity = 1.35;
            p.physical_gain = 3500.0;
            p.bloom_strength = 2.1;
            p.chromatic_amount = 1.0;
            p.starburst_intensity = 0.8;
            p.starburst_blades = 8;
        }
        10 => {
            p.global_brightness = 0.92;
            p.glow_intensity = 1.35;
            p.glow_color = [1.0, 0.56, 0.25];
            p.ghost_intensity = 1.7;
            p.physical_gain = 4500.0;
            p.physical_ghost_blur = 0.009;
            p.bloom_strength = 3.2;
            p.atmosphere_amount = 0.18;
        }
        11 => {
            p.global_brightness = 0.88;
            p.hotspot_intensity = 0.8;
            p.glow_intensity = 1.8;
            p.glow_radius = 135.0;
            p.glow_color = [1.0, 0.55, 0.82];
            p.ghost_intensity = 0.58;
            p.bloom_strength = 4.2;
            p.bloom_radius = 0.035;
            p.bloom_octaves = 6;
        }
        12 => {
            p.global_brightness = 1.05;
            p.ocular_enabled = true;
            p.ocular_pupil_mm = 7.2;
            p.ocular_corona = 1.25;
            p.ocular_halo = 0.38;
            p.ocular_veil = 0.42;
            p.ocular_tear = 0.32;
            p.ocular_squint = 0.55;
            p.ghost_intensity = 0.4;
            p.bloom_strength = 2.0;
        }
        13 => {
            p.global_brightness = 1.65;
            p.hotspot_intensity = 2.4;
            p.glow_intensity = 1.65;
            p.glow_radius = 190.0;
            p.streak_intensity = 1.1;
            p.starburst_intensity = 1.45;
            p.starburst_blades = 12;
            p.ghost_intensity = 1.25;
            p.bloom_strength = 3.6;
        }
        _ => {}
    }
}

// ---- Pixel buffer helpers ----

fn layer_to_flat(layer: &ae::Layer) -> (Vec<u8>, usize, usize) {
    let w = layer.width() as usize;
    let h = layer.height() as usize;
    let stride = layer.buffer_stride();
    let buf = layer.buffer();
    let depth = layer.bit_depth();
    let bytes_per_channel = match depth {
        16 => 2,
        32 => 4,
        _ => 1,
    };
    let bytes_per_pixel = bytes_per_channel * 4;
    let mut flat = vec![0u8; w * h * 4];
    for y in 0..h {
        let src_off = y * stride;
        let dst_off = y * w * 4;
        if src_off + w * bytes_per_pixel > buf.len() {
            continue;
        }
        for x in 0..w {
            let s = src_off + x * bytes_per_pixel;
            let d = dst_off + x * 4;
            for c in 0..4 {
                flat[d + c] = match depth {
                    // AE's PF_Pixel16 is nominally 16bpc but uses 0..32768.
                    16 => {
                        let o = s + c * 2;
                        let v = u16::from_ne_bytes([buf[o], buf[o + 1]]) as f32;
                        (v * (255.0 / 32768.0)).clamp(0.0, 255.0) as u8
                    }
                    32 => {
                        let o = s + c * 4;
                        let v = f32::from_ne_bytes([buf[o], buf[o + 1], buf[o + 2], buf[o + 3]]);
                        (v * 255.0).clamp(0.0, 255.0) as u8
                    }
                    _ => buf[s + c],
                };
            }
        }
    }
    (flat, w, h)
}

fn flat_to_layer(flat: &[u8], layer: &mut ae::Layer, w: usize, h: usize) {
    let stride = layer.buffer_stride();
    let depth = layer.bit_depth();
    let bytes_per_channel = match depth {
        16 => 2,
        32 => 4,
        _ => 1,
    };
    let bytes_per_pixel = bytes_per_channel * 4;
    let buf = layer.buffer_mut();
    for y in 0..h {
        let src_off = y * w * 4;
        let dst_off = y * stride;
        if src_off + w * 4 > flat.len() || dst_off + w * bytes_per_pixel > buf.len() {
            continue;
        }
        for x in 0..w {
            let s = src_off + x * 4;
            let d = dst_off + x * bytes_per_pixel;
            for c in 0..4 {
                match depth {
                    16 => {
                        let v = ((flat[s + c] as f32 / 255.0) * 32768.0)
                            .round()
                            .clamp(0.0, 32768.0) as u16;
                        let bytes = v.to_ne_bytes();
                        let o = d + c * 2;
                        buf[o] = bytes[0];
                        buf[o + 1] = bytes[1];
                    }
                    32 => {
                        let bytes = (flat[s + c] as f32 / 255.0).to_ne_bytes();
                        let o = d + c * 4;
                        buf[o..o + 4].copy_from_slice(&bytes);
                    }
                    _ => buf[d + c] = flat[s + c],
                }
            }
        }
    }
}

fn render_flare(ep: &EffectParams, original: &[u8], w: usize, h: usize) -> Vec<u8> {
    if let Some(gpu) = get_gpu() {
        gpu.process(ep, original, w, h)
    } else {
        flare::process(ep, original, w, h)
    }
}
