use after_effects as ae;

mod filters;

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
                f.set_options(&["Median", "Weighted Median"]);
                f.set_default(1);
            }),
        )?;

        params.add(
            Params::Radius,
            "Radius",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(100);
                f.set_slider_min(0);
                f.set_slider_max(50);
                f.set_default(3);
            }),
        )?;

        params.add(
            Params::EdgePreserve,
            "Weight Falloff",
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
                    "MedianPro v1.0\rMedian filtering with Luminance Map support.\rMedian and Weighted Median.\rWritten in Rust.",
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

            ae::Command::UpdateParamsUi | ae::Command::UserChangedParam { .. } => {
                update_filter_ui(params)?;
                out_data.set_out_flag(ae::OutFlags::RefreshUi, true);
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
            .clamp(0, 100) as usize,
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

fn checked_extent(w: usize, h: usize, stride: usize) -> Result<usize, ae::Error> {
    if w == 0 || h == 0 { return Ok(0); }
    let row = w.checked_mul(4).ok_or(ae::Error::OutOfMemory)?;
    if stride < row { return Err(ae::Error::BadCallbackParameter); }
    (h-1).checked_mul(stride).and_then(|v|v.checked_add(row)).ok_or(ae::Error::OutOfMemory)
}

fn copy_rows(src: &[u8], dst: &mut [u8], w: usize, h: usize,
    src_stride: usize, dst_stride: usize, src_reverse: bool, dst_reverse: bool) -> Result<(), ae::Error> {
    let src_extent = checked_extent(w,h,src_stride)?;
    let dst_extent = checked_extent(w,h,dst_stride)?;
    if src.len()<src_extent || dst.len()<dst_extent { return Err(ae::Error::BadCallbackParameter); }
    if w==0 || h==0 { return Ok(()); }
    let row=w*4;
    for y in 0..h {
        let s=(if src_reverse {h-1-y} else {y})*src_stride;
        let d=(if dst_reverse {h-1-y} else {y})*dst_stride;
        dst[d..d+row].copy_from_slice(&src[s..s+row]);
    }
    Ok(())
}

fn validate_world(layer: &ae::Layer) -> Result<(), ae::Error> {
    if layer.width()>i32::MAX as usize || layer.height()>i32::MAX as usize
        || layer.row_bytes()==i32::MIN as isize { return Err(ae::Error::BadCallbackParameter); }
    if layer.width()==0 || layer.height()==0 { return Ok(()); }
    checked_extent(layer.width(),layer.height(),layer.buffer_stride())?;
    // Layer::buffer constructs a slice covering stride * height.
    let len=layer.buffer_stride().checked_mul(layer.height()).ok_or(ae::Error::OutOfMemory)?;
    if len>isize::MAX as usize || unsafe {layer.data_ptr()}.is_null()
        || layer.pixel_format()?!=ae::PixelFormat::Argb32 { return Err(ae::Error::BadCallbackParameter); }
    Ok(())
}

fn layer_to_flat(layer: &ae::Layer) -> Result<(Vec<u8>, usize, usize), ae::Error> {
    validate_world(layer)?;
    let w = layer.width() as usize; let h = layer.height() as usize; let stride = layer.buffer_stride();
    if w==0 || h==0 { return Ok((Vec::new(),w,h)); }
    let row=w.checked_mul(4).ok_or(ae::Error::OutOfMemory)?;
    let count=row.checked_mul(h).ok_or(ae::Error::OutOfMemory)?;
    let mut flat=Vec::new();
    flat.try_reserve_exact(count).map_err(|_|ae::Error::OutOfMemory)?;
    flat.resize(count,0);
    copy_rows(layer.buffer(),&mut flat,w,h,stride,row,layer.row_bytes()<0,false)?;
    Ok((flat, w, h))
}

fn flat_to_layer(flat: &[u8], layer: &mut ae::Layer, w: usize, h: usize) -> Result<(), ae::Error> {
    if (w,h)!=(layer.width(),layer.height()) { return Err(ae::Error::BadCallbackParameter); }
    validate_world(layer)?;
    if w==0 || h==0 { return Ok(()); }
    let stride=layer.buffer_stride(); let reverse=layer.row_bytes()<0;
    let row=w.checked_mul(4).ok_or(ae::Error::OutOfMemory)?;
    copy_rows(flat,layer.buffer_mut(),w,h,row,stride,false,reverse)
}

// ---- Build luminance map from a flat ARGB buffer ----

fn build_luminance_map(buf: &[u8], w: usize, h: usize, invert: bool) -> Option<Vec<f64>> {
    if w==0 || h==0 { return None; }
    let count=w.checked_mul(h)?;
    if buf.len()<count.checked_mul(4)? { return None; }
    let mut map = Vec::new();
    map.try_reserve_exact(count).ok()?;
    for i in 0..count {
        let off = i * 4;
        let r = buf[off + 1] as f64;
        let g = buf[off + 2] as f64;
        let b = buf[off + 3] as f64;
        let luma = (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255.0;
        map.push(if invert { 1.0 - luma } else { luma });
    }
    Some(map)
}

// ---- Process filters on flat buffer ----

fn copy_buffer(src: &[u8]) -> Result<Vec<u8>,ae::Error> {
    let mut copy=Vec::new();
    copy.try_reserve_exact(src.len()).map_err(|_|ae::Error::OutOfMemory)?;
    copy.extend_from_slice(src);
    Ok(copy)
}

fn process_filters(
    fp: &FilterParams,
    original: &[u8],
    w: usize,
    h: usize,
    luma_map: Option<&[f64]>,
) -> Result<Vec<u8>,ae::Error> {
    let count=w.checked_mul(h).and_then(|n|n.checked_mul(4)).ok_or(ae::Error::OutOfMemory)?;
    if original.len()!=count { return Err(ae::Error::BadCallbackParameter); }
    if count==0 { return Ok(Vec::new()); }
    if fp.radius == 0 || fp.mix <= f64::EPSILON {
        return copy_buffer(original);
    }

    // AE effect worlds are premultiplied ARGB. Median filtering must operate
    // on straight RGB, otherwise transparent black becomes a valid color
    // sample and creates dark fringes around alpha edges.
    let straight_original = unpremultiply_buffer(original)?;
    let mut buf_a = copy_buffer(&straight_original)?;
    let mut buf_b = Vec::new();
    buf_b.try_reserve_exact(count).map_err(|_|ae::Error::OutOfMemory)?;
    buf_b.resize(count,0);

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

    // buf_b is the obsolete previous iteration after the last swap.
    filters::mix_buffers(&straight_original, &buf_a, &mut buf_b, fp.mix);
    premultiply_buffer(&mut buf_b);
    Ok(buf_b)
}

fn update_filter_ui(params: &ae::Parameters<Params>) -> Result<(), ae::Error> {
    let weighted_median = params.get(Params::FilterType)?.as_popup()?.value() == 2;
    let mut params_copy = params.cloned();
    let mut falloff = params_copy.get_mut(Params::EdgePreserve)?;
    falloff.set_ui_flag(ae::ParamUIFlags::DISABLED, !weighted_median);
    falloff.update_param_ui()?;
    Ok(())
}

#[inline]
fn unpremultiply_channel(channel: u8, alpha: u8) -> u8 {
    if alpha == 0 {
        0
    } else {
        ((channel as u16 * 255 + alpha as u16 / 2) / alpha as u16).min(255) as u8
    }
}

#[inline]
fn premultiply_channel(channel: u8, alpha: u8) -> u8 {
    ((channel as u16 * alpha as u16 + 127) / 255) as u8
}

fn unpremultiply_buffer(src: &[u8]) -> Result<Vec<u8>,ae::Error> {
    let mut out = copy_buffer(src)?;
    for pixel in out.chunks_exact_mut(4) {
        let alpha = pixel[0];
        pixel[1] = unpremultiply_channel(pixel[1], alpha);
        pixel[2] = unpremultiply_channel(pixel[2], alpha);
        pixel[3] = unpremultiply_channel(pixel[3], alpha);
    }
    Ok(out)
}

fn premultiply_buffer(pixels: &mut [u8]) {
    for pixel in pixels.chunks_exact_mut(4) {
        let alpha = pixel[0];
        pixel[1] = premultiply_channel(pixel[1], alpha);
        pixel[2] = premultiply_channel(pixel[2], alpha);
        pixel[3] = premultiply_channel(pixel[3], alpha);
    }
}

// ---- Legacy Render path ----

fn render_cpu(
    params: &ae::Parameters<Params>,
    in_data: &ae::InData,
    in_layer: &ae::Layer,
    out_layer: &mut ae::Layer,
) -> Result<(), ae::Error> {
    let fp = get_params(params)?;
    let (original, w, h) = layer_to_flat(in_layer)?;

    let luma_map = get_map_layer_luma(params, in_data, w, h, fp.invert_map);

    let result = process_filters(&fp, &original, w, h, luma_map.as_deref())?;
    flat_to_layer(&result, out_layer, w, h)?;
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
    let (flat, mw, mh) = layer_to_flat(&layer).ok()?;
    if mw == w && mh == h {
        build_luminance_map(&flat, w, h, invert)
    } else {
        build_luminance_map_resample(&flat, mw, mh, w, h, invert)
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
) -> Option<Vec<f64>> {
    if w==0 || h==0 { return Some(Vec::new()); }
    let count=w.checked_mul(h)?;
    (w-1).checked_mul(mw)?;
    (h-1).checked_mul(mh)?;
    let src_map = build_luminance_map(buf, mw, mh, invert)?;
    let mut map = Vec::new();
    map.try_reserve_exact(count).ok()?;
    for y in 0..h {
        let sy = (y * mh / h).min(mh - 1);
        for x in 0..w {
            let sx = (x * mw / w).min(mw - 1);
            map.push(src_map[sy * mw + sx]);
        }
    }
    Some(map)
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

// Preserve the render error if cleanup also fails, but never skip cleanup on
// a Result error. This does not provide panic/OOM unwinding guarantees.
fn with_checkin<T,E>(render: impl FnOnce()->Result<T,E>, checkin: impl FnOnce()->Result<(),E>) -> Result<T,E> {
    let result=render();
    let cleanup=checkin();
    match result { Err(error)=>Err(error), Ok(value)=>cleanup.map(|_|value) }
}

fn smart_render_cpu(
    extra: &ae::pf::SmartRenderExtra,
    params: &ae::Parameters<Params>,
) -> Result<(), ae::Error> {
    let fp = get_params(params)?;
    let cb = extra.callbacks();

    let input_world = cb.checkout_layer_pixels(0)?.ok_or(ae::Error::Generic)?;
    with_checkin(|| {
    let mut output_world = cb.checkout_output()?.ok_or(ae::Error::Generic)?;

    let (original, w, h) = layer_to_flat(&input_world)?;

    let invert = fp.invert_map;
    let luma_map = if let Ok(Some(map_world)) = cb.checkout_layer_pixels(MAP_CHECKOUT_ID as u32) {
        with_checkin(|| {
        let Ok((map_flat, mw, mh)) = layer_to_flat(&map_world) else { return Ok(None); };
        Ok(if mw == w && mh == h {
            build_luminance_map(&map_flat, w, h, invert)
        } else {
            build_luminance_map_resample(
                &map_flat, mw, mh, w, h, invert,
            )
        })
        }, ||cb.checkin_layer_pixels(MAP_CHECKOUT_ID as u32))?
    } else {
        None
    };

    let result = process_filters(&fp, &original, w, h, luma_map.as_deref())?;
    flat_to_layer(&result, &mut output_world, w, h)?;

    Ok(())
    }, ||cb.checkin_layer_pixels(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_copy_validates_every_row_before_writing() {
        let src=[1,2,3,4,5,6,7,8];
        let mut short=[99;7];
        assert!(copy_rows(&src,&mut short,1,2,4,4,false,false).is_err());
        assert_eq!(short,[99;7]);
        let mut padded=[99;10];
        assert!(copy_rows(&src[..7],&mut padded,1,2,4,6,false,false).is_err());
        assert_eq!(padded,[99;10]);
        assert!(copy_rows(&src,&mut padded,2,1,4,8,false,false).is_err());
        assert!(copy_rows(&src,&mut padded,usize::MAX,2,4,4,false,false).is_err());
        assert_eq!(padded,[99;10]);
        copy_rows(&src,&mut padded,1,2,4,6,false,true).unwrap();
        assert_eq!(padded,[5,6,7,8,99,99,1,2,3,4]);
        let mut restored=[0;8];
        copy_rows(&padded,&mut restored,1,2,6,4,true,false).unwrap();
        assert_eq!(restored,src);
        copy_rows(&[],&mut [],0,3,0,0,false,false).unwrap();
    }

    #[test]
    fn empty_primary_does_not_panic() {
        let result=std::panic::catch_unwind(||process_filters(&median_params(),&[],0,3,None));
        assert!(result.is_ok());
        assert!(result.unwrap().unwrap().is_empty());
        assert!(process_filters(&median_params(),&[],usize::MAX,2,None).is_err());
        assert!(process_filters(&median_params(),&[0;7],2,1,None).is_err());
    }

    #[test]
    fn cleanup_runs_on_every_result_and_retains_primary_error() {
        for render_ok in [false,true] {
            for cleanup_ok in [false,true] {
                let mut calls=0;
                let result=with_checkin(||if render_ok {Ok(7)} else {Err("render")},|| {
                    calls+=1;
                    if cleanup_ok {Ok(())} else {Err("cleanup")}
                });
                assert_eq!(calls,1);
                assert_eq!(result,if !render_ok {Err("render")} else if !cleanup_ok {Err("cleanup")} else {Ok(7)});
            }
        }
        let order=std::cell::RefCell::new(Vec::new());
        let result: Result<(),&str>=with_checkin(||with_checkin(||Ok(()),||{order.borrow_mut().push("map");Err("map cleanup")}),||{order.borrow_mut().push("input");Ok(())});
        assert_eq!(result,Err("map cleanup"));
        assert_eq!(*order.borrow(),["map","input"]);
    }

    #[test]
    fn empty_radius_map_does_not_panic() {
        let result=std::panic::catch_unwind(||build_luminance_map_resample(&[],0,0,17,31,false));
        assert!(result.is_ok(), "empty checked-out map must not panic");
        assert!(result.unwrap().is_none());
        assert!(build_luminance_map(&[0;7],2,1,false).is_none());
        assert!(build_luminance_map(&[],usize::MAX,2,false).is_none());
        assert!(build_luminance_map_resample(&[255,0,0,0],1,1,usize::MAX,2,false).is_none());
        assert_eq!(build_luminance_map_resample(&[255,0,0,0],1,1,2,3,false).unwrap(),[0.0;6]);
        assert_eq!(build_luminance_map_resample(&[255,0,0,0],1,1,2,3,true).unwrap(),[1.0;6]);
    }

    fn median_params() -> FilterParams {
        FilterParams {
            filter_type: 1,
            radius: 1,
            edge_preserve: 50.0,
            iterations: 1,
            mix: 1.0,
            invert_map: false,
        }
    }

    #[test]
    fn transparent_neighbors_do_not_turn_opaque_color_black() {
        let mut src = vec![0u8; 3 * 3 * 4];
        let center = (1 * 3 + 1) * 4;
        src[center..center + 4].copy_from_slice(&[255, 255, 0, 0]);

        let out = process_filters(&median_params(), &src, 3, 3, None).unwrap();

        assert_eq!(&out[center..center + 4], &[255, 255, 0, 0]);
        for pixel in out.chunks_exact(4) {
            assert!(pixel[1] <= pixel[0]);
            assert!(pixel[2] <= pixel[0]);
            assert!(pixel[3] <= pixel[0]);
        }
    }

    #[test]
    fn weighted_median_ignores_fully_transparent_color_samples() {
        let mut src = vec![0u8; 3 * 3 * 4];
        let center = (1 * 3 + 1) * 4;
        src[center..center + 4].copy_from_slice(&[255, 0, 255, 0]);
        let mut params = median_params();
        params.filter_type = 2;

        let out = process_filters(&params, &src, 3, 3, None).unwrap();

        assert_eq!(&out[center..center + 4], &[255, 0, 255, 0]);
    }

    #[test]
    fn half_transparent_color_stays_premultiplied() {
        let src = [128, 128, 0, 0].repeat(9);
        let out = process_filters(&median_params(), &src, 3, 3, None).unwrap();

        assert!(out.chunks_exact(4).all(|pixel| pixel == [128, 128, 0, 0]));
    }

    #[test]
    fn zero_radius_is_byte_exact() {
        let src = [
            255, 10, 20, 30, 128, 64, 32, 16, 0, 0, 0, 0, 255, 200, 100, 50,
        ];
        let mut params = median_params();
        params.radius = 0;
        params.iterations = 10;

        let out = process_filters(&params, &src, 2, 2, None).unwrap();

        assert_eq!(out, src);
    }
}
