use after_effects as ae;

mod gpu;
mod refract;
mod pixels;

// ---- Parameter IDs ----

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    IorMode,
    BaseIor,
    IorR,
    IorG,
    IorB,
    RefractPower,
    EdgeMode,
    ChromaticAb,
    PerAxisChroma,
    ChromaticAbX,
    ChromaticAbY,
    Samples,
    FresnelPower,
    Shininess,
    Diffuseness,
    LightAngleX,
    LightAngleY,
    Saturation,
    HeightStrength,
    HeightBlur,
    EdgeBlur,
    Use6ch,
    MaskLayer,
    MapBlur,
    HeightSource,
    HeightInvert,
    CoverageSource,
    GlassEdgeEnable,
    EdgeWidth,
    EdgeRimBlur,
    BevelMode,
    BevelHeight,
    RimHighlight,
    RimFrost,
    BgLayer,
    BgBlur,
    Mix,
    UseGpu,
    Brightness,
    Contrast,
    AffectedBlur,
    OutputMode,
    Dispersion,

    // Collapsible UI group markers (no stored value; just layout).
    GroupRefractionStart,
    GroupRefractionEnd,
    GroupColorStart,
    GroupColorEnd,
    GroupShapeStart,
    GroupShapeEnd,
    GroupGlassStart,
    GroupGlassEnd,
    GroupLightStart,
    GroupLightEnd,
    GroupLookStart,
    GroupLookEnd,
    GroupCompositeStart,
    GroupCompositeEnd,
    GroupOutputStart,
    GroupOutputEnd,
}

// ---- Plugin global state ----

#[derive(Default)]
struct Plugin;

