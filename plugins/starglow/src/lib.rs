use after_effects as ae;

mod glow;
mod blend;
mod colormap;
mod gpu;

use std::sync::OnceLock;

static GPU: OnceLock<Option<gpu::GpuProcessor>> = OnceLock::new();

fn get_gpu() -> Option<&'static gpu::GpuProcessor> {
    GPU.get_or_init(|| {
        match std::panic::catch_unwind(|| gpu::GpuProcessor::new()) {
            Ok(g) => Some(g),
            Err(_) => None,
        }
    }).as_ref()
}

// ---- Parameter IDs ----

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    InputChannel,
    GlowShape,
    Threshold,
    ThresholdSoft,
    StreakLength,
    BoostLight,
    DecayRate,
    // Individual Lengths (used in Custom mode)
    LenUp, LenDown, LenLeft, LenRight,
    LenUpLeft, LenUpRight, LenDownLeft, LenDownRight,
    // Colormap
    ColormapPreset,
    ColorHighlights, ColorMidHigh, ColorMidtones, ColorMidLow, ColorShadows,
    // Spectrum
    SpectrumOffset, SpectrumDensity, SpectrumRandom,
    // Shimmer
    ShimmerAmount, ShimmerDetail, ShimmerPhase,
    // Output
    SourceOpacity, StarglowOpacity, TransferMode,
    // Map
    MapLayer, InvertMap,
}

