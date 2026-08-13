use after_effects as ae;

mod glow;

// ---- Parameter IDs ----

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    // Core glow
    Brightness,
    ColorR,
    ColorG,
    ColorB,
    GlowWidth,
    Falloff,
    Bias,
    Threshold,
    ThresholdAddColorR,
    ThresholdAddColorG,
    ThresholdAddColorB,
    // Size controls
    WidthX,
    WidthY,
    WidthRed,
    WidthGreen,
    WidthBlue,
    // After glow
    AfterGlowWidth,
    AfterGlowColorR,
    AfterGlowColorG,
    AfterGlowColorB,
    AfterGlowStretchX,
    AfterGlowStretchY,
    // Streaks
    HorizontalStreaks,
    VerticalStreaks,
    // Source / combine
    Combine,
    ScaleSource,
    GlowUnderSource,
    GlowFromAlpha,
    AffectAlpha,
    Show,
    Mix,
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
        // --- Core Glow ---
        params.add(Params::Brightness, "Brightness",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(20.0);
                f.set_slider_min(0.0); f.set_slider_max(10.0);
                f.set_default(1.8); f.set_precision(2);
            }))?;

        params.add(Params::ColorR, "Color Red",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(1.0);
                f.set_slider_min(0.0); f.set_slider_max(1.0);
                f.set_default(1.0); f.set_precision(2);
            }))?;

        params.add(Params::ColorG, "Color Green",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(1.0);
                f.set_slider_min(0.0); f.set_slider_max(1.0);
                f.set_default(1.0); f.set_precision(2);
            }))?;

        params.add(Params::ColorB, "Color Blue",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(1.0);
                f.set_slider_min(0.0); f.set_slider_max(1.0);
                f.set_default(1.0); f.set_precision(2);
            }))?;

        params.add(Params::GlowWidth, "Glow Width",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(5.0);
                f.set_slider_min(0.0); f.set_slider_max(2.0);
                f.set_default(0.371); f.set_precision(3);
            }))?;

        params.add(Params::Falloff, "Falloff",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(-2.0); f.set_valid_max(2.0);
                f.set_slider_min(-2.0); f.set_slider_max(2.0);
                f.set_default(0.35); f.set_precision(2);
            }))?;

        params.add(Params::Bias, "Bias",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(-3.0); f.set_valid_max(3.0);
                f.set_slider_min(-3.0); f.set_slider_max(3.0);
                f.set_default(0.0); f.set_precision(2);
            }))?;

        params.add(Params::Threshold, "Threshold",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(1.0);
                f.set_slider_min(0.0); f.set_slider_max(1.0);
                f.set_default(0.4); f.set_precision(3);
            }))?;

        params.add(Params::ThresholdAddColorR, "Threshold Add Red",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(-1.0); f.set_valid_max(1.0);
                f.set_slider_min(-0.5); f.set_slider_max(0.5);
                f.set_default(0.0); f.set_precision(3);
            }))?;

        params.add(Params::ThresholdAddColorG, "Threshold Add Green",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(-1.0); f.set_valid_max(1.0);
                f.set_slider_min(-0.5); f.set_slider_max(0.5);
                f.set_default(0.0); f.set_precision(3);
            }))?;

        params.add(Params::ThresholdAddColorB, "Threshold Add Blue",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(-1.0); f.set_valid_max(1.0);
                f.set_slider_min(-0.5); f.set_slider_max(0.5);
                f.set_default(0.0); f.set_precision(3);
            }))?;

        // --- Size Controls ---
        params.add(Params::WidthX, "Width X",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(10.0);
                f.set_slider_min(0.0); f.set_slider_max(5.0);
                f.set_default(1.0); f.set_precision(2);
            }))?;

        params.add(Params::WidthY, "Width Y",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(10.0);
                f.set_slider_min(0.0); f.set_slider_max(5.0);
                f.set_default(1.0); f.set_precision(2);
            }))?;

        params.add(Params::WidthRed, "Width Red",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(5.0);
                f.set_slider_min(0.0); f.set_slider_max(3.0);
                f.set_default(1.0); f.set_precision(2);
            }))?;

        params.add(Params::WidthGreen, "Width Green",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(5.0);
                f.set_slider_min(0.0); f.set_slider_max(3.0);
                f.set_default(1.0); f.set_precision(2);
            }))?;

        params.add(Params::WidthBlue, "Width Blue",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(5.0);
                f.set_slider_min(0.0); f.set_slider_max(3.0);
                f.set_default(1.0); f.set_precision(2);
            }))?;

        // --- After Glow ---
        params.add(Params::AfterGlowWidth, "After Glow Width",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(5.0);
                f.set_slider_min(0.0); f.set_slider_max(2.0);
                f.set_default(0.808); f.set_precision(3);
            }))?;

        params.add(Params::AfterGlowColorR, "After Glow Color Red",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(1.0);
                f.set_slider_min(0.0); f.set_slider_max(1.0);
                f.set_default(1.0); f.set_precision(2);
            }))?;

        params.add(Params::AfterGlowColorG, "After Glow Color Green",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(1.0);
                f.set_slider_min(0.0); f.set_slider_max(1.0);
                f.set_default(1.0); f.set_precision(2);
            }))?;

        params.add(Params::AfterGlowColorB, "After Glow Color Blue",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(1.0);
                f.set_slider_min(0.0); f.set_slider_max(1.0);
                f.set_default(1.0); f.set_precision(2);
            }))?;

        params.add(Params::AfterGlowStretchX, "After Glow Stretch X",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(5.0);
                f.set_slider_min(0.0); f.set_slider_max(2.0);
                f.set_default(0.3); f.set_precision(2);
            }))?;

        params.add(Params::AfterGlowStretchY, "After Glow Stretch Y",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(5.0);
                f.set_slider_min(0.0); f.set_slider_max(2.0);
                f.set_default(0.1); f.set_precision(2);
            }))?;

        // --- Streaks ---
        params.add(Params::HorizontalStreaks, "Horizontal Streaks",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(5.0);
                f.set_slider_min(0.0); f.set_slider_max(2.0);
                f.set_default(0.25); f.set_precision(2);
            }))?;

        params.add(Params::VerticalStreaks, "Vertical Streaks",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(5.0);
                f.set_slider_min(0.0); f.set_slider_max(2.0);
                f.set_default(0.25); f.set_precision(2);
            }))?;

        // --- Source / Combine ---
        params.add(Params::Combine, "Combine",
            ae::PopupDef::setup(|f| {
                f.set_options(&["Multiply", "Add", "Screen", "Difference", "Overlay"]);
                f.set_default(3); // Screen (1-indexed: 1=Mult, 2=Add, 3=Screen, 4=Diff, 5=Overlay)
            }))?;

        params.add(Params::ScaleSource, "Scale Source",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(1.0);
                f.set_slider_min(0.0); f.set_slider_max(1.0);
                f.set_default(1.0); f.set_precision(2);
            }))?;

        params.add(Params::GlowUnderSource, "Glow Under Source",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(1.0);
                f.set_slider_min(0.0); f.set_slider_max(1.0);
                f.set_default(0.0); f.set_precision(2);
            }))?;

        params.add(Params::GlowFromAlpha, "Glow From Alpha",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(1.0);
                f.set_slider_min(0.0); f.set_slider_max(1.0);
                f.set_default(0.0); f.set_precision(2);
            }))?;

        params.add(Params::AffectAlpha, "Affect Alpha",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(1.0);
                f.set_slider_min(0.0); f.set_slider_max(1.0);
                f.set_default(0.0); f.set_precision(2);
            }))?;

        params.add(Params::Show, "Show",
            ae::PopupDef::setup(|f| {
                f.set_options(&["Result", "Threshold"]);
                f.set_default(1); // Result
            }))?;

        params.add(Params::Mix, "Mix with Original",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(100.0);
                f.set_slider_min(0.0); f.set_slider_max(100.0);
                f.set_default(100.0); f.set_precision(1);
            }))?;

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
                    "onmkGlow v0.1\rMulti-pass glow with falloff, bias,\rafter-glow shaping, and streaks.\rWritten in Rust.",
                );
            }
            ae::Command::Render { in_layer, mut out_layer } => {
                render_cpu(params, &in_layer, &mut out_layer)?;
            }
            ae::Command::SmartPreRender { mut extra } => {
                smart_pre_render(&in_data, &mut extra)?;
            }
            ae::Command::SmartRender { extra } => {
                smart_render_cpu(&extra, params)?;
            }
            ae::Command::SmartRenderGpu { .. } => {
                return Err(ae::Error::BadCallbackParameter);
            }
            _ => {}
        }
        Ok(())
    }
}