ae::define_effect!(Plugin, (), Params);

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _in_data: ae::InData,
        _out_data: ae::OutData,
    ) -> Result<(), ae::Error> {
        // Parameters are organized into collapsible groups for readability.
        // Lookups in get_params() are by enum key, so order is purely cosmetic;
        // the SmartFX layer-param positions are resolved at runtime via
        // `params.index(..)`, so reordering here is safe.

        // ---- Refraction ----
        params.add_group(
            Params::GroupRefractionStart,
            Params::GroupRefractionEnd,
            "Refraction",
            false,
            |p| {
                p.add(
                    Params::IorMode,
                    "IOR Mode",
                    ae::PopupDef::setup(|f| {
                        f.set_options(&["RGB Split", "Base IOR"]);
                        f.set_default(1);
                    }),
                )?;
                p.add(
                    Params::BaseIor,
                    "Base IOR",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(1.0);
                        f.set_valid_max(3.0);
                        f.set_slider_min(1.0);
                        f.set_slider_max(2.0);
                        f.set_default(1.18);
                        f.set_precision(3);
                    }),
                )?;
                p.add(
                    Params::RefractPower,
                    "Refract Power",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(50000.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(10000.0);
                        f.set_default(1000.0);
                        f.set_precision(1);
                    }),
                )?;
                p.add(
                    Params::Samples,
                    "Samples",
                    ae::SliderDef::setup(|f| {
                        f.set_valid_min(1);
                        f.set_valid_max(64);
                        f.set_slider_min(1);
                        f.set_slider_max(32);
                        f.set_default(16);
                    }),
                )?;
                p.add(
                    Params::EdgeMode,
                    "Edge Mode",
                    ae::PopupDef::setup(|f| {
                        f.set_options(&["Mirror", "Clamp", "Clamp + Fade"]);
                        f.set_default(1);
                    }),
                )?;
                Ok(())
            },
        )?;

        // ---- Dispersion (Color) ----
        params.add_group(
            Params::GroupColorStart,
            Params::GroupColorEnd,
            "Dispersion (Color)",
            false,
            |p| {
                // Master dispersion amount. Scales BOTH the per-sample chromatic
                // spread and the IOR R/G/B deviation, so 0% = no color dispersion
                // at all (kills any colored fringe / red fade) and 100% = authored.
                p.add(
                    Params::Dispersion,
                    "Dispersion",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(1000.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(500.0);
                        f.set_default(100.0);
                        f.set_precision(1);
                    }),
                )?;
                p.add(
                    Params::IorR,
                    "IOR Red",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(1.0);
                        f.set_valid_max(3.0);
                        f.set_slider_min(1.0);
                        f.set_slider_max(2.0);
                        f.set_default(1.15);
                        f.set_precision(3);
                    }),
                )?;
                p.add(
                    Params::IorG,
                    "IOR Green",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(1.0);
                        f.set_valid_max(3.0);
                        f.set_slider_min(1.0);
                        f.set_slider_max(2.0);
                        f.set_default(1.18);
                        f.set_precision(3);
                    }),
                )?;
                p.add(
                    Params::IorB,
                    "IOR Blue",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(1.0);
                        f.set_valid_max(3.0);
                        f.set_slider_min(1.0);
                        f.set_slider_max(2.0);
                        f.set_default(1.22);
                        f.set_precision(3);
                    }),
                )?;
                p.add(
                    Params::ChromaticAb,
                    "Chromatic Aberration",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(10.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(3.0);
                        f.set_default(0.6);
                        f.set_precision(3);
                    }),
                )?;
                p.add(
                    Params::PerAxisChroma,
                    "Per-axis Chroma",
                    ae::CheckBoxDef::setup(|f| {
                        f.set_default(false);
                        f.set_label("Enable X/Y split");
                    }),
                )?;
                p.add(
                    Params::ChromaticAbX,
                    "Chromatic Aberration X",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(10.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(3.0);
                        f.set_default(0.6);
                        f.set_precision(3);
                    }),
                )?;
                p.add(
                    Params::ChromaticAbY,
                    "Chromatic Aberration Y",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(10.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(3.0);
                        f.set_default(0.6);
                        f.set_precision(3);
                    }),
                )?;
                p.add(
                    Params::Use6ch,
                    "Use 6ch Dispersion (rygcbv)",
                    ae::CheckBoxDef::setup(|f| {
                        f.set_default(false);
                        f.set_label("Enable");
                    }),
                )?;
                Ok(())
            },
        )?;

        // ---- Shape (Height) ----
        params.add_group(
            Params::GroupShapeStart,
            Params::GroupShapeEnd,
            "Shape (Height)",
            false,
            |p| {
                p.add(
                    Params::MaskLayer,
                    "Mask (shape)",
                    ae::LayerDef::setup(|_f| {}),
                )?;
                p.add(
                    Params::MapBlur,
                    "Map Blur",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(1000.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(100.0);
                        f.set_default(0.0);
                        f.set_precision(1);
                    }),
                )?;
                p.add(
                    Params::HeightSource,
                    "Height Source",
                    ae::PopupDef::setup(|f| {
                        f.set_options(&[
                            "Luminance x Alpha",
                            "Luminance",
                            "Alpha",
                            "Red",
                            "Green",
                            "Blue",
                            "Max(RGB)",
                        ]);
                        f.set_default(1);
                    }),
                )?;
                p.add(
                    Params::HeightInvert,
                    "Invert Height",
                    ae::CheckBoxDef::setup(|f| {
                        f.set_default(false);
                        f.set_label("Enable");
                    }),
                )?;
                p.add(
                    Params::CoverageSource,
                    "Coverage Source",
                    ae::PopupDef::setup(|f| {
                        f.set_options(&["Alpha", "Luminance", "Full", "Same as Height"]);
                        f.set_default(1);
                    }),
                )?;
                p.add(
                    Params::HeightStrength,
                    "Height Strength",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(1000.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(100.0);
                        f.set_default(1.0);
                        f.set_precision(2);
                    }),
                )?;
                p.add(
                    Params::HeightBlur,
                    "Height Blur",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(5000.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(1000.0);
                        f.set_default(8.0);
                        f.set_precision(1);
                    }),
                )?;
                p.add(
                    Params::EdgeBlur,
                    "Edge Blur",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(500.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(100.0);
                        f.set_default(0.0);
                        f.set_precision(1);
                    }),
                )?;
                Ok(())
            },
        )?;

        // ---- Glass Edge (glassmorphism rim from an inner distance band) ----
        params.add_group(
            Params::GroupGlassStart,
            Params::GroupGlassEnd,
            "Glass Edge",
            true,
            |p| {
                p.add(
                    Params::GlassEdgeEnable,
                    "Glass Edge",
                    ae::CheckBoxDef::setup(|f| {
                        f.set_default(false);
                        f.set_label("Enable");
                    }),
                )?;
                p.add(
                    Params::EdgeWidth,
                    "Edge Width",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(500.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(100.0);
                        f.set_default(12.0);
                        f.set_precision(1);
                    }),
                )?;
                p.add(
                    Params::EdgeRimBlur,
                    "Edge Rim Blur",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(500.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(100.0);
                        f.set_default(8.0);
                        f.set_precision(1);
                    }),
                )?;
                p.add(
                    Params::BevelMode,
                    "Bevel Mode",
                    ae::PopupDef::setup(|f| {
                        f.set_options(&["Height Add", "SDF Normal"]);
                        f.set_default(1);
                    }),
                )?;
                p.add(
                    Params::BevelHeight,
                    "Bevel Height",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(-50.0);
                        f.set_valid_max(50.0);
                        f.set_slider_min(-10.0);
                        f.set_slider_max(10.0);
                        f.set_default(2.0);
                        f.set_precision(2);
                    }),
                )?;
                p.add(
                    Params::RimHighlight,
                    "Rim Highlight",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(400.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(200.0);
                        f.set_default(0.0);
                        f.set_precision(1);
                    }),
                )?;
                p.add(
                    Params::RimFrost,
                    "Rim Frost",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(500.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(100.0);
                        f.set_default(0.0);
                        f.set_precision(1);
                    }),
                )?;
                Ok(())
            },
        )?;

        // ---- Lighting ----
        params.add_group(
            Params::GroupLightStart,
            Params::GroupLightEnd,
            "Lighting",
            false,
            |p| {
                p.add(
                    Params::LightAngleX,
                    "Light Angle X",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(-180.0);
                        f.set_valid_max(180.0);
                        f.set_slider_min(-180.0);
                        f.set_slider_max(180.0);
                        f.set_default(-45.0);
                        f.set_precision(1);
                    }),
                )?;
                p.add(
                    Params::LightAngleY,
                    "Light Angle Y",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(-90.0);
                        f.set_valid_max(90.0);
                        f.set_slider_min(-90.0);
                        f.set_slider_max(90.0);
                        f.set_default(45.0);
                        f.set_precision(1);
                    }),
                )?;
                p.add(
                    Params::FresnelPower,
                    "Fresnel Power",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(10.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(5.0);
                        f.set_default(2.0);
                        f.set_precision(2);
                    }),
                )?;
                p.add(
                    Params::Shininess,
                    "Shininess",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(1.0);
                        f.set_valid_max(200.0);
                        f.set_slider_min(1.0);
                        f.set_slider_max(100.0);
                        f.set_default(40.0);
                        f.set_precision(1);
                    }),
                )?;
                p.add(
                    Params::Diffuseness,
                    "Diffuseness",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(2.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(1.0);
                        f.set_default(0.08);
                        f.set_precision(2);
                    }),
                )?;
                Ok(())
            },
        )?;

        // ---- Look ----
        params.add_group(
            Params::GroupLookStart,
            Params::GroupLookEnd,
            "Look",
            false,
            |p| {
                p.add(
                    Params::Saturation,
                    "Saturation",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(3.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(2.0);
                        f.set_default(1.0);
                        f.set_precision(2);
                    }),
                )?;
                p.add(
                    Params::Brightness,
                    "Affected Brightness",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(-200.0);
                        f.set_valid_max(200.0);
                        f.set_slider_min(-100.0);
                        f.set_slider_max(100.0);
                        f.set_default(0.0);
                        f.set_precision(1);
                    }),
                )?;
                p.add(
                    Params::Contrast,
                    "Affected Contrast",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(-100.0);
                        f.set_valid_max(500.0);
                        f.set_slider_min(-100.0);
                        f.set_slider_max(200.0);
                        f.set_default(0.0);
                        f.set_precision(1);
                    }),
                )?;
                p.add(
                    Params::AffectedBlur,
                    "Affected Box Blur",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(500.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(100.0);
                        f.set_default(0.0);
                        f.set_precision(1);
                    }),
                )?;
                Ok(())
            },
        )?;

        // ---- Composite ----
        params.add_group(
            Params::GroupCompositeStart,
            Params::GroupCompositeEnd,
            "Composite",
            false,
            |p| {
                p.add(Params::BgLayer, "Background", ae::LayerDef::setup(|_f| {}))?;
                p.add(
                    Params::BgBlur,
                    "Background Blur",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(1000.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(100.0);
                        f.set_default(0.0);
                        f.set_precision(1);
                    }),
                )?;
                p.add(
                    Params::Mix,
                    "Mix with Original",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(100.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(100.0);
                        f.set_default(100.0);
                        f.set_precision(1);
                    }),
                )?;
                Ok(())
            },
        )?;

        // ---- Output / System ----
        params.add_group(
            Params::GroupOutputStart,
            Params::GroupOutputEnd,
            "Output / System",
            false,
            |p| {
                p.add(
                    Params::OutputMode,
                    "Output Mode",
                    ae::PopupDef::setup(|f| {
                        f.set_options(&[
                            "Output",
                            "Input",
                            "Mask Map",
                            "Delta Map",
                            "Debug Regions",
                        ]);
                        f.set_default(1);
                    }),
                )?;
                p.add(
                    Params::UseGpu,
                    "Use GPU",
                    ae::CheckBoxDef::setup(|f| {
                        f.set_default(false);
                        f.set_label("Auto (wgpu)");
                    }),
                )?;
                Ok(())
            },
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
                    "RefractionDispersion v1.11\rPhysically-inspired refraction\rwith chromatic dispersion.\rPort of Maxime Heckel's shader.\rWritten in Rust.",
                );
            }

            ae::Command::Render {
                in_layer,
                mut out_layer,
            } => {
                render_cpu(params, &in_data, &in_layer, &mut out_layer)?;
            }

            ae::Command::SmartPreRender { mut extra } => {
                smart_pre_render(&in_data, &mut extra, params)?;
            }

            ae::Command::SmartRender { extra } => {
                smart_render_cpu(&extra, params)?;
            }

            ae::Command::SmartRenderGpu { extra } => {
                smart_render_cpu(&extra, params)?;
            }

            _ => {}
        }
        Ok(())
    }
}