// ---- Plugin ----

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
        params.add(Params::InputChannel, "Input Channel",
            ae::PopupDef::setup(|f| {
                f.set_options(&["Lightness", "Luminance", "Alpha", "Red", "Green", "Blue"]);
                f.set_default(2);
            }))?;

        params.add(Params::GlowShape, "Glow Shape",
            ae::PopupDef::setup(|f| {
                f.set_options(&[
                    "Star", "Cross (+)", "X", "H (Horizontal)", "V (Vertical)",
                    "Tri", "Y", "6-Point", "Custom",
                ]);
                f.set_default(1); // Star
            }))?;

        params.add(Params::Threshold, "Threshold",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(1000.0);
                f.set_slider_min(0.0); f.set_slider_max(1000.0);
                f.set_default(50.0); f.set_precision(1);
            }))?;

        params.add(Params::ThresholdSoft, "Threshold Soft",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(100.0);
                f.set_slider_min(0.0); f.set_slider_max(100.0);
                f.set_default(25.0); f.set_precision(1);
            }))?;

        params.add(Params::StreakLength, "Streak Length",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(500.0);
                f.set_slider_min(0.0); f.set_slider_max(500.0);
                f.set_default(80.0); f.set_precision(1);
            }))?;

        params.add(Params::BoostLight, "Boost Light",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(500.0);
                f.set_slider_min(0.0); f.set_slider_max(500.0);
                f.set_default(100.0); f.set_precision(1);
            }))?;

        params.add(Params::DecayRate, "Decay Rate",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.1); f.set_valid_max(10.0);
                f.set_slider_min(0.1); f.set_slider_max(10.0);
                f.set_default(1.0); f.set_precision(2);
            }))?;

        // Individual Lengths
        for (param, name) in [
            (Params::LenUp, "Length Up"),
            (Params::LenDown, "Length Down"),
            (Params::LenLeft, "Length Left"),
            (Params::LenRight, "Length Right"),
            (Params::LenUpLeft, "Length Up-Left"),
            (Params::LenUpRight, "Length Up-Right"),
            (Params::LenDownLeft, "Length Down-Left"),
            (Params::LenDownRight, "Length Down-Right"),
        ] {
            params.add(param, name,
                ae::FloatSliderDef::setup(|f| {
                    f.set_valid_min(0.0); f.set_valid_max(200.0);
                    f.set_slider_min(0.0); f.set_slider_max(200.0);
                    f.set_default(100.0); f.set_precision(1);
                }))?;
        }

        // Colormap
        params.add(Params::ColormapPreset, "Colormap Preset",
            ae::PopupDef::setup(|f| {
                f.set_options(&[
                    "One Color", "3-Color Gradient", "5-Color Gradient", "(-",
                    "Fire", "Electric", "Rainbow", "Heaven", "Romance", "Aqualight", "Sunset",
                    "(-", "Spectrum",
                ]);
                f.set_default(1);
            }))?;

        params.add(Params::ColorHighlights, "Color: Highlights",
            ae::ColorDef::setup(|f| { f.set_default(ae::Pixel8 { alpha: 255, red: 255, green: 255, blue: 255 }); }))?;
        params.add(Params::ColorMidHigh, "Color: Mid-High",
            ae::ColorDef::setup(|f| { f.set_default(ae::Pixel8 { alpha: 255, red: 255, green: 230, blue: 200 }); }))?;
        params.add(Params::ColorMidtones, "Color: Midtones",
            ae::ColorDef::setup(|f| { f.set_default(ae::Pixel8 { alpha: 255, red: 255, green: 200, blue: 150 }); }))?;
        params.add(Params::ColorMidLow, "Color: Mid-Low",
            ae::ColorDef::setup(|f| { f.set_default(ae::Pixel8 { alpha: 255, red: 200, green: 150, blue: 100 }); }))?;
        params.add(Params::ColorShadows, "Color: Shadows",
            ae::ColorDef::setup(|f| { f.set_default(ae::Pixel8 { alpha: 255, red: 100, green: 50, blue: 20 }); }))?;

        // Spectrum
        params.add(Params::SpectrumOffset, "Spectrum Offset",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(360.0);
                f.set_slider_min(0.0); f.set_slider_max(360.0);
                f.set_default(0.0); f.set_precision(1);
            }))?;
        params.add(Params::SpectrumDensity, "Spectrum Density",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.1); f.set_valid_max(10.0);
                f.set_slider_min(0.1); f.set_slider_max(10.0);
                f.set_default(1.0); f.set_precision(2);
            }))?;
        params.add(Params::SpectrumRandom, "Spectrum Random",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(100.0);
                f.set_slider_min(0.0); f.set_slider_max(100.0);
                f.set_default(0.0); f.set_precision(1);
            }))?;

        // Shimmer
        params.add(Params::ShimmerAmount, "Shimmer Amount",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(100.0);
                f.set_slider_min(0.0); f.set_slider_max(100.0);
                f.set_default(0.0); f.set_precision(1);
            }))?;
        params.add(Params::ShimmerDetail, "Shimmer Detail",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(100.0);
                f.set_slider_min(0.0); f.set_slider_max(100.0);
                f.set_default(50.0); f.set_precision(1);
            }))?;
        params.add(Params::ShimmerPhase, "Shimmer Phase",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(360.0);
                f.set_slider_min(0.0); f.set_slider_max(360.0);
                f.set_default(0.0); f.set_precision(1);
            }))?;

        // Output
        params.add(Params::SourceOpacity, "Source Opacity",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(100.0);
                f.set_slider_min(0.0); f.set_slider_max(100.0);
                f.set_default(100.0); f.set_precision(1);
            }))?;
        params.add(Params::StarglowOpacity, "Starglow Opacity",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(100.0);
                f.set_slider_min(0.0); f.set_slider_max(100.0);
                f.set_default(100.0); f.set_precision(1);
            }))?;
        params.add(Params::TransferMode, "Transfer Mode",
            ae::PopupDef::setup(|f| {
                f.set_options(&[
                    "None", "Normal", "(-",
                    "Add", "Multiply", "Screen", "Overlay",
                    "Soft Light", "Hard Light", "(-",
                    "Color Dodge", "Color Burn", "(-",
                    "Darken", "Lighten", "Difference",
                ]);
                f.set_default(4);
            }))?;

        // Map layer
        params.add(Params::MapLayer, "Luminance Map",
            ae::LayerDef::setup(|_f| {}))?;
        params.add(Params::InvertMap, "Invert Map",
            ae::CheckBoxDef::setup(|f| { f.set_default(false); f.set_label("Invert"); }))?;

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
                    "ONMK_Starglow v2.0\rStar-shaped glow effect.\rDirectional light streaks with shape presets.\rWritten in Rust.",
                );
            }

            ae::Command::Render { in_layer, mut out_layer } => {
                let ep = get_params(params)?;
                let (original, w, h) = layer_to_flat(&in_layer);
                let luma_map = get_map_layer_luma(params, &in_data, w, h, ep.invert_map);
                let result = render_with_params(&ep, &original, w, h, luma_map.as_deref());
                flat_to_layer(&result, &mut out_layer, w, h);
            }

            ae::Command::SmartPreRender { mut extra } => {
                smart_pre_render(&in_data, &mut extra)?;
            }

            ae::Command::SmartRender { extra } => {
                smart_render(&extra, params, &in_data)?;
            }

            ae::Command::SmartRenderGpu { extra } => {
                smart_render(&extra, params, &in_data)?;
            }

            _ => {}
        }
        Ok(())
    }
}

