use after_effects as ae;
use std::sync::OnceLock;

mod gpu;
mod slit_scan;

use slit_scan::{MODE_HORIZONTAL, MODE_MAP_LAYER};

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
    TimeFrames,
    FrameStep,
    TimeDirection,
    SliceMode,
    GradientPhase,
    Center,
    Interpolation,
    Mix,
    MapLayer,
    InvertMap,
}

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
            Params::TimeFrames,
            "Time Frames",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(2);
                f.set_valid_max(120);
                f.set_slider_min(2);
                f.set_slider_max(60);
                f.set_default(30);
            }),
        )?;

        params.add(
            Params::FrameStep,
            "Frame Step",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(1);
                f.set_valid_max(10);
                f.set_slider_min(1);
                f.set_slider_max(5);
                f.set_default(1);
            }),
        )?;

        params.add(
            Params::TimeDirection,
            "Time Direction",
            ae::PopupDef::setup(|f| {
                f.set_options(&["Past", "Future", "Both"]);
                f.set_default(1);
            }),
        )?;

        params.add(
            Params::SliceMode,
            "Slice Mode",
            ae::PopupDef::setup(|f| {
                f.set_options(&["Horizontal", "Vertical", "Radial", "Map Layer"]);
                f.set_default(1);
            }),
        )?;

        params.add(
            Params::GradientPhase,
            "Gradient Phase",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(-100.0);
                f.set_valid_max(100.0);
                f.set_slider_min(-100.0);
                f.set_slider_max(100.0);
                f.set_default(0.0);
                f.set_precision(1);
            }),
        )?;

        params.add(
            Params::Center,
            "Center",
            ae::PointDef::setup(|f| {
                f.set_default((50.0, 50.0));
                f.set_restrict_bounds(false);
            }),
        )?;

        params.add(
            Params::Interpolation,
            "Interpolation",
            ae::PopupDef::setup(|f| {
                f.set_options(&["Nearest", "Linear"]);
                f.set_default(2);
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

        params.add(Params::MapLayer, "Time Map", ae::LayerDef::setup(|_f| {}))?;

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
                    "FrameSlice v1.0.0\rSlit-scan effect with map support.\rGPU accelerated. Written in Rust.",
                );
            }
            ae::Command::Render {
                in_layer,
                mut out_layer,
            } => {
                let (src, w, h) = layer_to_flat(&in_layer);
                flat_to_layer(&src, &mut out_layer, w, h);
            }
            ae::Command::SmartPreRender { mut extra } => {
                let time_frames =
                    params.get(Params::TimeFrames)?.as_slider()?.value().max(2) as usize;
                let frame_step =
                    params.get(Params::FrameStep)?.as_slider()?.value().max(1) as usize;
                let time_direction = params.get(Params::TimeDirection)?.as_popup()?.value() as i32;
                let actual_frames = calc_actual_frames(time_frames, frame_step);
                smart_pre_render(
                    &in_data,
                    &mut extra,
                    time_frames,
                    actual_frames,
                    time_direction,
                )?;
            }
            ae::Command::SmartRender { extra } => {
                smart_render(&extra, params, &in_data)?;
            }
            _ => {}
        }
        Ok(())
    }
}

fn calc_actual_frames(time_frames: usize, frame_step: usize) -> usize {
    if frame_step <= 1 {
        time_frames
    } else {
        ((time_frames - 1) / frame_step) + 1
    }
}

// ---- Extract parameters ----

pub struct SlitScanParams {
    pub time_frames: usize,
    pub frame_step: usize,
    pub time_direction: i32,
    pub slice_mode: i32,
    pub gradient_phase: f64,
    pub center: (f64, f64),
    pub interpolation: i32,
    pub mix: f32,
    pub invert_map: bool,
}

