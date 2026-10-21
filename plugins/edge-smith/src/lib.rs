use after_effects as ae;

pub mod core;
mod gpu;
use core::EdgeParams;

const INPUT_ID: u32 = 1;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Border,
    Roughness,
    Scale,
    Detail,
    Balance,
    Sharpness,
    Stretch,
    Angle,
    Evolution,
    Seed,
    Preserve,
    Mix,
    UseGpu,
}

#[derive(Clone, Copy)]
struct PreRenderState {
    rect: ae::Rect,
}

#[derive(Default)]
struct Plugin;
ae::define_effect!(Plugin, (), Params);

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        p: &mut ae::Parameters<Params>,
        _i: ae::InData,
        _o: ae::OutData,
    ) -> Result<(), ae::Error> {
        p.add(
            Params::Border,
            "Border",
            fs(-512.0, 512.0, -100.0, 100.0, 0.0, 1),
        )?;
        p.add(
            Params::Roughness,
            "Roughness",
            fs(0.0, 512.0, 0.0, 100.0, 12.0, 1),
        )?;
        p.add(Params::Scale, "Scale", fs(1.0, 2048.0, 2.0, 500.0, 48.0, 1))?;
        p.add(
            Params::Detail,
            "Detail",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(1);
                f.set_valid_max(8);
                f.set_slider_min(1);
                f.set_slider_max(8);
                f.set_default(4);
            }),
        )?;
        p.add(
            Params::Balance,
            "Edge Balance",
            fs(-100.0, 100.0, -100.0, 100.0, 0.0, 1),
        )?;
        p.add(
            Params::Sharpness,
            "Sharpness",
            fs(0.0, 100.0, 0.0, 100.0, 70.0, 1),
        )?;
        p.add(
            Params::Stretch,
            "Stretch",
            fs(-99.0, 99.0, -99.0, 99.0, 0.0, 1),
        )?;
        p.add(
            Params::Angle,
            "Direction",
            ae::AngleDef::setup(|f| {
                f.set_default(0.0);
            }),
        )?;
        p.add(
            Params::Evolution,
            "Evolution",
            ae::AngleDef::setup(|f| {
                f.set_default(0.0);
            }),
        )?;
        p.add(
            Params::Seed,
            "Random Seed",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(100000);
                f.set_slider_min(0);
                f.set_slider_max(10000);
                f.set_default(0);
            }),
        )?;
        p.add(
            Params::Preserve,
            "Preserve Detail",
            fs(0.0, 100.0, 0.0, 100.0, 50.0, 1),
        )?;
        p.add(Params::Mix, "Mix", fs(0.0, 100.0, 0.0, 100.0, 100.0, 1))?;
        p.add(
            Params::UseGpu,
            "GPU Acceleration",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(true);
                f.set_label("Use GPU (Metal / Vulkan / DX12)");
            }),
        )?;
        Ok(())
    }
    fn handle_command(
        &mut self,
        cmd: ae::Command,
        i: ae::InData,
        mut o: ae::OutData,
        p: &mut ae::Parameters<Params>,
    ) -> Result<(), ae::Error> {
        match cmd {
            ae::Command::About=>o.set_return_msg("EdgeSmith v0.1\rDistance-field rough edge designer with deterministic GPU acceleration.\rWritten in Rust."),
            ae::Command::Render{in_layer,mut out_layer}=>render_layer(&in_layer,&mut out_layer,&read_params(p)?,false)?,
            ae::Command::SmartPreRender{mut extra}=>pre_render(&i,p,&mut extra)?,
            ae::Command::SmartRender{extra}=>smart_render(&extra,&i,p,false)?,
            ae::Command::SmartRenderGpu{extra}=>smart_render(&extra,&i,p,true)?,
            _=>{}
        }
        Ok(())
    }
}

