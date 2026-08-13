use after_effects as ae;

mod scatter;

// ---- Parameter IDs ----

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Amount,
    Direction,
    RandomSeed,
    RepeatEdge,
    Mix,
    MapLayer,
    InvertMap,
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
        params.add(Params::Amount, "Scatter Amount",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0); f.set_valid_max(500);
                f.set_slider_min(0); f.set_slider_max(100);
                f.set_default(5);
            }))?;

        params.add(Params::Direction, "Direction",
            ae::PopupDef::setup(|f| {
                f.set_options(&["Horizontal", "Vertical", "Both"]);
                f.set_default(3); // Both
            }))?;

        params.add(Params::RandomSeed, "Random Seed",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0); f.set_valid_max(10000);
                f.set_slider_min(0); f.set_slider_max(10000);
                f.set_default(0);
            }))?;

        params.add(Params::RepeatEdge, "Repeat Edge Pixels",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(true);
                f.set_label("Repeat");
            }))?;

        params.add(Params::Mix, "Mix with Original",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(100.0);
                f.set_slider_min(0.0); f.set_slider_max(100.0);
                f.set_default(100.0); f.set_precision(1);
            }))?;

        params.add(Params::MapLayer, "Scatter Map",
            ae::LayerDef::setup(|_f| {}))?;

        params.add(Params::InvertMap, "Invert Map",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(false);
                f.set_label("Invert");
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
                    "ScatterMap v1.0\rPixel scatter with Map support.\rWritten in Rust.",
                );
            }
            ae::Command::Render { in_layer, mut out_layer } => {
                render_cpu(params, &in_data, &in_layer, &mut out_layer)?;
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

pub struct ScatterParams {
    pub amount: usize,
    pub direction: i32,
    pub random_seed: u32,
    pub repeat_edge: bool,
    pub mix: f32,
    pub invert_map: bool,
}

fn get_params(params: &ae::Parameters<Params>) -> Result<ScatterParams, ae::Error> {
    Ok(ScatterParams {
        amount: params.get(Params::Amount)?.as_slider()?.value().max(0) as usize,
        direction: params.get(Params::Direction)?.as_popup()?.value() as i32,
        random_seed: params.get(Params::RandomSeed)?.as_slider()?.value().max(0) as u32,
        repeat_edge: params.get(Params::RepeatEdge)?.as_checkbox()?.value(),
        mix: params.get(Params::Mix)?.as_float_slider()?.value().clamp(0.0, 100.0) as f32 / 100.0,
        invert_map: params.get(Params::InvertMap)?.as_checkbox()?.value(),
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

// ---- Luminance map ----

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

// ---- Legacy Render ----

fn render_cpu(
    params: &ae::Parameters<Params>,
    in_data: &ae::InData,
    in_layer: &ae::Layer,
    out_layer: &mut ae::Layer,
) -> Result<(), ae::Error> {
    let sp = get_params(params)?;
    let (src, w, h) = layer_to_flat(in_layer);

    let luma_map = get_map_layer_luma(params, in_data, w, h, sp.invert_map);
    let result = scatter::scatter(&sp, &src, w, h, luma_map.as_deref());
    flat_to_layer(&result, out_layer, w, h);
    Ok(())
}

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

// ---- SmartFX ----

const MAP_CHECKOUT_ID: i32 = 1;

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

    // Checkout map layer (param index 6, checkout_id = MAP_CHECKOUT_ID)
    let _ = cb.checkout_layer(
        6, MAP_CHECKOUT_ID,
        &req,
        in_data.current_time(), in_data.time_step(), in_data.time_scale(),
    );

    Ok(())
}

fn smart_render_cpu(
    extra: &ae::pf::SmartRenderExtra,
    params: &ae::Parameters<Params>,
) -> Result<(), ae::Error> {
    let sp = get_params(params)?;
    let cb = extra.callbacks();

    let input_world = cb.checkout_layer_pixels(0)?.ok_or(ae::Error::Generic)?;
    let mut output_world = cb.checkout_output()?.ok_or(ae::Error::Generic)?;
    let (src, w, h) = layer_to_flat(&input_world);

    let invert = sp.invert_map;
    let luma_map = if let Ok(Some(map_world)) = cb.checkout_layer_pixels(MAP_CHECKOUT_ID as u32) {
        let (map_flat, mw, mh) = layer_to_flat(&map_world);
        if mw == w && mh == h {
            Some(build_luminance_map(&map_flat, w, h, invert))
        } else {
            Some(build_luminance_map_resample(&map_flat, mw, mh, w, h, invert))
        }
    } else {
        None
    };

    let result = scatter::scatter(&sp, &src, w, h, luma_map.as_deref());
    flat_to_layer(&result, &mut output_world, w, h);

    cb.checkin_layer_pixels(0)?;
    let _ = cb.checkin_layer_pixels(MAP_CHECKOUT_ID as u32);
    Ok(())
}