// ---- Extract parameters ----

#[cfg_attr(test, derive(Default))]
pub struct RefractParams {
    pub ior_mode: i32,
    pub base_ior: f32,
    pub ior_r: f32,
    pub ior_g: f32,
    pub ior_b: f32,
    pub refract_power: f32,
    pub chromatic_ab: f32,
    pub use_per_axis_chroma: bool,
    pub chromatic_ab_x: f32,
    pub chromatic_ab_y: f32,
    pub samples: usize,
    pub fresnel_power: f32,
    pub shininess: f32,
    pub diffuseness: f32,
    pub light_angle_x: f32,
    pub light_angle_y: f32,
    pub saturation: f32,
    pub height_strength: f32,
    pub height_blur: f64,
    pub edge_blur: f64,
    pub use_6ch: bool,
    pub map_blur: f64,
    pub height_source: i32,
    pub height_invert: bool,
    pub coverage_source: i32,
    pub edge_enable: bool,
    pub edge_width: f32,
    pub edge_rim_blur: f64,
    pub bevel_mode: i32,
    pub bevel_height: f32,
    pub rim_highlight: f32,
    pub rim_frost: f64,
    pub bg_blur: f64,
    pub mix: f32,
    pub use_gpu: bool,
    pub edge_mode: i32,
    pub brightness: f32,
    pub contrast: f32,
    pub affected_blur: f64,
    pub output_mode: i32,
}

