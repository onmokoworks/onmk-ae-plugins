use after_effects as ae;

mod gpu;
mod refract;

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

// ---- Copy layer pixels to flat buffer (ARGB u8) ----

fn layer_to_flat(layer: &ae::Layer) -> (Vec<u8>, usize, usize) {
    let w = layer.width() as usize;
    let h = layer.height() as usize;
    let stride = layer.buffer_stride();
    let buf = layer.buffer();
    let mut flat = vec![0u8; w * h * 4];
    for y in 0..h {
        let src_off = y * stride;
        let dst_off = y * w * 4;
        let row_len = w * 4;
        if src_off + row_len <= buf.len() && dst_off + row_len <= flat.len() {
            flat[dst_off..dst_off + row_len].copy_from_slice(&buf[src_off..src_off + row_len]);
        }
    }
    (flat, w, h)
}

fn flat_to_layer(flat: &[u8], layer: &mut ae::Layer, w: usize, h: usize) {
    let stride = layer.buffer_stride();
    let buf = layer.buffer_mut();
    for y in 0..h {
        let src_off = y * w * 4;
        let dst_off = y * stride;
        let row_len = w * 4;
        if src_off + row_len <= flat.len() && dst_off + row_len <= buf.len() {
            buf[dst_off..dst_off + row_len].copy_from_slice(&flat[src_off..src_off + row_len]);
        }
    }
}

/// Place a referenced layer into a buffer matching the input layer's size,
/// WITHOUT stretching. The layer's pixels keep their 1:1 resolution and are
/// blitted at (off_x, off_y) in the destination. This preserves the shape's
/// comp-space position so that moving a shape layer moves its refraction.
fn fit_layer_at_offset(
    src: &[u8],
    sw: usize,
    sh: usize,
    dw: usize,
    dh: usize,
    off_x: isize,
    off_y: isize,
) -> Vec<u8> {
    let mut dst = vec![0u8; dw * dh * 4];
    if sw == 0 || sh == 0 || dw == 0 || dh == 0 {
        return dst;
    }
    if off_x == 0 && off_y == 0 && sw == dw && sh == dh {
        dst.copy_from_slice(src);
        return dst;
    }
    for sy in 0..sh {
        let dy = sy as isize + off_y;
        if dy < 0 || dy >= dh as isize {
            continue;
        }
        let sx_start = (-off_x).max(0) as usize;
        let sx_end = ((dw as isize - off_x).min(sw as isize)).max(0) as usize;
        if sx_start >= sx_end {
            continue;
        }
        let dx_start = (sx_start as isize + off_x) as usize;
        let row_len = (sx_end - sx_start) * 4;
        let si = (sy * sw + sx_start) * 4;
        let di = (dy as usize * dw + dx_start) * 4;
        dst[di..di + row_len].copy_from_slice(&src[si..si + row_len]);
    }
    dst
}

// ---- Legacy Render path ----

fn render_cpu(
    params: &ae::Parameters<Params>,
    in_data: &ae::InData,
    in_layer: &ae::Layer,
    out_layer: &mut ae::Layer,
) -> Result<(), ae::Error> {
    let rp = get_params(params)?;
    let (src, w, h) = layer_to_flat(in_layer);
    let in_org = in_layer.origin();
    let (in_h, in_v) = (in_org.h as isize, in_org.v as isize);
    let mask = get_layer_flat(params, Params::MaskLayer, in_data, w, h, in_h, in_v);
    let bg = get_layer_flat(params, Params::BgLayer, in_data, w, h, in_h, in_v);
    let result = dispatch_render(&rp, &src, mask.as_deref(), bg.as_deref(), w, h);
    flat_to_layer(&result, out_layer, w, h);
    Ok(())
}

/// Choose GPU or CPU backend based on the `use_gpu` param and CUDA availability.
/// GPU failures silently fall back to CPU so a broken CUDA environment never
/// breaks rendering.
fn dispatch_render(
    rp: &RefractParams,
    src: &[u8],
    mask: Option<&[u8]>,
    bg: Option<&[u8]>,
    w: usize,
    h: usize,
) -> Vec<u8> {
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
            return result;
        }
    }
    refract::render(rp, src, mask, bg, w, h)
}

