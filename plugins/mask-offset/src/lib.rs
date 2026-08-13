use after_effects as ae;
use std::panic::{self, AssertUnwindSafe};

// ---- Parameters ----

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Mode,
    FillColor,
    MaskIndex,
    Expansion,
    ExpansionSeparateXY,
    ExpansionY,
    CornerRound,
    Feather,
    Invert,
}

// ---- Plugin state ----

struct Plugin {
    plugin_id: Option<ae::aegp::PluginId>,
}

impl Default for Plugin {
    fn default() -> Self {
        Self { plugin_id: None }
    }
}

/// Data passed from SmartPreRender to SmartRender.
struct PreRenderMaskData {
    polygon: Vec<(f64, f64)>,
}

ae::define_effect!(Plugin, (), Params);

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _in_data: ae::InData,
        _out_data: ae::OutData,
    ) -> Result<(), ae::Error> {
        params.add(
            Params::Mode,
            "Mode",
            ae::PopupDef::setup(|f| {
                f.set_options(&[
                    "Mask Alpha",
                    "Show Inside Only",
                    "Fill Inside",
                    "Show Outside Only",
                    "Fill Outside",
                ]);
                f.set_default(1);
            }),
        )?;

        params.add(
            Params::FillColor,
            "Fill Color",
            ae::ColorDef::setup(|f| {
                f.set_default(ae::Pixel8 { alpha: 255, red: 255, green: 255, blue: 255 });
            }),
        )?;

        params.add(
            Params::MaskIndex,
            "Mask Index",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(1.0);
                f.set_valid_max(128.0);
                f.set_slider_min(1.0);
                f.set_slider_max(16.0);
                f.set_default(1.0);
                f.set_precision(0);
            }),
        )?;

        params.add(
            Params::Expansion,
            "Expansion (px)",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(-5000.0);
                f.set_valid_max(5000.0);
                f.set_slider_min(-500.0);
                f.set_slider_max(500.0);
                f.set_default(0.0);
                f.set_precision(1);
            }),
        )?;

        params.add(
            Params::ExpansionSeparateXY,
            "Separate X/Y",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(false);
                f.set_label("Separate Expansion X/Y");
            }),
        )?;

        params.add(
            Params::ExpansionY,
            "Expansion Y (px)",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(-5000.0);
                f.set_valid_max(5000.0);
                f.set_slider_min(-500.0);
                f.set_slider_max(500.0);
                f.set_default(0.0);
                f.set_precision(1);
            }),
        )?;

        params.add(
            Params::CornerRound,
            "Corner Round (px)",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(2000.0);
                f.set_slider_min(0.0);
                f.set_slider_max(500.0);
                f.set_default(0.0);
                f.set_precision(1);
            }),
        )?;

        params.add(
            Params::Feather,
            "Feather (px)",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(500.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_default(0.0);
                f.set_precision(1);
            }),
        )?;

        params.add(
            Params::Invert,
            "Invert",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(false);
                f.set_label("Invert Mask");
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
                    "ONMK MaskOffset v1.0\rOffset mask path inward/outward with XY control.\rWritten in Rust.",
                );
            }

            ae::Command::GlobalSetup => {
                out_data.set_out_flag(ae::OutFlags::NonParamVary, true);
                out_data.set_out_flag2(ae::OutFlags2::DependsOnUnreferencedMasks, true);
                if let Ok(utility) = ae::aegp::suites::Utility::new() {
                    self.plugin_id = utility.register_with_aegp("MaskOffset").ok();
                }
            }

            ae::Command::SmartPreRender { mut extra } => {
                let plugin_id = self.plugin_id;
                let polygon = panic::catch_unwind(AssertUnwindSafe(|| {
                    compute_offset_polygon(&in_data, plugin_id, params)
                })).unwrap_or_default();

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

                extra.set_pre_render_data(PreRenderMaskData { polygon });
            }

            ae::Command::SmartRender { extra } => {
                let polygon = extra.pre_render_data::<PreRenderMaskData>()
                    .map(|d| d.polygon.clone())
                    .unwrap_or_default();
                let _ = panic::catch_unwind(AssertUnwindSafe(|| {
                    let _ = smart_render(&extra, params, &polygon);
                }));
            }

            ae::Command::SmartRenderGpu { extra } => {
                let polygon = extra.pre_render_data::<PreRenderMaskData>()
                    .map(|d| d.polygon.clone())
                    .unwrap_or_default();
                let _ = panic::catch_unwind(AssertUnwindSafe(|| {
                    let _ = smart_render(&extra, params, &polygon);
                }));
            }

            _ => {}
        }
        Ok(())
    }
}