fn get_params(params: &ae::Parameters<Params>) -> Result<RefractParams, ae::Error> {
    // Master dispersion (0 = none, 1.0 = authored). Scales the IOR R/G/B
    // deviation from their mean AND the chromatic-aberration amounts, so the
    // user can dial the whole color split (and any red fringe) up or down live.
    let dispersion = (params.get(Params::Dispersion)?.as_float_slider()?.value() as f32 / 100.0)
        .clamp(0.0, 10.0);
    let ior_r_raw = params.get(Params::IorR)?.as_float_slider()?.value() as f32;
    let ior_g_raw = params.get(Params::IorG)?.as_float_slider()?.value() as f32;
    let ior_b_raw = params.get(Params::IorB)?.as_float_slider()?.value() as f32;
    let ior_avg = (ior_r_raw + ior_g_raw + ior_b_raw) / 3.0;
    let ior_r = ior_avg + (ior_r_raw - ior_avg) * dispersion;
    let ior_g = ior_avg + (ior_g_raw - ior_avg) * dispersion;
    let ior_b = ior_avg + (ior_b_raw - ior_avg) * dispersion;

    Ok(RefractParams {
        ior_mode: params.get(Params::IorMode)?.as_popup()?.value() as i32,
        base_ior: params.get(Params::BaseIor)?.as_float_slider()?.value() as f32,
        ior_r,
        ior_g,
        ior_b,
        refract_power: params.get(Params::RefractPower)?.as_float_slider()?.value() as f32,
        chromatic_ab: params.get(Params::ChromaticAb)?.as_float_slider()?.value() as f32
            * dispersion,
        use_per_axis_chroma: params.get(Params::PerAxisChroma)?.as_checkbox()?.value(),
        chromatic_ab_x: params.get(Params::ChromaticAbX)?.as_float_slider()?.value() as f32
            * dispersion,
        chromatic_ab_y: params.get(Params::ChromaticAbY)?.as_float_slider()?.value() as f32
            * dispersion,
        samples: params
            .get(Params::Samples)?
            .as_slider()?
            .value()
            .clamp(1, 64) as usize,
        fresnel_power: params.get(Params::FresnelPower)?.as_float_slider()?.value() as f32,
        shininess: params.get(Params::Shininess)?.as_float_slider()?.value() as f32,
        diffuseness: params.get(Params::Diffuseness)?.as_float_slider()?.value() as f32,
        light_angle_x: params.get(Params::LightAngleX)?.as_float_slider()?.value() as f32,
        light_angle_y: params.get(Params::LightAngleY)?.as_float_slider()?.value() as f32,
        saturation: params.get(Params::Saturation)?.as_float_slider()?.value() as f32,
        height_strength: params
            .get(Params::HeightStrength)?
            .as_float_slider()?
            .value() as f32,
        height_blur: params
            .get(Params::HeightBlur)?
            .as_float_slider()?
            .value()
            .max(0.0),
        edge_blur: params
            .get(Params::EdgeBlur)?
            .as_float_slider()?
            .value()
            .max(0.0),
        use_6ch: params.get(Params::Use6ch)?.as_checkbox()?.value(),
        map_blur: params
            .get(Params::MapBlur)?
            .as_float_slider()?
            .value()
            .max(0.0),
        height_source: (params.get(Params::HeightSource)?.as_popup()?.value() as i32).clamp(1, 7),
        height_invert: params.get(Params::HeightInvert)?.as_checkbox()?.value(),
        coverage_source: (params.get(Params::CoverageSource)?.as_popup()?.value() as i32)
            .clamp(1, 4),
        edge_enable: params.get(Params::GlassEdgeEnable)?.as_checkbox()?.value(),
        edge_width: params.get(Params::EdgeWidth)?.as_float_slider()?.value() as f32,
        edge_rim_blur: params
            .get(Params::EdgeRimBlur)?
            .as_float_slider()?
            .value()
            .max(0.0),
        bevel_mode: (params.get(Params::BevelMode)?.as_popup()?.value() as i32).clamp(1, 2),
        bevel_height: params.get(Params::BevelHeight)?.as_float_slider()?.value() as f32,
        rim_highlight: params.get(Params::RimHighlight)?.as_float_slider()?.value() as f32,
        rim_frost: params
            .get(Params::RimFrost)?
            .as_float_slider()?
            .value()
            .max(0.0),
        bg_blur: params
            .get(Params::BgBlur)?
            .as_float_slider()?
            .value()
            .max(0.0),
        mix: (params
            .get(Params::Mix)?
            .as_float_slider()?
            .value()
            .clamp(0.0, 100.0)
            / 100.0) as f32,
        use_gpu: params.get(Params::UseGpu)?.as_checkbox()?.value(),
        edge_mode: (params.get(Params::EdgeMode)?.as_popup()?.value() as i32).clamp(1, 3),
        brightness: (params
            .get(Params::Brightness)?
            .as_float_slider()?
            .value()
            .clamp(-200.0, 200.0)
            / 100.0) as f32,
        contrast: (params
            .get(Params::Contrast)?
            .as_float_slider()?
            .value()
            .clamp(-100.0, 500.0)
            / 100.0) as f32,
        affected_blur: params
            .get(Params::AffectedBlur)?
            .as_float_slider()?
            .value()
            .max(0.0),
        output_mode: params.get(Params::OutputMode)?.as_popup()?.value() as i32,
    })
}