fn get_layer_flat(
    params: &ae::Parameters<Params>,
    param_id: Params,
    in_data: &ae::InData,
    w: usize,
    h: usize,
    in_h: isize,
    in_v: isize,
) -> Option<Vec<u8>> {
    let checkout = params
        .checkout_at(param_id, Some(in_data.current_time()), None, None)
        .ok()?;
    let layer_def = checkout.as_layer().ok()?;
    let layer = layer_def.value()?;
    let (flat, lw, lh) = layer_to_flat(&layer);
    let origin = layer.origin();
    Some(fit_layer_at_offset(
        &flat,
        lw,
        lh,
        w,
        h,
        origin.h as isize - in_h,
        origin.v as isize - in_v,
    ))
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

    if mask_index >= 0 {
        let _ = cb.checkout_layer(
            mask_index,
            MASK_CHECKOUT_ID,
            &req,
            in_data.current_time(),
            in_data.time_step(),
            in_data.time_scale(),
        );
    }

    if bg_index >= 0 {
        let _ = cb.checkout_layer(
            bg_index,
            BG_CHECKOUT_ID,
            &req,
            in_data.current_time(),
            in_data.time_step(),
            in_data.time_scale(),
        );
    }

    Ok(())
}

// ---- SmartFX CPU Render ----

fn smart_render_cpu(
    extra: &ae::pf::SmartRenderExtra,
    params: &ae::Parameters<Params>,
) -> Result<(), ae::Error> {
    let rp = get_params(params)?;
    let cb = extra.callbacks();

    let input_world = cb.checkout_layer_pixels(0)?.ok_or(ae::Error::Generic)?;
    let mut output_world = cb.checkout_output()?.ok_or(ae::Error::Generic)?;

    let (src, w, h) = layer_to_flat(&input_world);
    let in_org = input_world.origin();
    let (in_h, in_v) = (in_org.h as isize, in_org.v as isize);

    // When a mask layer is ASSIGNED but lands fully outside the frame, the
    // SmartFX pixel checkout returns nothing. We must NOT fall back to the
    // source then (that would drive the shape from the whole input and apply the
    // effect everywhere). Substitute an empty mask so the effect simply has no
    // shape. An UNassigned mask still returns None so the renderer uses the
    // source as the shape (the documented "no mask" behavior).
    let mask = checkout_layer_flat(&cb, MASK_CHECKOUT_ID as u32, w, h, in_h, in_v).or_else(|| {
        if layer_assigned(params, Params::MaskLayer) {
            Some(vec![0u8; w * h * 4])
        } else {
            None
        }
    });
    let bg = checkout_layer_flat(&cb, BG_CHECKOUT_ID as u32, w, h, in_h, in_v);

    let result = dispatch_render(&rp, &src, mask.as_deref(), bg.as_deref(), w, h);
    flat_to_layer(&result, &mut output_world, w, h);

    cb.checkin_layer_pixels(0)?;
    let _ = cb.checkin_layer_pixels(MASK_CHECKOUT_ID as u32);
    let _ = cb.checkin_layer_pixels(BG_CHECKOUT_ID as u32);
    Ok(())
}

/// True if a layer is actually assigned to `id` (regardless of whether it
/// currently overlaps the frame). Used to tell "no mask layer selected" apart
/// from "mask layer selected but moved off-frame".
fn layer_assigned(params: &ae::Parameters<Params>, id: Params) -> bool {
    let checkout = match params.checkout(id) {
        Ok(c) => c,
        Err(_) => return false,
    };
    let layer_def = match checkout.as_layer() {
        Ok(l) => l,
        Err(_) => return false,
    };
    layer_def.value().is_some()
}

fn checkout_layer_flat(
    cb: &ae::pf::SmartRenderCallbacks,
    checkout_id: u32,
    w: usize,
    h: usize,
    in_h: isize,
    in_v: isize,
) -> Option<Vec<u8>> {
    let world = cb.checkout_layer_pixels(checkout_id).ok()??;
    let (flat, lw, lh) = layer_to_flat(&world);
    let origin = world.origin();
    // Place relative to the INPUT world's origin. When AE renders a partial
    // region (ROI) — typically near the frame edges — the input world has a
    // non-zero origin; ignoring it makes the mask/bg jump frame-to-frame as the
    // requested region shifts. Subtracting it keeps placement stable.
    Some(fit_layer_at_offset(
        &flat,
        lw,
        lh,
        w,
        h,
        origin.h as isize - in_h,
        origin.v as isize - in_v,
    ))
}