// ---- Compute offset polygon ----

fn get_float(params: &ae::Parameters<Params>, id: Params, default: f64) -> f64 {
    match params.get(id) {
        Ok(p) => match p.as_float_slider() { Ok(s) => s.value(), Err(_) => default },
        Err(_) => default,
    }
}

fn get_bool(params: &ae::Parameters<Params>, id: Params, default: bool) -> bool {
    match params.get(id) {
        Ok(p) => match p.as_checkbox() { Ok(s) => s.value(), Err(_) => default },
        Err(_) => default,
    }
}

fn compute_offset_polygon(
    in_data: &ae::InData,
    plugin_id: Option<ae::aegp::PluginId>,
    params: &ae::Parameters<Params>,
) -> Vec<(f64, f64)> {
    let mask_index = get_float(params, Params::MaskIndex, 1.0) as i32;
    let expansion_x = get_float(params, Params::Expansion, 0.0);
    let separate_xy = get_bool(params, Params::ExpansionSeparateXY, false);
    let expansion_y = if separate_xy {
        get_float(params, Params::ExpansionY, expansion_x)
    } else {
        expansion_x
    };
    let corner_round = get_float(params, Params::CornerRound, 0.0);

    let plugin_id_val = match plugin_id {
        Some(id) => id,
        None => return Vec::new(),
    };

    let pf_iface = ae::aegp::suites::PFInterface::new().ok();
    let layer = pf_iface.as_ref()
        .and_then(|s| s.effect_layer(in_data.effect_ref()).ok());
    let layer = match layer { Some(l) => l, None => return Vec::new() };

    let mask_suite = match ae::aegp::suites::Mask::new() {
        Ok(s) => s, Err(_) => return Vec::new(),
    };
    let stream_suite = match ae::aegp::suites::Stream::new() {
        Ok(s) => s, Err(_) => return Vec::new(),
    };

    let num_masks = mask_suite.layer_num_masks(&layer).unwrap_or(0);
    if num_masks == 0 { return Vec::new(); }
    let mi = (mask_index - 1).max(0).min(num_masks - 1);
    let mask = match mask_suite.layer_mask_by_index(&layer, mi) {
        Ok(m) => m, Err(_) => return Vec::new(),
    };
    let mask_stream = match stream_suite.new_mask_stream(
        &mask, plugin_id_val, ae::aegp::MaskStream::Outline,
    ) {
        Ok(s) => s, Err(_) => return Vec::new(),
    };

    let (vertices, is_closed) = unsafe {
        read_outline_raw_ffi(in_data, &mask_stream, plugin_id_val)
    };
    if vertices.is_empty() { return Vec::new(); }

    let mut vertices = vertices;
    apply_corner_rounding(&mut vertices, is_closed, corner_round);

    // Apply expansion
    if expansion_x.abs() > 0.001 || expansion_y.abs() > 0.001 {
        let n = vertices.len() as f64;
        let cx: f64 = vertices.iter().map(|v| v.x).sum::<f64>() / n;
        let cy: f64 = vertices.iter().map(|v| v.y).sum::<f64>() / n;
        for vtx in &mut vertices {
            let dx = vtx.x - cx;
            let dy = vtx.y - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist > 0.001 {
                vtx.x += (dx / dist) * expansion_x;
                vtx.y += (dy / dist) * expansion_y;
            }
        }
    }

    // Sample bezier path densely into a polygon
    sample_polygon(&vertices, is_closed)
}