// ---- Native-depth layer codecs ----

fn read_aux<T: pixels::Channel + refract::Channel>(layer: &ae::Layer) -> Result<(Vec<T>,usize,usize),ae::Error> {
    if pixels::channel_bytes(layer)? == T::BYTES { return pixels::read::<T>(layer); }
    fn convert<S: pixels::Channel,T: refract::Channel>(layer: &ae::Layer) -> Result<(Vec<T>,usize,usize),ae::Error> {
        let (src,w,h)=pixels::read::<S>(layer)?;
        let mut converted = Vec::new();
        converted.try_reserve_exact(src.len()).map_err(|_| ae::Error::OutOfMemory)?;
        converted.extend(src.into_iter().map(|v| T::from_units((v.normalized()*255.0) as f32)));
        Ok((converted,w,h))
    }
    match pixels::channel_bytes(layer)? {
        1=>convert::<u8,T>(layer), 2=>convert::<u16,T>(layer), 4=>convert::<f32,T>(layer),
        _=>Err(ae::Error::BadCallbackParameter),
    }
}

/// Place a referenced layer into a buffer matching the input layer's size,
/// WITHOUT stretching. The layer's pixels keep their 1:1 resolution and are
/// blitted at (off_x, off_y) in the destination. This preserves the shape's
/// comp-space position so that moving a shape layer moves its refraction.
fn fit_layer_at_offset<T: Copy + Default>(
    src: &[T],
    sw: usize,
    sh: usize,
    dw: usize,
    dh: usize,
    off_x: isize,
    off_y: isize,
) -> Result<Vec<T>, ae::Error> {
    if sw.checked_mul(sh).and_then(|n|n.checked_mul(4)) != Some(src.len()) {
        return Err(ae::Error::BadCallbackParameter);
    }
    let mut dst = empty_image::<T>(dw,dh)?;
    if sw == 0 || sh == 0 || dw == 0 || dh == 0 {
        return Ok(dst);
    }
    if off_x == 0 && off_y == 0 && sw == dw && sh == dh {
        dst.copy_from_slice(src);
        return Ok(dst);
    }
    for sy in 0..sh {
        let dy = sy as i128 + off_y as i128;
        if dy < 0 || dy >= dh as i128 {
            continue;
        }
        let sx_start = (-(off_x as i128)).max(0) as usize;
        let sx_end = ((dw as i128 - off_x as i128).min(sw as i128)).max(0) as usize;
        if sx_start >= sx_end {
            continue;
        }
        let dx_start = (sx_start as i128 + off_x as i128) as usize;
        let row_len = (sx_end - sx_start) * 4;
        let si = (sy * sw + sx_start) * 4;
        let di = (dy as usize * dw + dx_start) * 4;
        dst[di..di + row_len].copy_from_slice(&src[si..si + row_len]);
    }
    Ok(dst)
}

fn empty_image<T: Copy + Default>(w: usize,h: usize) -> Result<Vec<T>,ae::Error> {
    let count = w.checked_mul(h).and_then(|n|n.checked_mul(4)).ok_or(ae::Error::OutOfMemory)?;
    let mut pixels = Vec::new();
    pixels.try_reserve_exact(count).map_err(|_|ae::Error::OutOfMemory)?;
    pixels.resize(count,T::default());
    Ok(pixels)
}

fn finish_auxiliary<T>(decoded: Result<T,ae::Error>, checkin: Result<(),ae::Error>) -> Result<T,ae::Error> {
    let pixels = decoded?;
    checkin?;
    Ok(pixels)
}