// ---- Extract parameters ----

pub struct GlowParams {
    pub brightness: f32,
    pub color_r: f32,
    pub color_g: f32,
    pub color_b: f32,
    pub width: f32,
    pub falloff: f32,
    pub bias: f32,
    pub threshold: f32,
    pub threshold_add_color_r: f32,
    pub threshold_add_color_g: f32,
    pub threshold_add_color_b: f32,
    pub width_x: f32,
    pub width_y: f32,
    pub width_red: f32,
    pub width_green: f32,
    pub width_blue: f32,
    pub after_glow_width: f32,
    pub after_glow_color_r: f32,
    pub after_glow_color_g: f32,
    pub after_glow_color_b: f32,
    pub after_glow_stretch_x: f32,
    pub after_glow_stretch_y: f32,
    pub horizontal_streaks: f32,
    pub vertical_streaks: f32,
    pub combine: i32,
    pub scale_source: f32,
    pub glow_under_source: f32,
    pub glow_from_alpha: f32,
    pub affect_alpha: f32,
    pub show: i32,
    pub mix: f32,
}

fn get_params(params: &ae::Parameters<Params>) -> Result<GlowParams, ae::Error> {
    Ok(GlowParams {
        brightness: params.get(Params::Brightness)?.as_float_slider()?.value() as f32,
        color_r: params.get(Params::ColorR)?.as_float_slider()?.value() as f32,
        color_g: params.get(Params::ColorG)?.as_float_slider()?.value() as f32,
        color_b: params.get(Params::ColorB)?.as_float_slider()?.value() as f32,
        width: params.get(Params::GlowWidth)?.as_float_slider()?.value() as f32,
        falloff: params.get(Params::Falloff)?.as_float_slider()?.value() as f32,
        bias: params.get(Params::Bias)?.as_float_slider()?.value() as f32,
        threshold: params.get(Params::Threshold)?.as_float_slider()?.value() as f32,
        threshold_add_color_r: params.get(Params::ThresholdAddColorR)?.as_float_slider()?.value() as f32,
        threshold_add_color_g: params.get(Params::ThresholdAddColorG)?.as_float_slider()?.value() as f32,
        threshold_add_color_b: params.get(Params::ThresholdAddColorB)?.as_float_slider()?.value() as f32,
        width_x: params.get(Params::WidthX)?.as_float_slider()?.value() as f32,
        width_y: params.get(Params::WidthY)?.as_float_slider()?.value() as f32,
        width_red: params.get(Params::WidthRed)?.as_float_slider()?.value() as f32,
        width_green: params.get(Params::WidthGreen)?.as_float_slider()?.value() as f32,
        width_blue: params.get(Params::WidthBlue)?.as_float_slider()?.value() as f32,
        after_glow_width: params.get(Params::AfterGlowWidth)?.as_float_slider()?.value() as f32,
        after_glow_color_r: params.get(Params::AfterGlowColorR)?.as_float_slider()?.value() as f32,
        after_glow_color_g: params.get(Params::AfterGlowColorG)?.as_float_slider()?.value() as f32,
        after_glow_color_b: params.get(Params::AfterGlowColorB)?.as_float_slider()?.value() as f32,
        after_glow_stretch_x: params.get(Params::AfterGlowStretchX)?.as_float_slider()?.value() as f32,
        after_glow_stretch_y: params.get(Params::AfterGlowStretchY)?.as_float_slider()?.value() as f32,
        horizontal_streaks: params.get(Params::HorizontalStreaks)?.as_float_slider()?.value() as f32,
        vertical_streaks: params.get(Params::VerticalStreaks)?.as_float_slider()?.value() as f32,
        combine: params.get(Params::Combine)?.as_popup()?.value() as i32,
        scale_source: params.get(Params::ScaleSource)?.as_float_slider()?.value() as f32,
        glow_under_source: params.get(Params::GlowUnderSource)?.as_float_slider()?.value() as f32,
        glow_from_alpha: params.get(Params::GlowFromAlpha)?.as_float_slider()?.value() as f32,
        affect_alpha: params.get(Params::AffectAlpha)?.as_float_slider()?.value() as f32,
        show: params.get(Params::Show)?.as_popup()?.value() as i32,
        mix: params.get(Params::Mix)?.as_float_slider()?.value().clamp(0.0, 100.0) as f32 / 100.0,
    })
}