// ---- Parameters ----

pub struct EffectParams {
    pub input_channel: i32,
    pub glow_shape: i32,
    pub threshold: f64,
    pub threshold_soft: f64,
    pub streak_length: f64,
    pub boost_light: f64,
    pub decay_rate: f64,
    pub lengths: [f64; 8],
    pub colormap_preset: i32,
    pub colors: [[f64; 3]; 5],
    pub spectrum_offset: f64,
    pub spectrum_density: f64,
    pub spectrum_random: f64,
    pub shimmer_amount: f64,
    pub shimmer_detail: f64,
    pub shimmer_phase: f64,
    pub source_opacity: f64,
    pub starglow_opacity: f64,
    pub transfer_mode: i32,
    pub invert_map: bool,
}

fn color_to_f64(p: ae::Pixel8) -> [f64; 3] {
    [p.red as f64 / 255.0, p.green as f64 / 255.0, p.blue as f64 / 255.0]
}

fn get_params(params: &ae::Parameters<Params>) -> Result<EffectParams, ae::Error> {
    let lengths = [
        params.get(Params::LenUp)?.as_float_slider()?.value() / 100.0,
        params.get(Params::LenDown)?.as_float_slider()?.value() / 100.0,
        params.get(Params::LenLeft)?.as_float_slider()?.value() / 100.0,
        params.get(Params::LenRight)?.as_float_slider()?.value() / 100.0,
        params.get(Params::LenUpLeft)?.as_float_slider()?.value() / 100.0,
        params.get(Params::LenUpRight)?.as_float_slider()?.value() / 100.0,
        params.get(Params::LenDownLeft)?.as_float_slider()?.value() / 100.0,
        params.get(Params::LenDownRight)?.as_float_slider()?.value() / 100.0,
    ];
    let colors = [
        color_to_f64(params.get(Params::ColorHighlights)?.as_color()?.value()),
        color_to_f64(params.get(Params::ColorMidHigh)?.as_color()?.value()),
        color_to_f64(params.get(Params::ColorMidtones)?.as_color()?.value()),
        color_to_f64(params.get(Params::ColorMidLow)?.as_color()?.value()),
        color_to_f64(params.get(Params::ColorShadows)?.as_color()?.value()),
    ];
    Ok(EffectParams {
        input_channel: params.get(Params::InputChannel)?.as_popup()?.value() as i32,
        glow_shape: params.get(Params::GlowShape)?.as_popup()?.value() as i32,
        threshold: params.get(Params::Threshold)?.as_float_slider()?.value() / 100.0,
        threshold_soft: params.get(Params::ThresholdSoft)?.as_float_slider()?.value() / 100.0,
        streak_length: params.get(Params::StreakLength)?.as_float_slider()?.value(),
        boost_light: params.get(Params::BoostLight)?.as_float_slider()?.value() / 100.0,
        decay_rate: params.get(Params::DecayRate)?.as_float_slider()?.value(),
        lengths,
        colormap_preset: params.get(Params::ColormapPreset)?.as_popup()?.value() as i32,
        colors,
        spectrum_offset: params.get(Params::SpectrumOffset)?.as_float_slider()?.value(),
        spectrum_density: params.get(Params::SpectrumDensity)?.as_float_slider()?.value(),
        spectrum_random: params.get(Params::SpectrumRandom)?.as_float_slider()?.value() / 100.0,
        shimmer_amount: params.get(Params::ShimmerAmount)?.as_float_slider()?.value() / 100.0,
        shimmer_detail: params.get(Params::ShimmerDetail)?.as_float_slider()?.value() / 100.0,
        shimmer_phase: params.get(Params::ShimmerPhase)?.as_float_slider()?.value(),
        source_opacity: params.get(Params::SourceOpacity)?.as_float_slider()?.value() / 100.0,
        starglow_opacity: params.get(Params::StarglowOpacity)?.as_float_slider()?.value() / 100.0,
        transfer_mode: params.get(Params::TransferMode)?.as_popup()?.value() as i32,
        invert_map: params.get(Params::InvertMap)?.as_checkbox()?.value(),
    })
}

// ---- Pixel buffer helpers ----

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