// Preserve the original failure while always returning a successfully checked-out
// world. Resume panics only after cleanup, for the plug-in's outer panic boundary.
fn with_pixel_checkin<T>(work: impl FnOnce() -> Result<T,ae::Error>,
    checkin: impl FnOnce() -> Result<(),ae::Error>) -> Result<T,ae::Error> {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work));
    let cleanup = checkin();
    match result {
        Ok(decoded) => finish_auxiliary(decoded,cleanup),
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

#[test]
fn checked_out_pixels_are_returned_on_error_and_unwind() {
    use std::cell::Cell;
    for work_failure in [false,true] {
        for cleanup_failure in [false,true] {
            let calls=Cell::new(0);
            let result=with_pixel_checkin(
                || if work_failure {Err(ae::Error::OutOfMemory)} else {Ok(7)},
                || {calls.set(calls.get()+1); if cleanup_failure {Err(ae::Error::BadCallbackParameter)} else {Ok(())}});
            assert_eq!(calls.get(),1);
            assert_eq!(result,if work_failure {Err(ae::Error::OutOfMemory)} else if cleanup_failure {Err(ae::Error::BadCallbackParameter)} else {Ok(7)});
        }
    }
    for cleanup_failure in [false,true] {
        let calls=Cell::new(0);
        let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            with_pixel_checkin::<()>(|| panic!("injected pixel failure"),
                || {calls.set(calls.get()+1); if cleanup_failure {Err(ae::Error::BadCallbackParameter)} else {Ok(())}})
        }));
        assert_eq!(calls.get(),1);
        assert_eq!(result.unwrap_err().downcast_ref::<&str>(),Some(&"injected pixel failure"));
    }
    let order=std::cell::RefCell::new(Vec::new());
    let _=std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        with_pixel_checkin::<()>(|| with_pixel_checkin(|| panic!("nested failure"),
            || {order.borrow_mut().push("auxiliary"); Ok(())}),
            || {order.borrow_mut().push("input"); Ok(())})
    }));
    assert_eq!(*order.borrow(),["auxiliary","input"]);
}

#[test]
fn auxiliary_errors_and_invalid_geometry_are_not_missing_layers() {
    assert_eq!(finish_auxiliary::<()>(Err(ae::Error::OutOfMemory),Err(ae::Error::BadCallbackParameter)),Err(ae::Error::OutOfMemory));
    assert_eq!(finish_auxiliary(Ok(17),Err(ae::Error::BadCallbackParameter)),Err(ae::Error::BadCallbackParameter));
    assert_eq!(finish_auxiliary(Ok(17),Ok(())),Ok(17));
    assert_eq!(fit_layer_at_offset(&[0u16;3],1,1,1,1,0,0),Err(ae::Error::BadCallbackParameter));
    assert_eq!(fit_layer_at_offset::<u8>(&[],usize::MAX,2,1,1,0,0),Err(ae::Error::BadCallbackParameter));
    assert_eq!(empty_image::<f32>(usize::MAX,2),Err(ae::Error::OutOfMemory));
    assert_eq!(empty_image::<f32>(usize::MAX/4,1),Err(ae::Error::OutOfMemory));
    for offset in [isize::MIN,isize::MAX] {
        assert_eq!(fit_layer_at_offset(&[255u8;8],1,2,1,1,offset,0).unwrap(),[0;4]);
        assert_eq!(fit_layer_at_offset(&[255u8;8],1,2,1,1,0,offset).unwrap(),[0;4]);
    }
}

// ---- Legacy Render path ----

fn render_cpu(params: &ae::Parameters<Params>, in_data: &ae::InData, input: &ae::Layer, output: &mut ae::Layer) -> Result<(),ae::Error> {
    match pixels::channel_bytes(input)? {
        1=>render_typed::<u8>(params,in_data,input,output),
        2=>render_typed::<u16>(params,in_data,input,output),
        4=>render_typed::<f32>(params,in_data,input,output),
        _=>Err(ae::Error::BadCallbackParameter),
    }
}

fn smart_render_cpu(extra: &ae::pf::SmartRenderExtra, params: &ae::Parameters<Params>) -> Result<(),ae::Error> {
    let cb=extra.callbacks();
    let input=cb.checkout_layer_pixels(0)?.ok_or(ae::Error::Generic)?;
    with_pixel_checkin(|| {
        let mut output=cb.checkout_output()?.ok_or(ae::Error::Generic)?;
        match pixels::channel_bytes(&input)? {
            1=>smart_render_typed::<u8>(extra,params,&input,&mut output),
            2=>smart_render_typed::<u16>(extra,params,&input,&mut output),
            4=>smart_render_typed::<f32>(extra,params,&input,&mut output),
            _=>Err(ae::Error::BadCallbackParameter),
        }
    }, || cb.checkin_layer_pixels(0))
}


fn render_typed<T: pixels::Channel + refract::Channel>(
    params: &ae::Parameters<Params>,
    in_data: &ae::InData,
    in_layer: &ae::Layer,
    out_layer: &mut ae::Layer,
) -> Result<(), ae::Error> {
    let rp = get_params(params)?;
    let (src, w, h) = pixels::read::<T>(in_layer)?;
    let in_org = in_layer.origin();
    let (in_h, in_v) = (in_org.h as isize, in_org.v as isize);
    let mask = get_layer_flat(params, Params::MaskLayer, in_data, w, h, in_h, in_v)?;
    let bg = get_layer_flat(params, Params::BgLayer, in_data, w, h, in_h, in_v)?;
    let result = dispatch_render(&rp, &src, mask.as_deref(), bg.as_deref(), w, h)?;
    pixels::write(&result, out_layer, w, h)
}

