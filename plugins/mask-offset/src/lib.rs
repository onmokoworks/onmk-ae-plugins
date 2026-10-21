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
                smart_render(&extra, params, &polygon)?;
            }

            ae::Command::SmartRenderGpu { extra } => {
                let polygon = extra.pre_render_data::<PreRenderMaskData>()
                    .map(|d| d.polygon.clone())
                    .unwrap_or_default();
                smart_render(&extra, params, &polygon)?;
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

    // Offset the sampled contour, not anchors along centroid rays. Tangent
    // direction and winding, rather than aspect ratio, determine the offset.
    let polygon = sample_polygon(&vertices, is_closed);
    offset_polygon(&polygon, expansion_x, expansion_y)
}

fn offset_polygon(polygon: &[(f64,f64)], ex: f64, ey: f64) -> Vec<(f64,f64)> {
    if ex.abs() <= 0.001 && ey.abs() <= 0.001 { return polygon.to_vec(); }
    // A nonempty degenerate contour means an empty mask, not missing-mask passthrough.
    let empty = || vec![(0.0,0.0);3];
    if !ex.is_finite() || !ey.is_finite() { return empty(); }
    let mut points = Vec::new();
    for &p in polygon {
        if !p.0.is_finite() || !p.1.is_finite() { return empty(); }
        if points.last().map_or(true, |q:&(f64,f64)| (p.0-q.0).hypot(p.1-q.1)>1e-9) {
            points.push(p);
        }
    }
    if points.len()>1 && (points[0].0-points[points.len()-1].0).hypot(points[0].1-points[points.len()-1].1)<=1e-9 {
        points.pop();
    }
    if points.len()<3 { return empty(); }
    let n=points.len();
    let cross=|a:(f64,f64),b:(f64,f64)| a.0*b.1-a.1*b.0;
    let area: f64=(0..n).map(|i|cross(points[i],points[(i+1)%n])).sum();
    if area.abs()<1e-9 || !area.is_finite() { return empty(); }
    let winding=area.signum();
    let normals: Vec<_>=(0..n).map(|i| {
        let a=points[i]; let b=points[(i+1)%n];
        let (dx,dy)=(b.0-a.0,b.1-a.1); let length=dx.hypot(dy);
        (winding*dy/length,-winding*dx/length)
    }).collect();
    let distance=|normal:(f64,f64)| ex*normal.0*normal.0+ey*normal.1*normal.1;
    let convex=(0..n).all(|i| cross(normals[i],normals[(i+1)%n])*winding>=-1e-9);
    if convex && ex<=0.0 && ey<=0.0 {
        // Half-plane clipping makes erosion disappear instead of turning an
        // over-shrunk convex contour inside out and making it grow again.
        let mut out=points.clone();
        for i in 0..n {
            let normal=normals[i]; let origin=points[i];
            let side=|p:(f64,f64)| normal.0*(p.0-origin.0)+normal.1*(p.1-origin.1)-distance(normal);
            let mut clipped=Vec::new();
            if out.is_empty() { return empty(); }
            let mut a=*out.last().unwrap(); let mut sa=side(a);
            for &b in &out {
                let sb=side(b);
                if (sa<=0.0)!=(sb<=0.0) {
                    let t=sa/(sa-sb);
                    clipped.push((a.0+(b.0-a.0)*t,a.1+(b.1-a.1)*t));
                }
                if sb<=0.0 { clipped.push(b); }
                a=b; sa=sb;
            }
            out=clipped;
        }
        return if out.len()<3 {empty()} else {out};
    }
    let mut out=Vec::new();
    for i in 0..n {
        let a=normals[(i+n-1)%n]; let b=normals[i]; let p=points[i];
        let (da,db)=(distance(a),distance(b)); let det=cross(a,b);
        let delta=if det.abs()>1e-9 {
            ((da*b.1-a.1*db)/det,(a.0*db-da*b.0)/det)
        } else { (b.0*db,b.1*db) };
        if delta.0.hypot(delta.1)>16.0*ex.abs().max(ey.abs()) {
            // Bevel near reversals instead of producing unbounded miter spikes.
            out.push((p.0+a.0*da,p.1+a.1*da));
            out.push((p.0+b.0*db,p.1+b.1*db));
        } else { out.push((p.0+delta.0,p.1+delta.1)); }
    }
    out
}