// ---- Mode constants (1-indexed from popup) ----
const MODE_MASK_ALPHA: i32 = 1;
const MODE_SHOW_INSIDE: i32 = 2;
const MODE_FILL_INSIDE: i32 = 3;
const MODE_SHOW_OUTSIDE: i32 = 4;
const MODE_FILL_OUTSIDE: i32 = 5;

// ---- SmartRender ----

fn smart_render(
    extra: &ae::pf::SmartRenderExtra,
    params: &ae::Parameters<Params>,
    polygon: &[(f64, f64)],
) -> Result<(), ae::Error> {
    let mode = match params.get(Params::Mode) {
        Ok(p) => match p.as_popup() { Ok(s) => s.value() as i32, Err(_) => 1 },
        Err(_) => 1,
    };
    let fill_color = match params.get(Params::FillColor) {
        Ok(p) => match p.as_color() { Ok(c) => c.value(), Err(_) => ae::Pixel8 { alpha: 255, red: 255, green: 255, blue: 255 } },
        Err(_) => ae::Pixel8 { alpha: 255, red: 255, green: 255, blue: 255 },
    };
    let feather = get_float(params, Params::Feather, 0.0);
    let invert = get_bool(params, Params::Invert, false);

    let cb = extra.callbacks();
    let input_world = cb.checkout_layer_pixels(0)?.ok_or(ae::Error::Generic)?;
    let mut output_world = cb.checkout_output()?.ok_or(ae::Error::Generic)?;

    let w = output_world.width() as usize;
    let h = output_world.height() as usize;
    if w == 0 || h == 0 {
        let _ = cb.checkin_layer_pixels(0);
        return Ok(());
    }

    // Copy input to output first
    copy_layer(&input_world, &mut output_world);

    if polygon.is_empty() {
        let _ = cb.checkin_layer_pixels(0);
        return Ok(());
    }

    let stride = output_world.buffer_stride();
    let buf = output_world.buffer_mut();
    let depth = input_world.bit_depth();

    for y in 0..h {
        let row_off = y * stride;
        for x in 0..w {
            let inside_raw = point_in_polygon(x as f64 + 0.5, y as f64 + 0.5, polygon);
            let mut mask_val = if inside_raw { 1.0 } else { 0.0 };

            if feather > 0.5 {
                let dist = approx_dist_to_polygon(x as f64 + 0.5, y as f64 + 0.5, polygon);
                mask_val = if inside_raw { (dist / feather).min(1.0) } else { 0.0 };
            }

            if invert { mask_val = 1.0 - mask_val; }

            match mode {
                MODE_MASK_ALPHA => {
                    // Multiply alpha only
                    multiply_alpha(buf, row_off, x, depth, mask_val);
                }
                MODE_SHOW_INSIDE => {
                    // Inside: keep pixel, Outside: transparent
                    multiply_alpha(buf, row_off, x, depth, mask_val);
                }
                MODE_FILL_INSIDE => {
                    // Inside mask: replace with fill color
                    if mask_val > 0.001 {
                        write_pixel(buf, row_off, x, depth, fill_color, mask_val);
                    }
                }
                MODE_SHOW_OUTSIDE => {
                    // Outside: keep pixel, Inside: transparent
                    multiply_alpha(buf, row_off, x, depth, 1.0 - mask_val);
                }
                MODE_FILL_OUTSIDE => {
                    // Outside mask: replace with fill color
                    let outside = 1.0 - mask_val;
                    if outside > 0.001 {
                        write_pixel(buf, row_off, x, depth, fill_color, outside);
                    }
                }
                _ => {
                    multiply_alpha(buf, row_off, x, depth, mask_val);
                }
            }
        }
    }

    let _ = cb.checkin_layer_pixels(0);
    Ok(())
}