fn build_luminance_map(buf: &[u8], w: usize, h: usize, invert: bool) -> Vec<f64> {
    let mut map = vec![0.0f64; w * h];
    for i in 0..(w * h) {
        let off = i * 4;
        let r = buf[off + 1] as f64;
        let g = buf[off + 2] as f64;
        let b = buf[off + 3] as f64;
        let luma = (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255.0;
        map[i] = if invert { 1.0 - luma } else { luma };
    }
    map
}

fn build_luminance_map_resample(
    buf: &[u8], mw: usize, mh: usize, w: usize, h: usize, invert: bool,
) -> Vec<f64> {
    let src_map = build_luminance_map(buf, mw, mh, invert);
    let mut map = vec![0.0f64; w * h];
    for y in 0..h {
        let sy = (y * mh / h).min(mh - 1);
        for x in 0..w {
            let sx = (x * mw / w).min(mw - 1);
            map[y * w + x] = src_map[sy * mw + sx];
        }
    }
    map
}

/// Read the map layer via legacy checkout
fn get_map_layer_luma(
    params: &ae::Parameters<Params>,
    in_data: &ae::InData,
    w: usize, h: usize,
    invert: bool,
) -> Option<Vec<f64>> {
    let checkout = params.checkout_at(
        Params::MapLayer,
        Some(in_data.current_time()),
        None, None,
    ).ok()?;
    let layer_def = checkout.as_layer().ok()?;
    let layer = layer_def.value()?;
    let (flat, mw, mh) = layer_to_flat(&layer);
    if mw == w && mh == h {
        Some(build_luminance_map(&flat, w, h, invert))
    } else {
        Some(build_luminance_map_resample(&flat, mw, mh, w, h, invert))
    }
}

// ---- SmartFX PreRender ----

// Parameter index for MapLayer (1-based: input=0, InputChannel=1, ..., MapLayer=28)
const MAP_PARAM_INDEX: i32 = 31;
const MAP_CHECKOUT_ID: i32 = 1;

fn smart_pre_render(
    in_data: &ae::InData,
    extra: &mut ae::pf::PreRenderExtra,
) -> Result<(), ae::Error> {
    let req = extra.output_request();
    let cb = extra.callbacks();

    let _in_result = cb.checkout_layer(
        0, 0, &req,
        in_data.current_time(),
        in_data.time_step(),
        in_data.time_scale(),
    )?;

    // Starglow can create visible pixels outside the source/precomp bounds.
    // Keep AE's requested output area instead of clipping it to the input's
    // result rect; checkout_layer supplies transparent pixels outside input.
    let req_rect: ae::Rect = req.rect.into();
    extra.set_result_rect(req_rect);
    extra.set_max_result_rect(req_rect);

    // Checkout map layer
    let _ = cb.checkout_layer(
        MAP_PARAM_INDEX, MAP_CHECKOUT_ID,
        &req,
        in_data.current_time(),
        in_data.time_step(),
        in_data.time_scale(),
    );

    Ok(())
}

// ---- Render ----

fn render_with_params(
    ep: &EffectParams, original: &[u8], w: usize, h: usize,
    luma_map: Option<&[f64]>,
) -> Vec<u8> {
    if let Some(gpu) = get_gpu() {
        gpu.process(ep, original, w, h, luma_map)
    } else {
        glow::process(ep, original, w, h, luma_map)
    }
}

fn smart_render(
    extra: &ae::pf::SmartRenderExtra,
    params: &ae::Parameters<Params>,
    _in_data: &ae::InData,
) -> Result<(), ae::Error> {
    let ep = get_params(params)?;
    let cb = extra.callbacks();

    let input_world = cb.checkout_layer_pixels(0)?
        .ok_or(ae::Error::Generic)?;
    let mut output_world = cb.checkout_output()?
        .ok_or(ae::Error::Generic)?;

    let (original, w, h) = layer_to_flat(&input_world);

    let luma_map = if let Ok(Some(map_world)) = cb.checkout_layer_pixels(MAP_CHECKOUT_ID as u32) {
        let (map_flat, mw, mh) = layer_to_flat(&map_world);
        if mw == w && mh == h {
            Some(build_luminance_map(&map_flat, w, h, ep.invert_map))
        } else {
            Some(build_luminance_map_resample(&map_flat, mw, mh, w, h, ep.invert_map))
        }
    } else {
        None
    };

    let result = render_with_params(&ep, &original, w, h, luma_map.as_deref());
    flat_to_layer(&result, &mut output_world, w, h);

    cb.checkin_layer_pixels(0)?;
    let _ = cb.checkin_layer_pixels(MAP_CHECKOUT_ID as u32);
    Ok(())
}