// ---- Layer helpers ----

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

// ---- Render ----

fn render_cpu(
    params: &ae::Parameters<Params>,
    in_layer: &ae::Layer,
    out_layer: &mut ae::Layer,
) -> Result<(), ae::Error> {
    let gp = get_params(params)?;
    let (src, w, h) = layer_to_flat(in_layer);
    let result = glow::ultra_glow(&gp, &src, w, h);
    flat_to_layer(&result, out_layer, w, h);
    Ok(())
}

fn smart_pre_render(
    in_data: &ae::InData,
    extra: &mut ae::pf::PreRenderExtra,
) -> Result<(), ae::Error> {
    let req = extra.output_request();
    let cb = extra.callbacks();
    let in_result = cb.checkout_layer(
        0, 0, &req,
        in_data.current_time(), in_data.time_step(), in_data.time_scale(),
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
    Ok(())
}

fn smart_render_cpu(
    extra: &ae::pf::SmartRenderExtra,
    params: &ae::Parameters<Params>,
) -> Result<(), ae::Error> {
    let gp = get_params(params)?;
    let cb = extra.callbacks();
    let input_world = cb.checkout_layer_pixels(0)?.ok_or(ae::Error::Generic)?;
    let mut output_world = cb.checkout_output()?.ok_or(ae::Error::Generic)?;
    let (src, w, h) = layer_to_flat(&input_world);
    let result = glow::ultra_glow(&gp, &src, w, h);
    flat_to_layer(&result, &mut output_world, w, h);
    cb.checkin_layer_pixels(0)?;
    Ok(())
}