fn fs(
    vmin: f32,
    vmax: f32,
    smin: f32,
    smax: f32,
    default: f64,
    precision: i16,
) -> ae::FloatSliderDef<'static> {
    ae::FloatSliderDef::setup(|f| {
        f.set_valid_min(vmin);
        f.set_valid_max(vmax);
        f.set_slider_min(smin);
        f.set_slider_max(smax);
        f.set_default(default);
        f.set_precision(precision);
    })
}
fn read_params(p: &ae::Parameters<Params>) -> Result<EdgeParams, ae::Error> {
    let v = |id| -> Result<f32, ae::Error> { Ok(p.get(id)?.as_float_slider()?.value() as f32) };
    Ok(EdgeParams {
        border: v(Params::Border)?,
        roughness: v(Params::Roughness)?,
        scale: v(Params::Scale)?,
        detail: p.get(Params::Detail)?.as_slider()?.value().clamp(1, 8) as u32,
        balance: v(Params::Balance)?,
        sharpness: v(Params::Sharpness)?,
        stretch: v(Params::Stretch)?,
        angle: p.get(Params::Angle)?.as_angle()?.value() as f32,
        evolution: p.get(Params::Evolution)?.as_angle()?.value() as f32,
        seed: p.get(Params::Seed)?.as_slider()?.value().max(0) as u32,
        preserve: v(Params::Preserve)?,
        mix: v(Params::Mix)? / 100.0,
    })
}

fn pre_render(
    i: &ae::InData,
    p: &ae::Parameters<Params>,
    extra: &mut ae::pf::PreRenderExtra,
) -> Result<(), ae::Error> {
    let pad = (p.get(Params::Border)?.as_float_slider()?.value().abs()
        + p.get(Params::Roughness)?.as_float_slider()?.value()
        + 8.0)
        .ceil()
        .min(1024.0) as i32;
    let mut req = extra.output_request();
    req.rect = expand_request(req.rect.into(), pad)?.into();
    let got = extra.callbacks().checkout_layer(
        0,
        INPUT_ID as i32,
        &req,
        i.current_time(),
        i.time_step(),
        i.time_scale(),
    )?;
    extra.set_gpu_render_possible(p.get(Params::UseGpu)?.as_checkbox()?.value());
    extra.set_returns_extra_pixels(true);
    extra.set_result_rect(got.result_rect.into());
    extra.set_max_result_rect(got.max_result_rect.into());
    extra.set_pre_render_data(PreRenderState {
        rect: got.result_rect.into(),
    });
    Ok(())
}

fn expand_request(r: ae::Rect, pad: i32) -> Result<ae::Rect, ae::Error> {
    let bad = ae::Error::BadCallbackParameter;
    if pad < 0 || r.left > r.right || r.top > r.bottom { return Err(bad); }
    Ok(ae::Rect {
        left: r.left.checked_sub(pad).ok_or(bad)?,
        top: r.top.checked_sub(pad).ok_or(bad)?,
        right: r.right.checked_add(pad).ok_or(bad)?,
        bottom: r.bottom.checked_add(pad).ok_or(bad)?,
    })
}

#[test]
fn request_expansion_rejects_coordinate_overflow() {
    let r = ae::Rect { left: -17, top: -31, right: 321, bottom: 181 };
    for pad in [0, 8, 1024] {
        let out = expand_request(r, pad).unwrap();
        assert_eq!((out.left,out.top,out.right,out.bottom),(-17-pad,-31-pad,321+pad,181+pad));
    }
    for r in [ae::Rect { left:i32::MIN,top:0,right:1,bottom:1 },
        ae::Rect { left:0,top:i32::MIN,right:1,bottom:1 },
        ae::Rect { left:0,top:0,right:i32::MAX,bottom:1 },
        ae::Rect { left:0,top:0,right:1,bottom:i32::MAX },
        ae::Rect { left:2,top:0,right:1,bottom:1 }] {
        assert!(expand_request(r,8).is_err());
    }
    assert!(expand_request(r,-1).is_err());
}