/// Choose GPU or CPU backend based on the `use_gpu` param and wgpu availability.
/// GPU failures fall back to CPU so an unavailable GPU environment never
/// breaks rendering.
fn dispatch_render<T: refract::Channel>(
    rp: &RefractParams,
    src: &[T],
    mask: Option<&[T]>,
    bg: Option<&[T]>,
    w: usize,
    h: usize,
) -> Result<Vec<T>,ae::Error> {
    // The Glass Edge distance transform, the Background Blur and the Map Blur are
    // CPU-only for now; fall back to the CPU renderer when any is active so GPU
    // and CPU stay visually consistent.
    if rp.use_gpu
        && !rp.edge_enable
        && rp.bg_blur < 0.5
        && rp.map_blur < 0.5
        && rp.output_mode != 5
        && gpu::available()
    {
        if let Some(result) = gpu::render(rp, src, mask, bg, w, h) {
            return Ok(result);
        }
    }
    refract::render(rp, src, mask, bg, w, h)
}

fn get_layer_flat<T: pixels::Channel + refract::Channel>(
    params: &ae::Parameters<Params>,
    param_id: Params,
    in_data: &ae::InData,
    w: usize,
    h: usize,
    in_h: isize,
    in_v: isize,
) -> Result<Option<Vec<T>>,ae::Error> {
    let checkout = params
        .checkout_at(param_id, Some(in_data.current_time()), None, None)
        ?;
    let layer_def = checkout.as_layer()?;
    let Some(layer) = layer_def.value() else {return Ok(None)};
    let (flat, lw, lh) = read_aux::<T>(&layer)?;
    let origin = layer.origin();
    Ok(Some(fit_layer_at_offset(
        &flat,
        lw,
        lh,
        w,
        h,
        origin.h as isize - in_h,
        origin.v as isize - in_v,
    )?))
}

// ---- SmartFX PreRender ----
//
// The mask / background layer params must be checked out by their POSITIONAL
// index (0 = input, then params in add-order, including group start/end
// markers). Instead of hardcoding those positions (which breaks whenever the
// UI is reordered or grouped), we resolve them at runtime from the param map
// via `params.index(..)`. `*_CHECKOUT_ID` are just arbitrary handles we pick to
// retrieve the checked-out pixels again in SmartRender.
const MASK_CHECKOUT_ID: i32 = 1;
const BG_CHECKOUT_ID: i32 = 2;

#[derive(Clone, Copy, Default)]
struct AuxiliaryPixels {
    mask: bool,
    background: bool,
}

fn has_pixels(rect: ae::Rect) -> bool {
    rect.right > rect.left && rect.bottom > rect.top
}

#[test]
fn native_auxiliary_offsets_keep_low_bits_and_hdr() {
    let src = [32768u16,16385,16387,1,32768,3,5,7];
    let out = fit_layer_at_offset(&src,2,1,3,2,1,1).unwrap();
    assert_eq!(&out[16..24],&src);
    assert!(out[..16].iter().all(|&v|v==0));
    let hdr = [1.0f32,-0.25,4.125,0.50001,1.0,2.0,3.0,4.0];
    let clipped = fit_layer_at_offset(&hdr,2,1,1,1,-1,0).unwrap();
    assert_eq!(clipped,&hdr[4..]);
}

#[test]
fn auxiliary_offsets_match_destination_sampling_reference() {
    fn compare<T: Copy + Default + PartialEq + std::fmt::Debug>(source: &[T]) {
        let (sw,sh,dw,dh)=(7usize,5usize,9usize,7usize);
        for ox in [-10isize,-7,-2,0,2,8,10] {
            for oy in [-8isize,-5,-1,0,2,6,8] {
                let actual=fit_layer_at_offset(source,sw,sh,dw,dh,ox,oy).unwrap();
                // Independent destination-driven reference, not source-row clipping.
                for y in 0..dh {
                    for x in 0..dw {
                        let (sx,sy)=(x as isize-ox,y as isize-oy);
                        for c in 0..4 {
                            let expected=if sx>=0 && sy>=0 && sx<sw as isize && sy<sh as isize {
                                source[(sy as usize*sw+sx as usize)*4+c]
                            } else {T::default()};
                            assert_eq!(actual[(y*dw+x)*4+c],expected,"offset={ox},{oy} pixel={x},{y} channel={c}");
                        }
                    }
                }
            }
        }
    }
    let deep:Vec<u16>=(0..35).flat_map(|i|[32768,16385+i*2,16387+i*2,1+i*2]).collect();
    compare(&deep);
    let hdr:Vec<f32>=(0..35).flat_map(|i|[0.375,-0.25-i as f32/16.0,4.125+i as f32/16.0,0.50001]).collect();
    compare(&hdr);
}

#[test]
fn empty_auxiliary_rect_is_not_a_black_image() {
    assert!(!has_pixels(ae::Rect { left: 0, top: 0, right: 0, bottom: 0 }));
    assert!(!has_pixels(ae::Rect { left: 1, top: 2, right: 1, bottom: 5 }));
    assert!(has_pixels(ae::Rect { left: -3, top: -2, right: 2, bottom: 5 }));
}

