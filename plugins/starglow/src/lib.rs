use after_effects as ae;

mod glow;
mod blend;
mod colormap;
mod gpu;
mod pixels;

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
    // Appended for compatibility with existing projects.
    AffectAlpha,
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
        params.add(Params::AffectAlpha, "Affect Alpha",
            ae::CheckBoxDef::setup(|f| { f.set_default(true); f.set_label("Extend Alpha with Glow"); }))?;

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
                let (w, h) = (in_layer.width(), in_layer.height());
                let luma_map = get_map_layer_luma(params, &in_data, w, h, ep.invert_map)?;
                render_layers(&ep, &in_layer, &mut out_layer, luma_map.as_deref())?;
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

#[cfg_attr(test, derive(Default))]
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
    pub affect_alpha: bool,
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
        affect_alpha: params.get(Params::AffectAlpha)?.as_checkbox()?.value(),
    })
}

// ---- Pixel buffer helpers ----

fn map_luma<T: pixels::Channel>(layer: &ae::Layer, w: usize, h: usize, invert: bool) -> Result<Vec<f64>, ae::Error> {
    let (flat, mw, mh) = pixels::read::<T>(layer)?;
    let count = w.checked_mul(h).ok_or(ae::Error::BadCallbackParameter)?;
    if w == 0 || h == 0 { return Err(ae::Error::BadCallbackParameter); }
    let mut map = glow::filled::<f64>(count)?;
    for y in 0..h {
        let sy = ((y as u128 * mh as u128 / h as u128) as usize).min(mh - 1);
        for x in 0..w {
            let sx = ((x as u128 * mw as u128 / w as u128) as usize).min(mw - 1);
            let p = &flat[(sy * mw + sx) * 4..];
            let luma = 0.2126 * p[1].normalized() + 0.7152 * p[2].normalized() + 0.0722 * p[3].normalized();
            map[y * w + x] = if invert { 1.0 - luma } else { luma };
        }
    }
    Ok(map)
}

fn layer_luma(layer: &ae::Layer, w: usize, h: usize, invert: bool) -> Result<Vec<f64>, ae::Error> {
    match pixels::channel_bytes(layer)? {
        1 => map_luma::<u8>(layer, w, h, invert),
        2 => map_luma::<u16>(layer, w, h, invert),
        4 => map_luma::<f32>(layer, w, h, invert),
        _ => Err(ae::Error::BadCallbackParameter),
    }
}

fn decode_optional_map<T, U>(map: Result<Option<T>, ae::Error>,
    decode: impl FnOnce(T) -> Result<U, ae::Error>) -> Result<Option<U>, ae::Error> {
    map?.map(decode).transpose()
}

#[test]
fn absent_map_and_failed_map_are_distinct() {
    use std::cell::Cell;
    let calls = Cell::new(0);
    for (input, expected) in [
        (Ok(None), Ok(None)),
        (Err(ae::Error::Generic), Err(ae::Error::Generic)),
        (Ok(Some(1)), Ok(Some(7))),
        (Ok(Some(2)), Err(ae::Error::OutOfMemory)),
    ] {
        let result = decode_optional_map(input, |value| {
            calls.set(calls.get()+1);
            if value == 1 { Ok(7) } else { Err(ae::Error::OutOfMemory) }
        });
        assert_eq!(result, expected);
    }
    assert_eq!(calls.get(), 2);
}

/// Read the map layer via legacy checkout
fn get_map_layer_luma(
    params: &ae::Parameters<Params>,
    in_data: &ae::InData,
    w: usize, h: usize,
    invert: bool,
) -> Result<Option<Vec<f64>>, ae::Error> {
    let checkout = params.checkout_at(
        Params::MapLayer,
        Some(in_data.current_time()),
        None, None,
    )?;
    let layer_def = checkout.as_layer()?;
    decode_optional_map(Ok(layer_def.value()), |layer| layer_luma(&layer, w, h, invert))
}

// ---- SmartFX PreRender ----

// Parameter index for MapLayer (input=0; includes all preceding controls).
const MAP_PARAM_INDEX: i32 = 31;
const MAP_CHECKOUT_ID: i32 = 1;

fn has_pixels(rect: ae::Rect) -> bool {
    rect.right > rect.left && rect.bottom > rect.top
}