fn multiply_alpha(buf: &mut [u8], row_off: usize, x: usize, depth: i16, factor: f64) {
    match depth {
        16 => {
            let off = row_off + x * 8;
            if off + 1 < buf.len() {
                let v = u16::from_ne_bytes([buf[off], buf[off + 1]]);
                let bytes = ((v as f64 * factor) as u16).to_ne_bytes();
                buf[off] = bytes[0]; buf[off + 1] = bytes[1];
            }
        }
        32 => {
            let off = row_off + x * 16;
            if off + 3 < buf.len() {
                let v = f32::from_ne_bytes([buf[off], buf[off+1], buf[off+2], buf[off+3]]);
                let bytes = (v * factor as f32).to_ne_bytes();
                buf[off] = bytes[0]; buf[off+1] = bytes[1]; buf[off+2] = bytes[2]; buf[off+3] = bytes[3];
            }
        }
        _ => {
            let off = row_off + x * 4;
            if off < buf.len() {
                buf[off] = (buf[off] as f64 * factor) as u8;
            }
        }
    }
}

fn write_pixel(buf: &mut [u8], row_off: usize, x: usize, depth: i16, color: ae::Pixel8, factor: f64) {
    match depth {
        16 => {
            let off = row_off + x * 8;
            if off + 7 < buf.len() {
                let f = factor as f64;
                let orig_a = u16::from_ne_bytes([buf[off], buf[off+1]]) as f64;
                let orig_r = u16::from_ne_bytes([buf[off+2], buf[off+3]]) as f64;
                let orig_g = u16::from_ne_bytes([buf[off+4], buf[off+5]]) as f64;
                let orig_b = u16::from_ne_bytes([buf[off+6], buf[off+7]]) as f64;
                let ca = color.alpha as f64 / 255.0 * 32768.0;
                let cr = color.red as f64 / 255.0 * 32768.0;
                let cg = color.green as f64 / 255.0 * 32768.0;
                let cb = color.blue as f64 / 255.0 * 32768.0;
                let inv = 1.0 - f;
                for (i, (cv, ov)) in [(ca, orig_a), (cr, orig_r), (cg, orig_g), (cb, orig_b)].iter().enumerate() {
                    let v = (cv * f + ov * inv) as u16;
                    let bytes = v.to_ne_bytes();
                    buf[off + i * 2] = bytes[0]; buf[off + i * 2 + 1] = bytes[1];
                }
            }
        }
        32 => {
            let off = row_off + x * 16;
            if off + 15 < buf.len() {
                let f = factor as f32;
                let inv = 1.0 - f;
                let colors = [color.alpha as f32 / 255.0, color.red as f32 / 255.0, color.green as f32 / 255.0, color.blue as f32 / 255.0];
                for ch in 0..4 {
                    let co = ch * 4;
                    let orig = f32::from_ne_bytes([buf[off+co], buf[off+co+1], buf[off+co+2], buf[off+co+3]]);
                    let v = colors[ch] * f + orig * inv;
                    let bytes = v.to_ne_bytes();
                    buf[off+co] = bytes[0]; buf[off+co+1] = bytes[1]; buf[off+co+2] = bytes[2]; buf[off+co+3] = bytes[3];
                }
            }
        }
        _ => {
            // 8-bit ARGB
            let off = row_off + x * 4;
            if off + 3 < buf.len() {
                let f = factor;
                let inv = 1.0 - f;
                buf[off]   = (color.alpha as f64 * f + buf[off]   as f64 * inv) as u8;
                buf[off+1] = (color.red   as f64 * f + buf[off+1] as f64 * inv) as u8;
                buf[off+2] = (color.green as f64 * f + buf[off+2] as f64 * inv) as u8;
                buf[off+3] = (color.blue  as f64 * f + buf[off+3] as f64 * inv) as u8;
            }
        }
    }
}

