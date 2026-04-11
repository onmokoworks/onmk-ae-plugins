use after_effects as ae;

mod refract;

// ---- Parameter IDs ----

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    IorR,
    IorG,
    IorB,
    RefractPower,
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
    BgLayer,
    Mix,
    UseGpu,
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
        params.add(
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

        params.add(
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

        params.add(
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

        params.add(
            Params::RefractPower,
            "Refract Power",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(2000.0);
                f.set_slider_min(0.0);
                f.set_slider_max(300.0);
                f.set_default(50.0);
                f.set_precision(1);
            }),
        )?;

        params.add(
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

        params.add(
            Params::PerAxisChroma,
            "Per-axis Chroma",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(false);
                f.set_label("Enable X/Y split");
            }),
        )?;

        params.add(
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

        params.add(
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

        params.add(
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

        params.add(
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

        params.add(
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

        params.add(
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

        params.add(
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

        params.add(
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

        params.add(
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

        params.add(
            Params::HeightStrength,
            "Height Strength",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(10.0);
                f.set_slider_min(0.0);
                f.set_slider_max(5.0);
                f.set_default(1.0);
                f.set_precision(2);
            }),
        )?;

        params.add(
            Params::HeightBlur,
            "Height Blur",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(500.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_default(8.0);
                f.set_precision(1);
            }),
        )?;

        params.add(
            Params::EdgeBlur,
            "Edge Blur",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(500.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_default(4.0);
                f.set_precision(1);
            }),
        )?;

        params.add(
            Params::Use6ch,
            "Use 6ch Dispersion (rygcbv)",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(false);
                f.set_label("Enable");
            }),
        )?;

        params.add(
            Params::MaskLayer,
            "Mask (shape)",
            ae::LayerDef::setup(|_f| {}),
        )?;

        params.add(
            Params::BgLayer,
            "Background",
            ae::LayerDef::setup(|_f| {}),
        )?;

        params.add(
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

        params.add(
            Params::UseGpu,
            "Use GPU (CUDA)",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(true);
                f.set_label("Enable");
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
                    "RefractionDispersion v1.0\rPhysically-inspired refraction\rwith chromatic dispersion.\rPort of Maxime Heckel's shader.\rWritten in Rust.",
                );
            }

            ae::Command::Render {
                in_layer,
                mut out_layer,
            } => {
                render_cpu(params, &in_data, &in_layer, &mut out_layer)?;
            }

            ae::Command::SmartPreRender { mut extra } => {
                smart_pre_render(&in_data, &mut extra)?;
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
    pub mix: f32,
    pub use_gpu: bool,
}

fn get_params(params: &ae::Parameters<Params>) -> Result<RefractParams, ae::Error> {
    Ok(RefractParams {
        ior_r: params.get(Params::IorR)?.as_float_slider()?.value() as f32,
        ior_g: params.get(Params::IorG)?.as_float_slider()?.value() as f32,
        ior_b: params.get(Params::IorB)?.as_float_slider()?.value() as f32,
        refract_power: params.get(Params::RefractPower)?.as_float_slider()?.value() as f32,
        chromatic_ab: params.get(Params::ChromaticAb)?.as_float_slider()?.value() as f32,
        use_per_axis_chroma: params.get(Params::PerAxisChroma)?.as_checkbox()?.value(),
        chromatic_ab_x: params.get(Params::ChromaticAbX)?.as_float_slider()?.value() as f32,
        chromatic_ab_y: params.get(Params::ChromaticAbY)?.as_float_slider()?.value() as f32,
        samples: params.get(Params::Samples)?.as_slider()?.value().clamp(1, 64) as usize,
        fresnel_power: params.get(Params::FresnelPower)?.as_float_slider()?.value() as f32,
        shininess: params.get(Params::Shininess)?.as_float_slider()?.value() as f32,
        diffuseness: params.get(Params::Diffuseness)?.as_float_slider()?.value() as f32,
        light_angle_x: params.get(Params::LightAngleX)?.as_float_slider()?.value() as f32,
        light_angle_y: params.get(Params::LightAngleY)?.as_float_slider()?.value() as f32,
        saturation: params.get(Params::Saturation)?.as_float_slider()?.value() as f32,
        height_strength: params.get(Params::HeightStrength)?.as_float_slider()?.value() as f32,
        height_blur: params.get(Params::HeightBlur)?.as_float_slider()?.value().max(0.0),
        edge_blur: params.get(Params::EdgeBlur)?.as_float_slider()?.value().max(0.0),
        use_6ch: params.get(Params::Use6ch)?.as_checkbox()?.value(),
        mix: (params.get(Params::Mix)?.as_float_slider()?.value().clamp(0.0, 100.0) / 100.0) as f32,
        use_gpu: params.get(Params::UseGpu)?.as_checkbox()?.value(),
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
    let mask = get_layer_flat(params, Params::MaskLayer, in_data, w, h);
    let bg = get_layer_flat(params, Params::BgLayer, in_data, w, h);
    let result = refract::render(&rp, &src, mask.as_deref(), bg.as_deref(), w, h);
    flat_to_layer(&result, out_layer, w, h);
    Ok(())
}

fn get_layer_flat(
    params: &ae::Parameters<Params>,
    param_id: Params,
    in_data: &ae::InData,
    w: usize,
    h: usize,
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
        origin.h as isize,
        origin.v as isize,
    ))
}

// ---- SmartFX PreRender ----
//
// Param index layout (0 = input, then user params in the order added):
// 0 = input, 1 = IorR, 2 = IorG, 3 = IorB, 4 = RefractPower,
// 5 = ChromaticAb, 6 = PerAxisChroma, 7 = ChromaticAbX, 8 = ChromaticAbY,
// 9 = Samples, 10 = FresnelPower, 11 = Shininess, 12 = Diffuseness,
// 13 = LightAngleX, 14 = LightAngleY, 15 = Saturation,
// 16 = HeightStrength, 17 = HeightBlur, 18 = EdgeBlur, 19 = Use6ch,
// 20 = MaskLayer, 21 = BgLayer, 22 = Mix, 23 = UseGpu

const MASK_PARAM_INDEX: i32 = 20;
const BG_PARAM_INDEX: i32 = 21;
const MASK_CHECKOUT_ID: i32 = 1;
const BG_CHECKOUT_ID: i32 = 2;

fn smart_pre_render(
    in_data: &ae::InData,
    extra: &mut ae::pf::PreRenderExtra,
) -> Result<(), ae::Error> {
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

    let _ = cb.checkout_layer(
        MASK_PARAM_INDEX,
        MASK_CHECKOUT_ID,
        &req,
        in_data.current_time(),
        in_data.time_step(),
        in_data.time_scale(),
    );

    let _ = cb.checkout_layer(
        BG_PARAM_INDEX,
        BG_CHECKOUT_ID,
        &req,
        in_data.current_time(),
        in_data.time_step(),
        in_data.time_scale(),
    );

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

    let mask = checkout_layer_flat(&cb, MASK_CHECKOUT_ID as u32, w, h);
    let bg = checkout_layer_flat(&cb, BG_CHECKOUT_ID as u32, w, h);

    let result = refract::render(&rp, &src, mask.as_deref(), bg.as_deref(), w, h);
    flat_to_layer(&result, &mut output_world, w, h);

    cb.checkin_layer_pixels(0)?;
    let _ = cb.checkin_layer_pixels(MASK_CHECKOUT_ID as u32);
    let _ = cb.checkin_layer_pixels(BG_CHECKOUT_ID as u32);
    Ok(())
}

fn checkout_layer_flat(
    cb: &ae::pf::SmartRenderCallbacks,
    checkout_id: u32,
    w: usize,
    h: usize,
) -> Option<Vec<u8>> {
    let world = cb.checkout_layer_pixels(checkout_id).ok()??;
    let (flat, lw, lh) = layer_to_flat(&world);
    let origin = world.origin();
    Some(fit_layer_at_offset(
        &flat,
        lw,
        lh,
        w,
        h,
        origin.h as isize,
        origin.v as isize,
    ))
}