fn smart_render(
    extra: &ae::pf::SmartRenderExtra,
    i: &ae::InData,
    p: &ae::Parameters<Params>,
    gpu_requested: bool,
) -> Result<(), ae::Error> {
    let cb = extra.callbacks();
    let input = cb
        .checkout_layer_pixels(INPUT_ID)?
        .ok_or(ae::Error::Generic)?;
    with_input_checkin(|| {
        let mut output = cb.checkout_output()?.ok_or(ae::Error::Generic)?;
        let ep = read_params(p)?;
        let use_gpu =
            (gpu_requested || p.get(Params::UseGpu)?.as_checkbox()?.value()) && gpu_requested;
        let (rgba, w, h) = layer_rgba(&input)?;
        let rendered = if use_gpu {
            gpu::render(&rgba, w, h, &ep).unwrap_or_else(|_| core::render_rgba(&rgba, w, h, &ep))
        } else {
            core::render_rgba(&rgba, w, h, &ep)
        };
        let ow = output.width();
        let oh = output.height();
        let rect = extra
            .pre_render_data::<PreRenderState>()
            .map(|s| s.rect)
            .unwrap_or(ae::Rect {
                left: 0,
                top: 0,
                right: w as i32,
                bottom: h as i32,
            });
        let cropped = crop_rgba(&rendered, w, h, ow, oh,
            (i.output_origin().h, i.output_origin().v), (rect.left, rect.top))?;
        write_rgba(&cropped, &mut output, ow, oh)
    }, || cb.checkin_layer_pixels(INPUT_ID))
}

fn with_input_checkin<T>(
    work: impl FnOnce() -> Result<T, ae::Error>,
    checkin: impl FnOnce() -> Result<(), ae::Error>,
) -> Result<T, ae::Error> {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work));
    let cleanup = checkin();
    // Always return the acquired input, but do not hide the render failure
    // behind a secondary host cleanup error.
    match result.unwrap_or(Err(ae::Error::InternalStructDamaged)) {
        Err(error) => Err(error),
        Ok(value) => cleanup.map(|()| value),
    }
}

#[test]
fn input_checkin_runs_once_and_preserves_primary_failure() {
    use std::cell::Cell;
    for cleanup_fails in [false, true] {
        for work_kind in 0..3 {
            let calls = Cell::new(0);
            let result = with_input_checkin(|| match work_kind {
                0 => Ok(42),
                1 => Err(ae::Error::OutOfMemory),
                _ => panic!("injected render panic"),
            }, || {
                calls.set(calls.get() + 1);
                if cleanup_fails { Err(ae::Error::BadCallbackParameter) } else { Ok(()) }
            });
            assert_eq!(calls.get(), 1);
            let expected = match work_kind {
                0 if cleanup_fails => Err(ae::Error::BadCallbackParameter),
                0 => Ok(42),
                1 => Err(ae::Error::OutOfMemory),
                _ => Err(ae::Error::InternalStructDamaged),
            };
            assert_eq!(result, expected, "work={work_kind}, cleanup_fails={cleanup_fails}");
        }
    }
}

fn depth_from_format(format: ae::PixelFormat) -> Result<i16, ae::Error> {
    match format {
        ae::PixelFormat::Argb32 => Ok(8),
        ae::PixelFormat::Argb64 => Ok(16),
        ae::PixelFormat::Argb128 => Ok(32),
        _ => Err(ae::Error::BadCallbackParameter),
    }
}

#[test]
fn explicit_world_formats_determine_channel_width() {
    assert_eq!(depth_from_format(ae::PixelFormat::Argb32).unwrap(), 8);
    assert_eq!(depth_from_format(ae::PixelFormat::Argb64).unwrap(), 16);
    assert_eq!(depth_from_format(ae::PixelFormat::Argb128).unwrap(), 32);
    assert!(depth_from_format(ae::PixelFormat::GpuBgra128).is_err());
}

fn checked_world_layout(w: usize, h: usize, row_bytes: isize, bpp: usize) -> Result<(usize, usize), ae::Error> {
    let bad = ae::Error::BadCallbackParameter;
    let stride = row_bytes.checked_abs().ok_or(bad)? as usize;
    let row = w.checked_mul(bpp).ok_or(bad)?;
    let count = w.checked_mul(h).and_then(|n| n.checked_mul(4)).ok_or(bad)?;
    let extent = stride.checked_mul(h).ok_or(bad)?;
    if ![4, 8, 16].contains(&bpp) || w > i32::MAX as usize || h > i32::MAX as usize
        || row_bytes == i32::MIN as isize || extent > isize::MAX as usize
        || count > isize::MAX as usize / std::mem::size_of::<f32>()
        || (w != 0 && h != 0 && stride < row) {
        return Err(bad);
    }
    Ok((stride, count))
}

