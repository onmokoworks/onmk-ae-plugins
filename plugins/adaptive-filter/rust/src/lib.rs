use after_effects as ae;

mod filters;
mod kernel;

// ---- Parameter IDs ----

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    FilterType,
    Radius,
    EdgePreserve,
    Iterations,
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
        params.add(
            Params::FilterType,
            "Filter Type",
            ae::PopupDef::setup(|f| {
                f.set_options(&["Kuwahara", "Generalized Kuwahara", "Bilateral"]);
                f.set_default(1);
            }),
        )?;

        params.add(
            Params::Radius,
            "Radius",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(1);
                f.set_valid_max(20);
                f.set_slider_min(1);
                f.set_slider_max(20);
                f.set_default(3);
            }),
        )?;

        params.add(
            Params::EdgePreserve,
            "Edge Preserve",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_default(50.0);
                f.set_precision(1);
            }),
        )?;

        params.add(
            Params::Iterations,
            "Iterations",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(1);
                f.set_valid_max(10);
                f.set_slider_min(1);
                f.set_slider_max(10);
                f.set_default(1);
            }),
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
            Params::MapLayer,
            "Luminance Map",
            ae::LayerDef::setup(|_f| {}),
        )?;

        params.add(
            Params::InvertMap,
            "Invert Map",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(false);
                f.set_label("Invert");
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
                    "AdaptiveFilter v0.1\rWorking-name plug-in for edge-aware filters with Luminance Map support.\rKuwahara, Generalized Kuwahara, and Bilateral.\rWritten in Rust.",
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
                // GPU path - fall back to CPU for now
                smart_render_cpu(&extra, params)?;
            }

            _ => {}
        }
        Ok(())
    }
}

// ---- Extract parameters ----

struct FilterParams {
    filter_type: i32,
    radius: usize,
    edge_preserve: f64,
    iterations: usize,
    mix: f64,
    invert_map: bool,
}

fn get_params(params: &ae::Parameters<Params>) -> Result<FilterParams, ae::Error> {
    Ok(FilterParams {
        filter_type: params.get(Params::FilterType)?.as_popup()?.value() as i32,
        radius: params
            .get(Params::Radius)?
            .as_slider()?
            .value()
            .clamp(1, 20) as usize,
        edge_preserve: params
            .get(Params::EdgePreserve)?
            .as_float_slider()?
            .value()
            .clamp(0.0, 100.0),
        iterations: params
            .get(Params::Iterations)?
            .as_slider()?
            .value()
            .clamp(1, 10) as usize,
        mix: params
            .get(Params::Mix)?
            .as_float_slider()?
            .value()
            .clamp(0.0, 100.0)
            / 100.0,
        invert_map: params.get(Params::InvertMap)?.as_checkbox()?.value(),
    })
}

// ---- Copy layer pixels to flat buffer ----

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

// ---- Build luminance map from a flat ARGB buffer ----

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

// ---- Process filters on flat buffer ----

fn process_filters(
    fp: &FilterParams,
    original: &[u8],
    w: usize,
    h: usize,
    luma_map: Option<&[f64]>,
) -> Vec<u8> {
    let mut buf_a = original.to_vec();
    let mut buf_b = vec![0u8; w * h * 4];

    for _ in 0..fp.iterations {
        filters::dispatch_filter(
            fp.filter_type,
            &buf_a,
            &mut buf_b,
            w,
            h,
            fp.radius,
            fp.edge_preserve,
            luma_map,
        );
        std::mem::swap(&mut buf_a, &mut buf_b);
    }

    let mut result = vec![0u8; w * h * 4];
    filters::mix_buffers(original, &buf_a, &mut result, fp.mix);
    result
}

// ---- Legacy Render path ----

fn render_cpu(
    params: &ae::Parameters<Params>,
    in_data: &ae::InData,
    in_layer: &ae::Layer,
    out_layer: &mut ae::Layer,
) -> Result<(), ae::Error> {
    let fp = get_params(params)?;
    let (original, w, h) = layer_to_flat(in_layer);

    let luma_map = get_map_layer_luma(params, in_data, w, h, fp.invert_map);

    let result = process_filters(&fp, &original, w, h, luma_map.as_deref());
    flat_to_layer(&result, out_layer, w, h);
    Ok(())
}

/// Read the map layer via legacy checkout and build a luminance map.
fn get_map_layer_luma(
    params: &ae::Parameters<Params>,
    in_data: &ae::InData,
    w: usize,
    h: usize,
    invert: bool,
) -> Option<Vec<f64>> {
    let checkout = params
        .checkout_at(Params::MapLayer, Some(in_data.current_time()), None, None)
        .ok()?;
    let layer_def = checkout.as_layer().ok()?;
    let layer = layer_def.value()?;
    let (flat, mw, mh) = layer_to_flat(&layer);
    if mw == w && mh == h {
        Some(build_luminance_map(&flat, w, h, invert))
    } else {
        Some(build_luminance_map_resample(&flat, mw, mh, w, h, invert))
    }
}

/// Resample a luminance map from (mw,mh) to (w,h) using nearest-neighbor.
fn build_luminance_map_resample(
    buf: &[u8],
    mw: usize,
    mh: usize,
    w: usize,
    h: usize,
    invert: bool,
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

// ---- SmartFX PreRender ----

const MAP_CHECKOUT_ID: i32 = 1;

fn smart_pre_render(
    in_data: &ae::InData,
    extra: &mut ae::pf::PreRenderExtra,
) -> Result<(), ae::Error> {
    let req = extra.output_request();
    let cb = extra.callbacks();

    // Checkout input layer (checkout_id = 0)
    let in_result = cb.checkout_layer(
        0,
        0,
        &req,
        in_data.current_time(),
        in_data.time_step(),
        in_data.time_scale(),
    )?;

    // Set result rects directly from checkout result — never exceed request rect
    let req_rect: ae::Rect = req.rect.into();
    let mut res: ae::Rect = in_result.result_rect.into();
    let mut max_res: ae::Rect = in_result.max_result_rect.into();

    // Clamp to request rect to ensure we never exceed it
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
        6,
        MAP_CHECKOUT_ID,
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
    let fp = get_params(params)?;
    let cb = extra.callbacks();

    let input_world = cb.checkout_layer_pixels(0)?.ok_or(ae::Error::Generic)?;
    let mut output_world = cb.checkout_output()?.ok_or(ae::Error::Generic)?;

    let (original, w, h) = layer_to_flat(&input_world);

    let invert = fp.invert_map;
    let luma_map = if let Ok(Some(map_world)) = cb.checkout_layer_pixels(MAP_CHECKOUT_ID as u32) {
        let (map_flat, mw, mh) = layer_to_flat(&map_world);
        if mw == w && mh == h {
            Some(build_luminance_map(&map_flat, w, h, invert))
        } else {
            Some(build_luminance_map_resample(
                &map_flat, mw, mh, w, h, invert,
            ))
        }
    } else {
        None
    };

    let result = process_filters(&fp, &original, w, h, luma_map.as_deref());
    flat_to_layer(&result, &mut output_world, w, h);

    cb.checkin_layer_pixels(0)?;
    let _ = cb.checkin_layer_pixels(MAP_CHECKOUT_ID as u32);
    Ok(())
}