/// Positional index of a layer param (for `checkout_layer`). Falls back to a
/// large unused index if the param is somehow missing, so a stray checkout
/// simply returns no layer rather than grabbing the wrong one.
fn layer_param_index(params: &ae::Parameters<Params>, id: Params) -> i32 {
    params.index(id).map(|i| i as i32).unwrap_or(-1)
}

fn smart_pre_render(
    in_data: &ae::InData,
    extra: &mut ae::pf::PreRenderExtra,
    params: &ae::Parameters<Params>,
) -> Result<(), ae::Error> {
    let req = extra.output_request();
    let cb = extra.callbacks();

    let _in_result = cb.checkout_layer(
        0,
        0,
        &req,
        in_data.current_time(),
        in_data.time_step(),
        in_data.time_scale(),
    )?;

    let req_rect: ae::Rect = req.rect.into();
    extra.set_result_rect(req_rect);
    extra.set_max_result_rect(req_rect);

    let mask_index = layer_param_index(params, Params::MaskLayer);
    let bg_index = layer_param_index(params, Params::BgLayer);

    let mut available = AuxiliaryPixels::default();
    if mask_index >= 0 {
        available.mask = cb.checkout_layer(
            mask_index,
            MASK_CHECKOUT_ID,
            &req,
            in_data.current_time(),
            in_data.time_step(),
            in_data.time_scale(),
        ).map(|r| has_pixels(r.result_rect.into())).unwrap_or(false);
    }

    if bg_index >= 0 {
        available.background = cb.checkout_layer(
            bg_index,
            BG_CHECKOUT_ID,
            &req,
            in_data.current_time(),
            in_data.time_step(),
            in_data.time_scale(),
        ).map(|r| has_pixels(r.result_rect.into())).unwrap_or(false);
    }

    extra.set_pre_render_data(available);
    Ok(())
}

// ---- SmartFX CPU Render ----

fn smart_render_typed<T: pixels::Channel + refract::Channel>(
    extra: &ae::pf::SmartRenderExtra,
    params: &ae::Parameters<Params>,
    input_world: &ae::Layer,
    output_world: &mut ae::Layer,
) -> Result<(), ae::Error> {
    let rp = get_params(params)?;
    let cb = extra.callbacks();
    let available = extra.pre_render_data::<AuxiliaryPixels>().copied().unwrap_or_default();


    let (src, w, h) = pixels::read::<T>(&input_world)?;
    let in_org = input_world.origin();
    let (in_h, in_v) = (in_org.h as isize, in_org.v as isize);

    // When a mask layer is ASSIGNED but lands fully outside the frame, the
    // SmartFX pixel checkout returns nothing. We must NOT fall back to the
    // source then (that would drive the shape from the whole input and apply the
    // effect everywhere). Substitute an empty mask so the effect simply has no
    // shape. An UNassigned mask still returns None so the renderer uses the
    // source as the shape (the documented "no mask" behavior).
    let mut mask = if available.mask {
        checkout_layer_flat(&cb, MASK_CHECKOUT_ID as u32, w, h, in_h, in_v)?
    } else { None };
    if mask.is_none() && layer_assigned(params, Params::MaskLayer)? {
        mask = Some(empty_image::<T>(w,h)?);
    }
    let bg = if available.background {
        checkout_layer_flat(&cb, BG_CHECKOUT_ID as u32, w, h, in_h, in_v)?
    } else { None };

    let result = dispatch_render(&rp, &src, mask.as_deref(), bg.as_deref(), w, h)?;
    pixels::write(&result, output_world, w, h)?;

    Ok(())
}

/// True if a layer is actually assigned to `id` (regardless of whether it
/// currently overlaps the frame). Used to tell "no mask layer selected" apart
/// from "mask layer selected but moved off-frame".
fn layer_assigned(params: &ae::Parameters<Params>, id: Params) -> Result<bool,ae::Error> {
    let checkout = params.checkout(id)?;
    let layer_def = checkout.as_layer()?;
    Ok(layer_def.value().is_some())
}

fn checkout_layer_flat<T: pixels::Channel + refract::Channel>(
    cb: &ae::pf::SmartRenderCallbacks,
    checkout_id: u32,
    w: usize,
    h: usize,
    in_h: isize,
    in_v: isize,
) -> Result<Option<Vec<T>>,ae::Error> {
    let Some(world) = cb.checkout_layer_pixels(checkout_id)? else {return Ok(None)};
    let ((flat,lw,lh),origin) = with_pixel_checkin(
        || Ok((read_aux::<T>(&world)?,world.origin())),
        || cb.checkin_layer_pixels(checkout_id))?;
    // Place relative to the INPUT world's origin. When AE renders a partial
    // region (ROI) — typically near the frame edges — the input world has a
    // non-zero origin; ignoring it makes the mask/bg jump frame-to-frame as the
    // requested region shifts. Subtracting it keeps placement stable.
    Ok(Some(fit_layer_at_offset(
        &flat,
        lw,
        lh,
        w,
        h,
        origin.h as isize - in_h,
        origin.v as isize - in_v,
    )?))
}