fn crop_rgba(src: &[f32], w: usize, h: usize, ow: usize, oh: usize,
    output_origin: (i32, i32), input_origin: (i32, i32)) -> Result<Vec<f32>, ae::Error> {
    let count = |width: usize, height: usize| {
        width.checked_mul(height).and_then(|n| n.checked_mul(4))
            .filter(|&n| width <= i32::MAX as usize && height <= i32::MAX as usize
                && n <= isize::MAX as usize / 4)
            .ok_or(ae::Error::BadCallbackParameter)
    };
    if src.len() != count(w,h)? { return Err(ae::Error::BadCallbackParameter); }
    let len = count(ow,oh)?;
    let mut out = Vec::new();
    out.try_reserve_exact(len).map_err(|_|ae::Error::OutOfMemory)?;
    out.resize(len,0.0);
    if len == 0 || w == 0 || h == 0 { return Ok(out); }
    let ox = output_origin.0 as i64 - input_origin.0 as i64;
    let oy = output_origin.1 as i64 - input_origin.1 as i64;
    for y in 0..oh {
        let sy = oy + y as i64;
        if sy < 0 || sy >= h as i64 { continue; }
        let left = 0i64.max(-ox);
        let right = (ow as i64).min(w as i64 - ox);
        if left >= right { continue; }
        let s = (sy as usize * w + (ox + left) as usize) * 4;
        let d = (y * ow + left as usize) * 4;
        let n = (right-left) as usize * 4;
        out[d..d+n].copy_from_slice(&src[s..s+n]);
    }
    Ok(out)
}

#[test]
fn crop_preserves_intersections_and_rejects_invalid_geometry() {
    let src: Vec<f32> = (1..=6).flat_map(|v|[v as f32;4]).collect();
    let out = crop_rgba(&src,3,2,3,2,(1,1),(0,0)).unwrap();
    assert_eq!(out.chunks_exact(4).map(|p|p[0]).collect::<Vec<_>>(),[5.0,6.0,0.0,0.0,0.0,0.0]);
    let out = crop_rgba(&src,3,2,3,2,(-1,-1),(0,0)).unwrap();
    assert_eq!(out.chunks_exact(4).map(|p|p[0]).collect::<Vec<_>>(),[0.0,0.0,0.0,0.0,1.0,2.0]);
    for (a,b) in [(i32::MAX,i32::MIN),(i32::MIN,i32::MAX)] {
        assert!(crop_rgba(&src,3,2,3,2,(a,a),(b,b)).unwrap().iter().all(|&v|v==0.0));
    }
    assert!(crop_rgba(&src[..20],3,2,3,2,(0,0),(0,0)).is_err());
    assert!(crop_rgba(&src,3,2,usize::MAX,2,(0,0),(0,0)).is_err());
    assert!(crop_rgba(&src,3,2,0,2,(0,0),(0,0)).unwrap().is_empty());
    assert_eq!(crop_rgba(&src,3,2,3,2,(0,0),(0,0)).unwrap(),src);
}

#[test]
fn world_layout_rejects_short_stride_and_overflow() {
    for bpp in [4, 8, 16] {
        for padding in [0, 13] {
            let stride = (17 * bpp + padding) as isize;
            for sign in [1, -1] {
                assert_eq!(checked_world_layout(17,31,stride*sign,bpp),Ok((stride as usize,17*31*4)));
            }
        }
        assert!(checked_world_layout(17,31,(17*bpp-1) as isize,bpp).is_err());
    }
    for (w,h,s,bpp) in [(usize::MAX,1,4,4),(1,2,isize::MAX,4),
        (1,1,isize::MIN,4),(1,1,i32::MIN as isize,4),(1,1,16,3),
        (i32::MAX as usize,i32::MAX as usize, isize::MAX,16)] {
        assert!(checked_world_layout(w,h,s,bpp).is_err());
    }
    assert_eq!(checked_world_layout(0,0,0,4),Ok((0,0)));
}