// ---- Copy layer pixels ----

fn copy_layer(src: &ae::Layer, dst: &mut ae::Layer) {
    let w = src.width().min(dst.width()) as usize;
    let h = src.height().min(dst.height()) as usize;
    let src_stride = src.buffer_stride();
    let dst_stride = dst.buffer_stride();
    let src_buf = src.buffer();
    let dst_buf = dst.buffer_mut();
    let bpp = match src.bit_depth() { 16 => 8, 32 => 16, _ => 4 };
    let row_bytes = w * bpp;
    for y in 0..h {
        let so = y * src_stride;
        let do_ = y * dst_stride;
        if so + row_bytes <= src_buf.len() && do_ + row_bytes <= dst_buf.len() {
            dst_buf[do_..do_ + row_bytes].copy_from_slice(&src_buf[so..so + row_bytes]);
        }
    }
}

// ---- Point-in-polygon (ray casting) ----

fn point_in_polygon(px: f64, py: f64, poly: &[(f64, f64)]) -> bool {
    let n = poly.len();
    if n < 3 { return false; }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = poly[i];
        let (xj, yj) = poly[j];
        if ((yi > py) != (yj > py)) && (px < (xj - xi) * (py - yi) / (yj - yi) + xi) {
            inside = !inside;
        }
        j = i;
    }
    inside
}

// ---- Approximate distance to polygon edge ----

fn approx_dist_to_polygon(px: f64, py: f64, poly: &[(f64, f64)]) -> f64 {
    let n = poly.len();
    if n < 2 { return f64::MAX; }
    let mut min_dist = f64::MAX;
    let mut j = n - 1;
    for i in 0..n {
        let (x1, y1) = poly[j];
        let (x2, y2) = poly[i];
        let dx = x2 - x1;
        let dy = y2 - y1;
        let len_sq = dx * dx + dy * dy;
        let t = if len_sq > 0.001 {
            ((px - x1) * dx + (py - y1) * dy) / len_sq
        } else { 0.0 }.clamp(0.0, 1.0);
        let cx = x1 + t * dx;
        let cy = y1 + t * dy;
        let d = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
        min_dist = min_dist.min(d);
        j = i;
    }
    min_dist
}

// ---- Sample bezier vertices into dense polygon ----

fn sample_polygon(vertices: &[MaskVertex], is_closed: bool) -> Vec<(f64, f64)> {
    if vertices.is_empty() { return Vec::new(); }
    let samples_per_seg = 32;
    let seg_count = if is_closed { vertices.len() } else { vertices.len().saturating_sub(1) };
    let mut poly = Vec::with_capacity(seg_count * samples_per_seg);

    for seg in 0..seg_count {
        let v0 = &vertices[seg];
        let v1 = if is_closed { &vertices[(seg + 1) % vertices.len()] } else { &vertices[seg + 1] };
        let p0 = (v0.x, v0.y);
        let p1 = (v0.x + v0.tan_out_x, v0.y + v0.tan_out_y);
        let p2 = (v1.x + v1.tan_in_x, v1.y + v1.tan_in_y);
        let p3 = (v1.x, v1.y);

        for si in 0..samples_per_seg {
            let t = si as f64 / samples_per_seg as f64;
            poly.push(cubic_bezier(p0, p1, p2, p3, t));
        }
    }
    if !is_closed {
        if let Some(last) = vertices.last() {
            poly.push((last.x, last.y));
        }
    }
    poly
}