#[test]
fn absent_map_is_not_a_black_control_image() {
    assert!(!has_pixels(ae::Rect { left: 0, top: 0, right: 0, bottom: 0 }));
    assert!(!has_pixels(ae::Rect { left: 5, top: 0, right: 5, bottom: 20 }));
    assert!(has_pixels(ae::Rect { left: -5, top: 0, right: 5, bottom: 20 }));
}

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
    let map_has_pixels = cb.checkout_layer(
        MAP_PARAM_INDEX, MAP_CHECKOUT_ID,
        &req,
        in_data.current_time(),
        in_data.time_step(),
        in_data.time_scale(),
    ).map(|r| has_pixels(r.result_rect.into()))?;
    extra.set_pre_render_data(map_has_pixels);
    Ok(())
}

// ---- Render ----

fn render_with_params<T: glow::Channel>(
    ep: &EffectParams, original: &[T], w: usize, h: usize,
    luma_map: Option<&[f64]>,
) -> Result<Vec<T>, ae::Error> {
    if let Some(gpu) = get_gpu() {
        if let Ok(result) = gpu.process_channels(ep, original, w, h, luma_map) { return Ok(result); }
    }
    glow::process_channels(ep, original, w, h, luma_map)
}

fn render_typed<T: pixels::Channel + glow::Channel>(ep: &EffectParams, input: &ae::Layer, output: &mut ae::Layer, map: Option<&[f64]>) -> Result<(), ae::Error> {
    let (original, w, h) = pixels::read::<T>(input)?;
    let result = render_with_params(ep, &original, w, h, map)?;
    pixels::write(&result, output, w, h)
}

fn render_layers(ep: &EffectParams, input: &ae::Layer, output: &mut ae::Layer, map: Option<&[f64]>) -> Result<(), ae::Error> {
    match pixels::channel_bytes(input)? {
        1 => render_typed::<u8>(ep, input, output, map),
        2 => render_typed::<u16>(ep, input, output, map),
        4 => render_typed::<f32>(ep, input, output, map),
        _ => Err(ae::Error::BadCallbackParameter),
    }
}

fn with_pixel_checkin<T>(work: impl FnOnce() -> Result<T, ae::Error>,
    checkin: impl FnOnce() -> Result<(), ae::Error>) -> Result<T, ae::Error> {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work));
    let cleanup = checkin();
    match result {
        Ok(Ok(value)) => cleanup.map(|()| value),
        Ok(Err(error)) => Err(error),
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

#[test]
fn pixel_checkin_preserves_errors_and_unwinds_in_reverse_order() {
    use std::cell::RefCell;
    for fail_work in [false, true] {
        for fail_cleanup in [false, true] {
            let calls = RefCell::new(Vec::new());
            let result = with_pixel_checkin(|| with_pixel_checkin(
                || if fail_work { Err(ae::Error::OutOfMemory) } else { Ok(7) },
                || { calls.borrow_mut().push(1); if fail_cleanup { Err(ae::Error::BadCallbackParameter) } else { Ok(()) } }),
                || { calls.borrow_mut().push(0); Ok(()) });
            assert_eq!(*calls.borrow(), vec![1, 0]);
            assert_eq!(result, if fail_work { Err(ae::Error::OutOfMemory) }
                else if fail_cleanup { Err(ae::Error::BadCallbackParameter) } else { Ok(7) });
        }
    }
    for fail_cleanup in [false, true] {
        let calls = RefCell::new(Vec::new());
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _: Result<(), ae::Error> = with_pixel_checkin(|| with_pixel_checkin(
                || panic!("render sentinel"),
                || { calls.borrow_mut().push(1); if fail_cleanup { Err(ae::Error::Generic) } else { Ok(()) } }),
                || { calls.borrow_mut().push(0); Ok(()) });
        })).unwrap_err();
        assert_eq!(panic.downcast_ref::<&str>(), Some(&"render sentinel"));
        assert_eq!(*calls.borrow(), vec![1, 0]);
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
    with_pixel_checkin(|| {
        let mut output_world = cb.checkout_output()?.ok_or(ae::Error::Generic)?;
        let (w, h) = (input_world.width(), input_world.height());
        let map_world = if extra.pre_render_data::<bool>().copied().unwrap_or(false) {
            cb.checkout_layer_pixels(MAP_CHECKOUT_ID as u32)?
        } else { None };
        with_pixel_checkin(|| {
            let luma_map = map_world.as_ref().map(|map| layer_luma(map, w, h, ep.invert_map)).transpose()?;
            render_layers(&ep, &input_world, &mut output_world, luma_map.as_deref())
        }, || if map_world.is_some() { cb.checkin_layer_pixels(MAP_CHECKOUT_ID as u32) } else { Ok(()) })
    }, || cb.checkin_layer_pixels(0))
}