fn layer_rgba(layer: &ae::Layer) -> Result<(Vec<f32>, usize, usize), ae::Error> {
    let w = layer.width();
    let h = layer.height();
    if w == 0 || h == 0 {
        return Err(ae::Error::BadCallbackParameter);
    }
    // DEEP alone does not distinguish 16-bit from float host worlds.
    let depth = depth_from_format(layer.pixel_format()?)?;
    let bpp = match depth {
        32 => 16,
        16 => 8,
        8 => 4,
        _ => return Err(ae::Error::BadCallbackParameter),
    };
    let (stride, count) = checked_world_layout(w, h, layer.row_bytes(), bpp)?;
    if unsafe { layer.data_ptr() }.is_null() { return Err(ae::Error::BadCallbackParameter); }
    let bottom_up = layer.row_bytes() < 0;
    let src = layer.buffer();
    let mut out = Vec::new();
    out.try_reserve_exact(count).map_err(|_| ae::Error::OutOfMemory)?;
    out.resize(count, 0.0);
    for y in 0..h {
        for x in 0..w {
            let s = storage_row(y, h, bottom_up) * stride + x * bpp;
            let d = (y * w + x) * 4;
            out[d..d + 4].copy_from_slice(&decode_argb(&src[s..s + bpp], depth));
        }
    }
    Ok((out, w, h))
}
fn output_rgb(rgb: f32, alpha: f32, depth: i16) -> f32 {
    let premultiplied = rgb * alpha;
    if depth == 32 { premultiplied } else { premultiplied.clamp(0.0, 1.0) }
}

// Layer::buffer exposes the lowest physical address even for negative rowbytes.
fn storage_row(y: usize, height: usize, bottom_up: bool) -> usize {
    if bottom_up { height - 1 - y } else { y }
}

#[test]
fn signed_rows_preserve_logical_order_and_padding() {
    for (depth, bpp) in [(8, 4), (16, 8), (32, 16)] {
        for h in [1usize, 3, 7] {
            for input_bottom_up in [false, true] {
                for output_bottom_up in [false, true] {
                    let stride = 3 * bpp + 13;
                    let mut input = vec![211u8; stride * h];
                    for physical in 0..h {
                        let logical = if input_bottom_up { h - 1 - physical } else { physical };
                        for x in 0..3 {
                            let value = (logical * 3 + x + 1) as f32 / 32.0;
                            let off = physical * stride + x * bpp;
                            encode_argb(&[value, 0.25, 0.5, 1.0], &mut input[off..off+bpp], depth);
                        }
                    }
                    let mut output = vec![211u8; stride * h];
                    for y in 0..h {
                        for x in 0..3 {
                            let s = storage_row(y,h,input_bottom_up)*stride+x*bpp;
                            let rgba = decode_argb(&input[s..s+bpp],depth);
                            let expected = (y * 3 + x + 1) as f32 / 32.0;
                            assert!((rgba[0]-expected).abs() <= 1.0/255.0,
                                "depth={depth} h={h} input_bottom_up={input_bottom_up} y={y}");
                            let d = storage_row(y,h,output_bottom_up)*stride+x*bpp;
                            encode_argb(&rgba,&mut output[d..d+bpp],depth);
                        }
                    }
                    for physical in 0..h {
                        let logical = if output_bottom_up { h-1-physical } else { physical };
                        let source = if input_bottom_up { h-1-logical } else { logical };
                        assert_eq!(&output[physical*stride..physical*stride+3*bpp],
                            &input[source*stride..source*stride+3*bpp]);
                        assert!(output[physical*stride+3*bpp..(physical+1)*stride].iter().all(|&b|b==211));
                    }
                }
            }
        }
    }
}

fn decode_argb(src: &[u8], depth: i16) -> [f32; 4] {
    let read = |c: usize| match depth {
        32 => f32::from_ne_bytes(src[c * 4..c * 4 + 4].try_into().unwrap()),
        16 => u16::from_ne_bytes(src[c * 2..c * 2 + 2].try_into().unwrap()) as f32 / 32768.0,
        _ => src[c] as f32 / 255.0,
    };
    let a = read(0).clamp(0.0, 1.0);
    let inv = if a > 1e-6 { 1.0 / a } else { 1.0 };
    [read(1) * inv, read(2) * inv, read(3) * inv, a]
}