#[test]
fn contour_offset_uses_edge_distance_and_preserves_winding_and_zero() {
    let rectangle=vec![(4.0,6.0),(12.0,6.0),(12.0,24.0),(4.0,24.0)];
    assert_eq!(offset_polygon(&rectangle,0.0,0.0),rectangle);
    for reverse in [false,true] {
        let mut input=rectangle.clone(); if reverse {input.reverse();}
        for (ex,ey,bounds) in [(2.0,2.0,(2.0,4.0,14.0,26.0)),
            (-2.0,-2.0,(6.0,8.0,10.0,22.0)),(2.0,-2.0,(2.0,8.0,14.0,22.0))] {
            let out=offset_polygon(&input,ex,ey);
            for y in 0..31 {for x in 0..17 {
                let (px,py)=(x as f64+0.5,y as f64+0.5);
                assert_eq!(point_in_polygon(px,py,&out),px>bounds.0&&px<bounds.2&&py>bounds.1&&py<bounds.3);
            }}
        }
        let collapsed=offset_polygon(&input,-20.0,-20.0);
        assert!(!collapsed.is_empty());
        assert!(!point_in_polygon(8.0,15.0,&collapsed));
    }
    // Non-axis-aligned contour: each edge of this diamond moves exactly 2px.
    let diamond=vec![(0.0,-10.0),(10.0,0.0),(0.0,10.0),(-10.0,0.0)];
    let out=offset_polygon(&diamond,2.0,2.0);
    assert!((out[1].0-(10.0+2.0*2.0f64.sqrt())).abs()<1e-10);
    let duplicate=vec![(4.0,6.0),(4.0,6.0),(12.0,6.0),(12.0,24.0),(4.0,24.0),(4.0,6.0)];
    assert_eq!(offset_polygon(&duplicate,2.0,2.0),offset_polygon(&rectangle,2.0,2.0));
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
    render_with_checkin(|| {
    let mut output_world = cb.checkout_output()?.ok_or(ae::Error::Generic)?;

    let w = output_world.width() as usize;
    let h = output_world.height() as usize;
    // Copy input to output first
    let depth = copy_layer(&input_world, &mut output_world)?;
    if w == 0 || h == 0 { return Ok(()); }

    if polygon.is_empty() {
        return Ok(());
    }

    let stride = output_world.buffer_stride();
    let reverse = output_world.row_bytes() < 0;
    let buf = output_world.buffer_mut();

    for y in 0..h {
        let row_off = (if reverse {h - 1 - y} else {y}) * stride;
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
                    // AE pixels are premultiplied: coverage scales all channels.
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

    Ok(())
    }, || cb.checkin_layer_pixels(0))
}

// Once input pixels have been acquired, release them on every work-result path,
// including a Rust panic. Preserve the original render error over cleanup errors.
fn render_with_checkin(
    work: impl FnOnce() -> Result<(), ae::Error>,
    checkin: impl FnOnce() -> Result<(), ae::Error>,
) -> Result<(), ae::Error> {
    let result = panic::catch_unwind(AssertUnwindSafe(work))
        .unwrap_or(Err(ae::Error::Generic));
    let released = checkin();
    result.and(released)
}

fn multiply_alpha(buf: &mut [u8], row_off: usize, x: usize, depth: i16, factor: f64) {
    let channel_bytes = match depth {16 => 2usize, 32 => 4, _ => 1};
    let pixel_bytes = 4 * channel_bytes;
    let Some(start) = x.checked_mul(pixel_bytes).and_then(|v|row_off.checked_add(v)) else {return;};
    let Some(end) = start.checked_add(pixel_bytes) else {return;};
    let Some(pixel) = buf.get_mut(start..end) else {return;};
    // Preserve exact identity, including native low bits and float HDR values.
    if factor == 1.0 {return;}
    if factor == 0.0 {pixel.fill(0); return;}
    for channel in pixel.chunks_exact_mut(channel_bytes) {
        match depth {
            16 => {
                let value = u16::from_ne_bytes(channel.try_into().unwrap());
                channel.copy_from_slice(&((value as f64 * factor) as u16).to_ne_bytes());
            }
            32 => {
                let value = f32::from_ne_bytes(channel.try_into().unwrap());
                channel.copy_from_slice(&(value * factor as f32).to_ne_bytes());
            }
            _ => channel[0] = (channel[0] as f64 * factor) as u8,
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

fn world_depth(layer: &ae::Layer) -> Result<i16, ae::Error> {
    if layer.width()>i32::MAX as usize || layer.height()>i32::MAX as usize
        || layer.row_bytes()==i32::MIN as isize {return Err(ae::Error::BadCallbackParameter);}
    if layer.width()==0 || layer.height()==0 {return Ok(8);}
    let depth = match layer.pixel_format()? {
        ae::PixelFormat::Argb32 => 8,
        ae::PixelFormat::Argb64 => 16,
        ae::PixelFormat::Argb128 => 32,
        _ => return Err(ae::Error::BadCallbackParameter),
    };
    checked_extent(layer.width(),layer.height(),layer.buffer_stride(),depth as usize / 2)?;
    let len=layer.buffer_stride().checked_mul(layer.height()).ok_or(ae::Error::OutOfMemory)?;
    if len>isize::MAX as usize || unsafe {layer.data_ptr()}.is_null() {return Err(ae::Error::BadCallbackParameter);}
    Ok(depth)
}

fn checked_extent(w:usize,h:usize,stride:usize,bpp:usize)->Result<usize,ae::Error> {
    if w==0 || h==0 {return Ok(0);}
    let row=w.checked_mul(bpp).ok_or(ae::Error::OutOfMemory)?;
    if stride<row {return Err(ae::Error::BadCallbackParameter);}
    (h-1).checked_mul(stride).and_then(|v|v.checked_add(row)).ok_or(ae::Error::OutOfMemory)
}

fn copy_rows(src:&[u8],dst:&mut[u8],w:usize,h:usize,bpp:usize,ss:usize,ds:usize,sr:bool,dr:bool)->Result<(),ae::Error> {
    if src.len()<checked_extent(w,h,ss,bpp)? || dst.len()<checked_extent(w,h,ds,bpp)? {return Err(ae::Error::BadCallbackParameter);}
    if w==0 || h==0 {return Ok(());}
    let row_bytes=w*bpp;
    for y in 0..h {
        let so=(if sr {h-1-y} else {y})*ss;
        let do_=(if dr {h-1-y} else {y})*ds;
        dst[do_..do_+row_bytes].copy_from_slice(&src[so..so+row_bytes]);
    }
    Ok(())
}

fn copy_layer(src: &ae::Layer, dst: &mut ae::Layer) -> Result<i16,ae::Error> {
    if (src.width(),src.height())!=(dst.width(),dst.height()) {return Err(ae::Error::BadCallbackParameter);}
    let depth=world_depth(src)?;
    if depth!=world_depth(dst)? {return Err(ae::Error::BadCallbackParameter);}
    let (w,h)=(src.width(),src.height());
    if w==0 || h==0 {return Ok(depth);}
    let (ss,ds,sr,dr)=(src.buffer_stride(),dst.buffer_stride(),src.row_bytes()<0,dst.row_bytes()<0);
    copy_rows(src.buffer(),dst.buffer_mut(),w,h,depth as usize/2,ss,ds,sr,dr)?;
    Ok(depth)
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

    let mut is_open: A_Boolean = 0;
    let mut num_segments: A_long = 0;
    let vertices=(|| {
        if outline_suite.AEGP_IsMaskOutlineOpen?(outline_h,&mut is_open)!=0 {return None;}
        if outline_suite.AEGP_GetMaskOutlineNumSegments?(outline_h,&mut num_segments)!=0 {return None;}
        let vf=outline_suite.AEGP_GetMaskOutlineVertexInfo?;
        collect_outline_vertices(num_segments,is_open!=0,|i| {
            let mut vtx: AEGP_MaskVertex=std::mem::zeroed();
            if vf(outline_h,i,&mut vtx)!=0 {return None;}
            Some(MaskVertex {x:vtx.x,y:vtx.y,tan_in_x:vtx.tan_in_x,tan_in_y:vtx.tan_in_y,
                tan_out_x:vtx.tan_out_x,tan_out_y:vtx.tan_out_y})
        })
    })().unwrap_or_default();

    if let Some(dispose) = stream_suite.AEGP_DisposeStreamValue { dispose(&mut stream_val2); }
    if let Some(r) = release { r(outline_suite_name, outline_suite_version); r(stream_suite_name, stream_suite_version); }

    (vertices, is_open == 0)
}

fn collect_outline_vertices(segments:i32,open:bool,mut read:impl FnMut(i32)->Option<MaskVertex>) -> Option<Vec<MaskVertex>> {
    if segments<0 {return None;}
    let count=segments.checked_add(i32::from(open))?;
    let mut vertices=Vec::new();
    vertices.try_reserve_exact(count as usize).ok()?;
    for index in 0..count {vertices.push(read(index)?);}
    Some(vertices)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checked_rows_preserve_padding_reverse_and_reject_partial_copy() {
        for bpp in [4,8,16] {
            let stride=2*bpp+3;
            let src:Vec<u8>=(0..stride*3).map(|v|v as u8).collect();
            for sr in [false,true] {for dr in [false,true] {
                let mut dst=vec![255;stride*3];
                copy_rows(&src,&mut dst,2,3,bpp,stride,stride,sr,dr).unwrap();
                for y in 0..3 {
                    let s=(if sr {2-y} else {y})*stride;
                    let d=(if dr {2-y} else {y})*stride;
                    assert_eq!(&dst[d..d+2*bpp],&src[s..s+2*bpp]);
                    assert_eq!(&dst[d+2*bpp..d+stride],&[255;3]);
                }
            }}
            let mut dst=vec![99;stride*3];
            assert!(copy_rows(&src[..stride],&mut dst,2,3,bpp,stride,stride,false,false).is_err());
            assert!(dst.iter().all(|v|*v==99));
            assert!(checked_extent(2,3,bpp,bpp).is_err());
        }
        assert!(checked_extent(usize::MAX,2,usize::MAX,16).is_err());
        assert_eq!(checked_extent(0,usize::MAX,0,16),Ok(0));
    }
    #[test]
    fn mask_coverage_scales_premultiplied_rgb_at_native_precision() {
        let mut p8 = [128, 100, 64, 2];
        multiply_alpha(&mut p8, 0, 0, 8, 0.5);
        assert_eq!(p8, [64, 50, 32, 1]);
        let mut p16: Vec<u8> = [32768u16, 16386, 2, 0].into_iter().flat_map(u16::to_ne_bytes).collect();
        multiply_alpha(&mut p16, 0, 0, 16, 0.5);
        assert_eq!(p16.chunks_exact(2).map(|c|u16::from_ne_bytes(c.try_into().unwrap())).collect::<Vec<_>>(), [16384,8193,1,0]);
        let mut pf: Vec<u8> = [0.5f32, 8.0, -2.0, 0.25].into_iter().flat_map(f32::to_ne_bytes).collect();
        multiply_alpha(&mut pf, 0, 0, 32, 0.5);
        assert_eq!(pf.chunks_exact(4).map(|c|f32::from_ne_bytes(c.try_into().unwrap())).collect::<Vec<_>>(), [0.25,4.0,-1.0,0.125]);
        multiply_alpha(&mut pf, 0, 0, 32, 0.0);
        assert!(pf.chunks_exact(4).all(|c|f32::from_ne_bytes(c.try_into().unwrap())==0.0));
    }
    #[test]
    fn render_checkin_runs_once_on_success_error_and_panic() {
        for stage in 0..3 {
            for release_fails in [false, true] {
                let calls = std::cell::Cell::new(0);
                let result = render_with_checkin(|| match stage {
                    0 => Ok(()),
                    1 => Err(ae::Error::BadCallbackParameter),
                    _ => panic!("injected render panic"),
                }, || {
                    calls.set(calls.get() + 1);
                    if release_fails { Err(ae::Error::OutOfMemory) } else { Ok(()) }
                });
                assert_eq!(calls.get(), 1);
                match stage {
                    0 => assert_eq!(result, if release_fails {Err(ae::Error::OutOfMemory)} else {Ok(())}),
                    1 => assert_eq!(result, Err(ae::Error::BadCallbackParameter)),
                    _ => assert_eq!(result, Err(ae::Error::Generic)),
                }
            }
        }
    }
    fn vertex(x:f64)->MaskVertex {MaskVertex{x,y:0.0,tan_in_x:0.0,tan_in_y:0.0,tan_out_x:0.0,tan_out_y:0.0}}
    #[test]
    fn open_outline_reads_terminal_vertex_closed_avoids_duplicate() {
        let open=collect_outline_vertices(2,true,|i|Some(vertex(i as f64))).unwrap();
        assert_eq!(open.iter().map(|v|v.x).collect::<Vec<_>>(),[0.0,1.0,2.0]);
        let closed=collect_outline_vertices(2,false,|i|Some(vertex(i as f64))).unwrap();
        assert_eq!(closed.len(),2);
        assert_eq!(collect_outline_vertices(0,true,|_|Some(vertex(0.0))).unwrap().len(),1);
    }
    #[test]
    fn bad_vertex_discards_whole_outline_not_partial_shape() {
        assert!(collect_outline_vertices(3,false,|i|if i==1 {None} else {Some(vertex(i as f64))}).is_none());
        assert!(collect_outline_vertices(-1,false,|_|panic!("invalid read")).is_none());
        assert!(collect_outline_vertices(i32::MAX,true,|_|panic!("overflow read")).is_none());
    }
}