fn cubic_bezier(p0: (f64, f64), p1: (f64, f64), p2: (f64, f64), p3: (f64, f64), t: f64) -> (f64, f64) {
    let u = 1.0 - t;
    let uu = u * u;
    let tt = t * t;
    (
        uu * u * p0.0 + 3.0 * uu * t * p1.0 + 3.0 * u * tt * p2.0 + tt * t * p3.0,
        uu * u * p0.1 + 3.0 * uu * t * p1.1 + 3.0 * u * tt * p2.1 + tt * t * p3.1,
    )
}

// ---- Vertex + corner rounding (shared with PathArray) ----

#[derive(Clone)]
struct MaskVertex {
    x: f64, y: f64,
    tan_in_x: f64, tan_in_y: f64,
    tan_out_x: f64, tan_out_y: f64,
}

fn apply_corner_rounding(vertices: &mut Vec<MaskVertex>, is_closed: bool, round_px: f64) {
    if round_px < 0.5 || vertices.len() < 2 { return; }
    let n = vertices.len();
    let orig = vertices.clone();
    let mut result: Vec<MaskVertex> = Vec::with_capacity(n * 2);

    for i in 0..n {
        let vtx = &orig[i];
        let tan_in_mag = (vtx.tan_in_x.powi(2) + vtx.tan_in_y.powi(2)).sqrt();
        let tan_out_mag = (vtx.tan_out_x.powi(2) + vtx.tan_out_y.powi(2)).sqrt();
        let has_prev = i > 0 || is_closed;
        let has_next = i + 1 < n || is_closed;

        if !has_prev || !has_next || (tan_in_mag > round_px * 0.5 && tan_out_mag > round_px * 0.5) {
            result.push(vtx.clone());
            continue;
        }

        let prev = if i > 0 { &orig[i - 1] } else { &orig[n - 1] };
        let next = if i + 1 < n { &orig[i + 1] } else { &orig[0] };
        let dx_in = prev.x - vtx.x;
        let dy_in = prev.y - vtx.y;
        let dist_in = (dx_in * dx_in + dy_in * dy_in).sqrt();
        let dx_out = next.x - vtx.x;
        let dy_out = next.y - vtx.y;
        let dist_out = (dx_out * dx_out + dy_out * dy_out).sqrt();

        if dist_in < 0.001 || dist_out < 0.001 {
            result.push(vtx.clone());
            continue;
        }

        let pull_in = round_px.min(dist_in * 0.45);
        let pull_out = round_px.min(dist_out * 0.45);
        let dir_in_x = dx_in / dist_in;
        let dir_in_y = dy_in / dist_in;
        let dir_out_x = dx_out / dist_out;
        let dir_out_y = dy_out / dist_out;
        let kappa = 0.5523;

        result.push(MaskVertex {
            x: vtx.x + dir_in_x * pull_in, y: vtx.y + dir_in_y * pull_in,
            tan_in_x: 0.0, tan_in_y: 0.0,
            tan_out_x: -dir_in_x * pull_in * kappa, tan_out_y: -dir_in_y * pull_in * kappa,
        });
        result.push(MaskVertex {
            x: vtx.x + dir_out_x * pull_out, y: vtx.y + dir_out_y * pull_out,
            tan_in_x: -dir_out_x * pull_out * kappa, tan_in_y: -dir_out_y * pull_out * kappa,
            tan_out_x: 0.0, tan_out_y: 0.0,
        });
    }
    *vertices = result;
}

// ---- Direct FFI mask outline reading (same as PathArray) ----