fn get_params(
    params: &ae::Parameters<Params>,
    _in_data: &ae::InData,
) -> Result<SlitScanParams, ae::Error> {
    let (cx, cy) = params.get(Params::Center)?.as_point()?.value();
    Ok(SlitScanParams {
        time_frames: params.get(Params::TimeFrames)?.as_slider()?.value().max(2) as usize,
        frame_step: params.get(Params::FrameStep)?.as_slider()?.value().max(1) as usize,
        time_direction: params.get(Params::TimeDirection)?.as_popup()?.value() as i32,
        slice_mode: params.get(Params::SliceMode)?.as_popup()?.value() as i32,
        gradient_phase: params
            .get(Params::GradientPhase)?
            .as_float_slider()?
            .value()
            / 100.0,
        center: (cx as f64 / 100.0, cy as f64 / 100.0),
        interpolation: params.get(Params::Interpolation)?.as_popup()?.value() as i32,
        mix: params
            .get(Params::Mix)?
            .as_float_slider()?
            .value()
            .clamp(0.0, 100.0) as f32
            / 100.0,
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

// ---- SmartFX ----

const MAP_CHECKOUT_ID: i32 = 1;
const FRAME_BASE_ID: i32 = 10;
const MAP_PARAM_INDEX: i32 = 9;

fn compute_frame_time(
    current_time: i32,
    time_step: i32,
    time_frames: usize,
    actual_frames: usize,
    direction: i32,
    frame_index: usize,
) -> i32 {
    let t = if actual_frames <= 1 {
        0.0
    } else {
        frame_index as f64 / (actual_frames - 1) as f64
    };
    let range = (time_frames - 1) as f64;

    match direction {
        2 => {
            let offset = (t * range).round() as i32;
            current_time + offset * time_step
        }
        3 => {
            let half = (range / 2.0).round() as i32;
            let offset = (t * range).round() as i32 - half;
            current_time + offset * time_step
        }
        _ => {
            let offset = ((1.0 - t) * range).round() as i32;
            current_time - offset * time_step
        }
    }
}

fn smart_pre_render(
    in_data: &ae::InData,
    extra: &mut ae::pf::PreRenderExtra,
    time_frames: usize,
    actual_frames: usize,
    time_direction: i32,
) -> Result<(), ae::Error> {
    let req = extra.output_request();
    let cb = extra.callbacks();

    let current_time = in_data.current_time();
    let time_step = in_data.time_step();
    let time_scale = in_data.time_scale();

    let in_result = cb.checkout_layer(0, 0, &req, current_time, time_step, time_scale)?;

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

    for i in 0..actual_frames {
        let frame_time = compute_frame_time(
            current_time,
            time_step,
            time_frames,
            actual_frames,
            time_direction,
            i,
        );
        let checkout_id = FRAME_BASE_ID + i as i32;
        let _ = cb.checkout_layer(0, checkout_id, &req, frame_time, time_step, time_scale);
    }

    let _ = cb.checkout_layer(
        MAP_PARAM_INDEX,
        MAP_CHECKOUT_ID,
        &req,
        current_time,
        time_step,
        time_scale,
    );

    extra.set_result_rect(res);
    extra.set_max_result_rect(max_res);
    Ok(())
}

fn smart_render(
    extra: &ae::pf::SmartRenderExtra,
    params: &ae::Parameters<Params>,
    in_data: &ae::InData,
) -> Result<(), ae::Error> {
    let sp = get_params(params, in_data)?;
    let cb = extra.callbacks();

    let input_world = cb.checkout_layer_pixels(0)?.ok_or(ae::Error::Generic)?;
    let mut output_world = cb.checkout_output()?.ok_or(ae::Error::Generic)?;
    let (src, w, h) = layer_to_flat(&input_world);

    if w == 0 || h == 0 {
        flat_to_layer(&src, &mut output_world, w, h);
        cb.checkin_layer_pixels(0)?;
        return Ok(());
    }

    let actual_frames = calc_actual_frames(sp.time_frames, sp.frame_step);
    let mut frames: Vec<Vec<u8>> = Vec::with_capacity(actual_frames);
    for i in 0..actual_frames {
        let checkout_id = (FRAME_BASE_ID + i as i32) as u32;
        if let Ok(Some(frame_world)) = cb.checkout_layer_pixels(checkout_id) {
            let (flat, _, _) = layer_to_flat(&frame_world);
            frames.push(flat);
        } else {
            frames.push(src.clone());
        }
    }

    let map = if sp.slice_mode == MODE_MAP_LAYER {
        if let Ok(Some(map_world)) = cb.checkout_layer_pixels(MAP_CHECKOUT_ID as u32) {
            let (map_flat, mw, mh) = layer_to_flat(&map_world);
            if mw == w && mh == h {
                build_luminance_map(&map_flat, w, h, sp.invert_map)
            } else {
                build_luminance_map_resample(&map_flat, mw, mh, w, h, sp.invert_map)
            }
        } else {
            slit_scan::generate_gradient_map(MODE_HORIZONTAL, w, h, 0.0, (0.5, 0.5), sp.invert_map)
        }
    } else {
        slit_scan::generate_gradient_map(
            sp.slice_mode,
            w,
            h,
            sp.gradient_phase,
            sp.center,
            sp.invert_map,
        )
    };

    let result = get_gpu()
        .and_then(|g| g.process(&sp, &frames, w, h, &map, &src))
        .unwrap_or_else(|| slit_scan::render(&sp, &frames, w, h, &map, &src));
    flat_to_layer(&result, &mut output_world, w, h);

    cb.checkin_layer_pixels(0)?;
    for i in 0..actual_frames {
        let _ = cb.checkin_layer_pixels((FRAME_BASE_ID + i as i32) as u32);
    }
    let _ = cb.checkin_layer_pixels(MAP_CHECKOUT_ID as u32);

    Ok(())
}