fn encode_argb(src: &[f32], dst: &mut [u8], depth: i16) {
    let a = src[3].clamp(0.0, 1.0);
    for (dc, sc) in [3usize, 0, 1, 2].into_iter().enumerate() {
        let v = if sc == 3 { a } else { output_rgb(src[sc], a, depth) };
        match depth {
            32 => dst[dc * 4..dc * 4 + 4].copy_from_slice(&v.to_ne_bytes()),
            16 => dst[dc * 2..dc * 2 + 2].copy_from_slice(&((v * 32768.0).round() as u16).to_ne_bytes()),
            _ => dst[dc] = (v * 255.0).round() as u8,
        }
    }
}

#[test]
fn native_integer_codecs_preserve_low_bits_and_partial_alpha() {
    for alpha in [1u16, 129, 16384, 32768] {
        for value in 0..=alpha {
            let channels = [alpha, value, alpha - value, value / 2];
            let mut bytes = [0u8; 8];
            for (index, channel) in channels.into_iter().enumerate() {
                bytes[index * 2..index * 2 + 2].copy_from_slice(&channel.to_ne_bytes());
            }
            let decoded = decode_argb(&bytes, 16);
            if alpha == 32768 { assert_eq!(decoded[0], value as f32 / 32768.0); }
            let mut restored = [0u8; 8];
            encode_argb(&decoded, &mut restored, 16);
            assert_eq!(restored, bytes, "alpha={alpha} value={value}");
        }
    }
    for alpha in 1..=255u8 {
        for value in 0..=alpha {
            let bytes = [alpha, value, alpha - value, value / 2];
            let mut restored = [0u8; 4];
            encode_argb(&decode_argb(&bytes, 8), &mut restored, 8);
            assert_eq!(restored, bytes);
        }
    }
}

#[test]
fn float_output_preserves_signed_hdr_and_integer_output_remains_bounded() {
    assert_eq!(output_rgb(8.0, 0.5, 32), 4.0);
    assert_eq!(output_rgb(-2.0, 0.5, 32), -1.0);
    for depth in [8, 16] {
        assert_eq!(output_rgb(8.0, 0.5, depth), 1.0);
        assert_eq!(output_rgb(-2.0, 0.5, depth), 0.0);
        assert_eq!(output_rgb(0.5, 0.5, depth), 0.25);
    }
}

fn write_rgba(src: &[f32], layer: &mut ae::Layer, w: usize, h: usize) -> Result<(), ae::Error> {
    if w != layer.width() || h != layer.height() { return Err(ae::Error::BadCallbackParameter); }
    let depth = depth_from_format(layer.pixel_format()?)?;
    let bpp = match depth {
        32 => 16,
        16 => 8,
        8 => 4,
        _ => return Err(ae::Error::BadCallbackParameter),
    };
    let (stride, count) = checked_world_layout(w, h, layer.row_bytes(), bpp)?;
    if src.len() != count { return Err(ae::Error::BadCallbackParameter); }
    if count == 0 { return Ok(()); }
    if unsafe { layer.data_ptr() }.is_null() { return Err(ae::Error::BadCallbackParameter); }
    let bottom_up = layer.row_bytes() < 0;
    let dst = layer.buffer_mut();
    for y in 0..h {
        for x in 0..w {
            let s = (y * w + x) * 4;
            let d = storage_row(y, h, bottom_up) * stride + x * bpp;
            encode_argb(&src[s..s + 4], &mut dst[d..d + bpp], depth);
        }
    }
    Ok(())
}
fn render_layer(
    input: &ae::Layer,
    output: &mut ae::Layer,
    p: &EdgeParams,
    use_gpu: bool,
) -> Result<(), ae::Error> {
    let (rgba, w, h) = layer_rgba(input)?;
    let mut rendered = if use_gpu {
        gpu::render(&rgba, w, h, p).unwrap_or_else(|_| core::render_rgba(&rgba, w, h, p))
    } else {
        core::render_rgba(&rgba, w, h, p)
    };
    let ow = output.width();
    let oh = output.height();
    if ow != w || oh != h {
        rendered = crop_rgba(&rendered,w,h,ow,oh,(0,0),(0,0))?;
    }
    write_rgba(&rendered, output, ow, oh)
}