unsafe fn read_outline_raw_ffi(
    in_data: &ae::InData,
    mask_stream: &ae::aegp::StreamReferenceHandle,
    plugin_id: ae::aegp::PluginId,
) -> (Vec<MaskVertex>, bool) {
    use ae::sys::*;

    let pica = in_data.pica_basic_suite_ptr();
    if pica.is_null() { return (Vec::new(), true); }

    let acquire = match (*pica).AcquireSuite {
        Some(f) => f,
        None => return (Vec::new(), true),
    };
    let release = (*pica).ReleaseSuite;

    let stream_suite_name = kAEGPStreamSuite.as_ptr() as *const std::os::raw::c_char;
    let stream_suite_version = kAEGPStreamSuiteVersion6 as i32;
    let mut stream_suite_ptr: *const std::ffi::c_void = std::ptr::null();
    if acquire(stream_suite_name, stream_suite_version, &mut stream_suite_ptr) != 0 || stream_suite_ptr.is_null() {
        return (Vec::new(), true);
    }
    let stream_suite = &*(stream_suite_ptr as *const AEGP_StreamSuite6);

    let outline_suite_name = kAEGPMaskOutlineSuite.as_ptr() as *const std::os::raw::c_char;
    let outline_suite_version = kAEGPMaskOutlineSuiteVersion3 as i32;
    let mut outline_suite_ptr: *const std::ffi::c_void = std::ptr::null();
    if acquire(outline_suite_name, outline_suite_version, &mut outline_suite_ptr) != 0 || outline_suite_ptr.is_null() {
        if let Some(r) = release { r(stream_suite_name, stream_suite_version); }
        return (Vec::new(), true);
    }
    let outline_suite = &*(outline_suite_ptr as *const AEGP_MaskOutlineSuite3);

    let time = A_Time { value: in_data.current_time(), scale: in_data.time_scale() };
    let mut stream_val2: AEGP_StreamValue2 = std::mem::zeroed();
    let get_val_fn = match stream_suite.AEGP_GetNewStreamValue {
        Some(f) => f,
        None => {
            if let Some(r) = release { r(outline_suite_name, outline_suite_version); r(stream_suite_name, stream_suite_version); }
            return (Vec::new(), true);
        }
    };

    let err = get_val_fn(
        plugin_id,
        <ae::aegp::StreamReferenceHandle as after_effects::AsPtr<_>>::as_ptr(mask_stream),
        AEGP_LTimeMode_LayerTime as AEGP_LTimeMode,
        &time as *const A_Time, 0, &mut stream_val2,
    );
    if err != 0 {
        if let Some(r) = release { r(outline_suite_name, outline_suite_version); r(stream_suite_name, stream_suite_version); }
        return (Vec::new(), true);
    }

    let outline_h: AEGP_MaskOutlineValH = stream_val2.val.mask;
    if outline_h.is_null() {
        if let Some(dispose) = stream_suite.AEGP_DisposeStreamValue { dispose(&mut stream_val2); }
        if let Some(r) = release { r(outline_suite_name, outline_suite_version); r(stream_suite_name, stream_suite_version); }
        return (Vec::new(), true);
    }

    let mut num_segments: A_long = 0;
    if let Some(f) = outline_suite.AEGP_GetMaskOutlineNumSegments {
        f(outline_h, &mut num_segments);
    }

    let mut vertices: Vec<MaskVertex> = Vec::new();
    if num_segments > 0 {
        if let Some(vf) = outline_suite.AEGP_GetMaskOutlineVertexInfo {
            for i in 0..num_segments {
                let mut vtx: AEGP_MaskVertex = std::mem::zeroed();
                if vf(outline_h, i, &mut vtx) == 0 {
                    vertices.push(MaskVertex {
                        x: vtx.x, y: vtx.y,
                        tan_in_x: vtx.tan_in_x, tan_in_y: vtx.tan_in_y,
                        tan_out_x: vtx.tan_out_x, tan_out_y: vtx.tan_out_y,
                    });
                }
            }
        }
    }

    let mut is_open: A_Boolean = 0;
    if let Some(f) = outline_suite.AEGP_IsMaskOutlineOpen {
        let _ = f(outline_h, &mut is_open);
    }

    if let Some(dispose) = stream_suite.AEGP_DisposeStreamValue { dispose(&mut stream_val2); }
    if let Some(r) = release { r(outline_suite_name, outline_suite_version); r(stream_suite_name, stream_suite_version); }

    (vertices, is_open == 0)
}

