use after_effects as ae;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Amount,
    Size,
    Shadows,
    Midtones,
    Highlights,
    Seed,
    Monochrome,
}

#[derive(Clone, Copy)]
struct GrainParams {
    amount: f32,
    size: usize,
    shadows: f32,
    midtones: f32,
    highlights: f32,
    seed: u32,
    monochrome: bool,
}

#[derive(Clone, Copy, Default)]
struct PreRenderState {
    time: i32,
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
        params.add(Params::Amount, "Amount", float_slider(0.0, 100.0, 15.0, 1))?;
        params.add(
            Params::Size,
            "Size",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(1);
                f.set_valid_max(32);
                f.set_slider_min(1);
                f.set_slider_max(12);
                f.set_default(2);
            }),
        )?;
        params.add(
            Params::Shadows,
            "Shadows",
            float_slider(0.0, 200.0, 130.0, 0),
        )?;
        params.add(
            Params::Midtones,
            "Midtones",
            float_slider(0.0, 200.0, 100.0, 0),
        )?;
        params.add(
            Params::Highlights,
            "Highlights",
            float_slider(0.0, 200.0, 45.0, 0),
        )?;
        params.add(
            Params::Seed,
            "Seed",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(10000);
                f.set_slider_min(0);
                f.set_slider_max(10000);
                f.set_default(0);
            }),
        )?;
        params.add(
            Params::Monochrome,
            "Monochrome",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(true);
                f.set_label("Monochrome");
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
            ae::Command::About => out_data.set_return_msg(
                "LumaGrain v1.0\rLightweight luminance-weighted grain.\rWritten in Rust.",
            ),
            ae::Command::Render {
                in_layer,
                mut out_layer,
            } => {
                let p = read_params(params, in_data.current_time())?;
                render(&p, &in_layer, &mut out_layer)?;
            }
            ae::Command::SmartPreRender { mut extra } => pre_render(&in_data, &mut extra)?,
            ae::Command::SmartRender { extra } => smart_render(&extra, params)?,
            ae::Command::SmartRenderGpu { .. } => return Err(ae::Error::BadCallbackParameter),
            _ => {}
        }
        Ok(())
    }
}

fn float_slider(min: f32, max: f32, default: f32, precision: i16) -> ae::FloatSliderDef<'static> {
    ae::FloatSliderDef::setup(|f| {
        f.set_valid_min(min);
        f.set_valid_max(max);
        f.set_slider_min(min);
        f.set_slider_max(max);
        f.set_default(default as f64);
        f.set_precision(precision);
    })
}

fn read_params(params: &ae::Parameters<Params>, time: i32) -> Result<GrainParams, ae::Error> {
    let pct = |id| -> Result<f32, ae::Error> {
        Ok(params.get(id)?.as_float_slider()?.value() as f32 / 100.0)
    };
    Ok(GrainParams {
        amount: pct(Params::Amount)?,
        size: params.get(Params::Size)?.as_slider()?.value().clamp(1, 32) as usize,
        shadows: pct(Params::Shadows)?,
        midtones: pct(Params::Midtones)?,
        highlights: pct(Params::Highlights)?,
        seed: params.get(Params::Seed)?.as_slider()?.value().max(0) as u32
            ^ (time as u32).wrapping_mul(0x9e37_79b9),
        monochrome: params.get(Params::Monochrome)?.as_checkbox()?.value(),
    })
}

fn luma_weight(luma: f32, p: &GrainParams) -> f32 {
    let l = luma.clamp(0.0, 1.0);
    let shadow = (1.0 - l).powi(2);
    let highlight = l.powi(2);
    let midtone = (1.0 - (2.0 * l - 1.0).abs()).max(0.0);
    let sum = shadow + midtone + highlight;
    (shadow * p.shadows + midtone * p.midtones + highlight * p.highlights) / sum.max(1.0e-6)
}

fn hash(mut v: u32) -> f32 {
    v ^= v >> 16;
    v = v.wrapping_mul(0x7feb_352d);
    v ^= v >> 15;
    v = v.wrapping_mul(0x846c_a68b);
    v ^= v >> 16;
    (v as f32 / u32::MAX as f32) * 2.0 - 1.0
}

fn noise(x: usize, y: usize, channel: u32, p: &GrainParams) -> f32 {
    let gx = (x / p.size) as u32;
    let gy = (y / p.size) as u32;
    hash(gx.wrapping_mul(0x1f12_3bb5) ^ gy.wrapping_mul(0x5f35_6495) ^ p.seed ^ channel)
}

fn process_pixel(px: [u8; 4], x: usize, y: usize, p: &GrainParams) -> [u8; 4] {
    if p.amount==0.0 { return px; }
    if px[0]==0 { return [0;4]; }
    let alpha=px[0] as f32;
    // AE RGB is premultiplied: use straight luminance, then scale grain by alpha.
    let luma = (0.2126 * px[1] as f32 + 0.7152 * px[2] as f32 + 0.0722 * px[3] as f32) / alpha;
    let scale = p.amount * luma_weight(luma, p) * 64.0;
    let mono = noise(x, y, 0, p) * scale;
    let apply = |v: u8, c: u32| -> u8 {
        let n = if p.monochrome {
            mono
        } else {
            noise(x, y, c, p) * scale
        };
        (v as f32 + n*(alpha/255.0)).clamp(0.0, alpha).round() as u8
    };
    [px[0], apply(px[1], 1), apply(px[2], 2), apply(px[3], 3)]
}

fn checked_extent(w: usize, h: usize, stride: usize) -> Result<usize, ae::Error> {
    if w == 0 || h == 0 { return Ok(0); }
    let row=w.checked_mul(4).ok_or(ae::Error::OutOfMemory)?;
    if stride < row { return Err(ae::Error::BadCallbackParameter); }
    (h-1).checked_mul(stride).and_then(|v|v.checked_add(row)).ok_or(ae::Error::OutOfMemory)
}

fn geometry(layer: &ae::Layer) -> Result<(usize, usize, usize), ae::Error> {
    let w = layer.width();
    let h = layer.height();
    if w>i32::MAX as usize || h>i32::MAX as usize || layer.row_bytes()==i32::MIN as isize {
        return Err(ae::Error::BadCallbackParameter);
    }
    if w==0 || h==0 { return Ok((w,h,0)); }
    let stride = layer.buffer_stride();
    checked_extent(w,h,stride)?;
    // The SDK constructs a slice covering the complete stride * height.
    let need = stride.checked_mul(h).ok_or(ae::Error::OutOfMemory)?;
    if need>isize::MAX as usize || unsafe {layer.data_ptr()}.is_null()
        || layer.pixel_format()?!=ae::PixelFormat::Argb32 {
        return Err(ae::Error::BadCallbackParameter);
    }
    Ok((w, h, stride))
}

fn render(p: &GrainParams, input: &ae::Layer, output: &mut ae::Layer) -> Result<(), ae::Error> {
    let (w, h, src_stride) = geometry(input)?;
    let (ow, oh, dst_stride) = geometry(output)?;
    if (w,h)!=(ow,oh) {
        return Err(ae::Error::BadCallbackParameter);
    }
    if w==0 || h==0 { return Ok(()); }
    let src_reverse=input.row_bytes()<0;
    let dst_reverse=output.row_bytes()<0;
    render_rows(p,input.buffer(),output.buffer_mut(),w,h,src_stride,dst_stride,src_reverse,dst_reverse)
}

fn render_rows(p: &GrainParams, src: &[u8], dst: &mut [u8], w: usize, h: usize,
    src_stride: usize, dst_stride: usize, src_reverse: bool, dst_reverse: bool) -> Result<(),ae::Error> {
    if src.len()<checked_extent(w,h,src_stride)? || dst.len()<checked_extent(w,h,dst_stride)? {
        return Err(ae::Error::BadCallbackParameter);
    }
    if w==0 || h==0 { return Ok(()); }
    for y in 0..h {
        for x in 0..w {
            let s = (if src_reverse {h-1-y} else {y}) * src_stride + x * 4;
            let d = (if dst_reverse {h-1-y} else {y}) * dst_stride + x * 4;
            let out = process_pixel([src[s], src[s + 1], src[s + 2], src[s + 3]], x, y, p);
            dst[d..d + 4].copy_from_slice(&out);
        }
    }
    Ok(())
}

fn pre_render(in_data: &ae::InData, extra: &mut ae::pf::PreRenderExtra) -> Result<(), ae::Error> {
    let req = extra.output_request();
    let cb = extra.callbacks();
    let r = cb.checkout_layer(
        0,
        0,
        &req,
        in_data.current_time(),
        in_data.time_step(),
        in_data.time_scale(),
    )?;
    extra.set_result_rect(r.result_rect.into());
    extra.set_max_result_rect(r.max_result_rect.into());
    extra.set_pre_render_data(PreRenderState {
        time: in_data.current_time(),
    });
    Ok(())
}

fn with_pixel_checkin<T>(work: impl FnOnce()->Result<T,ae::Error>, checkin: impl FnOnce()->Result<(),ae::Error>)->Result<T,ae::Error> {
    let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(work))
        .unwrap_or(Err(ae::Error::InternalStructDamaged));
    let released=checkin();
    match result { Err(error)=>Err(error), Ok(value)=>released.map(|()|value) }
}

fn smart_render(
    extra: &ae::pf::SmartRenderExtra,
    params: &ae::Parameters<Params>,
) -> Result<(), ae::Error> {
    let cb = extra.callbacks();
    let input = cb.checkout_layer_pixels(0)?.ok_or(ae::Error::Generic)?;
    with_pixel_checkin(|| {
        let mut output = cb.checkout_output()?.ok_or(ae::Error::Generic)?;
        let time = extra
            .pre_render_data::<PreRenderState>()
            .map(|state| state.time)
            .unwrap_or(0);
        let p = read_params(params, time)?;
        render(&p, &input, &mut output)
    }, || cb.checkin_layer_pixels(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checkin_runs_once_and_preserves_primary_error() {
        for outcome in 0..3 {for release_fails in [false,true] {
            let calls=std::cell::Cell::new(0);
            let result=with_pixel_checkin(|| match outcome {
                0=>Ok(42), 1=>Err(ae::Error::BadCallbackParameter), _=>panic!("injected render panic"),
            },|| {calls.set(calls.get()+1); if release_fails {Err(ae::Error::Generic)} else {Ok(())}});
            assert_eq!(calls.get(),1);
            let expected=match outcome {
                0=>if release_fails {Err(ae::Error::Generic)} else {Ok(42)},
                1=>Err(ae::Error::BadCallbackParameter), _=>Err(ae::Error::InternalStructDamaged),
            };
            assert_eq!(result,expected);
        }}
    }
    #[test]
    fn rows_validate_before_write_and_use_logical_noise_coordinates() {
        let src:Vec<u8>=(0..30).collect();
        for sr in [false,true] {for dr in [false,true] {
            let mut dst=vec![99;30];
            render_rows(&p(),&src,&mut dst,2,3,10,10,sr,dr).unwrap();
            for y in 0..3 {for x in 0..2 {
                let s=(if sr {2-y} else {y})*10+x*4;
                let d=(if dr {2-y} else {y})*10+x*4;
                assert_eq!(&dst[d..d+4],&process_pixel(src[s..s+4].try_into().unwrap(),x,y,&p()));
            }}
            for row in dst.chunks_exact(10) {assert_eq!(&row[8..],&[99;2]);}
        }}
        let mut dst=vec![99;30];
        assert!(render_rows(&p(),&src[..8],&mut dst,2,3,10,10,false,false).is_err());
        assert_eq!(dst,vec![99;30]);
        assert!(checked_extent(2,3,7).is_err());
        assert!(checked_extent(usize::MAX,2,usize::MAX).is_err());
        render_rows(&p(),&[],&mut [],0,usize::MAX,0,0,false,false).unwrap();
    }
    fn p() -> GrainParams {
        GrainParams {
            amount: 1.0,
            size: 2,
            shadows: 1.5,
            midtones: 1.0,
            highlights: 0.25,
            seed: 7,
            monochrome: true,
        }
    }
    #[test]
    fn reused_rows_survive_resize_seek_and_orientation_changes() {
        let mut reused = vec![0xa5; 8192];
        let mut first = None;
        let transitions = [(17,31,0u32),(1,1,9),(37,35,1),(1,31,9),(17,31,0)];
        for cycle in 0..4 {
            for (step, &(w,h,time)) in transitions.iter().enumerate() {
                let mut params = p();
                params.seed = 7 ^ time.wrapping_mul(0x9e37_79b9);
                params.size = if time == 0 {1} else {32};
                params.monochrome = time == 0;
                let src_stride = w * 4 + 7;
                let dst_stride = w * 4 + 11;
                let sr = (cycle + step) % 2 == 0;
                let dr = (cycle + step) % 3 == 0;
                let mut src = vec![0x5a; src_stride * h];
                for y in 0..h { for x in 0..w {
                    let alpha = [0,85,170,255][(x+y)%4];
                    let pixel = [alpha,alpha/5,alpha/5*2,alpha/5*4];
                    let offset = (if sr {h-1-y} else {y}) * src_stride + x*4;
                    src[offset..offset+4].copy_from_slice(&pixel);
                }}
                let before = reused.clone();
                assert!(render_rows(&params,&src[..3],&mut reused,w,h,src_stride,dst_stride,sr,dr).is_err());
                assert_eq!(reused, before);
                let mut fresh = vec![0x3c; dst_stride*h];
                render_rows(&params,&src,&mut fresh,w,h,src_stride,dst_stride,sr,dr).unwrap();
                render_rows(&params,&src,&mut reused,w,h,src_stride,dst_stride,sr,dr).unwrap();
                let mut logical = Vec::new();
                for y in 0..h {
                    let row = (if dr {h-1-y} else {y}) * dst_stride;
                    assert_eq!(&reused[row..row+w*4], &fresh[row..row+w*4]);
                    assert_eq!(&reused[row+w*4..row+dst_stride], &before[row+w*4..row+dst_stride]);
                    logical.extend_from_slice(&reused[row..row+w*4]);
                }
                assert_eq!(&reused[dst_stride*h..], &before[dst_stride*h..]);
                if time == 0 {
                    if let Some(reference) = &first { assert_eq!(&logical, reference); }
                    else { first = Some(logical); }
                }
            }
        }
    }
    #[test]
    fn shadows_receive_more_grain_than_highlights() {
        assert!(luma_weight(0.05, &p()) > luma_weight(0.95, &p()));
    }
    #[test]
    fn noise_is_deterministic() {
        assert_eq!(noise(4, 8, 0, &p()), noise(4, 8, 0, &p()));
    }
    #[test]
    fn grain_respects_premultiplied_alpha() {
        for monochrome in [false,true] {
            let mut q=p(); q.monochrome=monochrome;
            for y in 0..8 {for x in 0..8 {
                for alpha in [0u8,5,85,170,255] {
                    // Straight RGB [51,102,204] has exact premultiplied values at these alphas.
                    let input=[alpha,alpha/5,alpha/5*2,alpha/5*4];
                    let opaque=process_pixel([255,51,102,204],x,y,&q);
                    let actual=process_pixel(input,x,y,&q);
                    assert_eq!(actual[0],alpha);
                    for c in 1..4 {
                        let expected=(opaque[c] as f32*alpha as f32/255.0).round() as u8;
                        assert!(actual[c].abs_diff(expected)<=1,"alpha={alpha} channel={c} actual={} expected={expected}",actual[c]);
                    }
                }
            }}
        }
        let mut q=p(); q.amount=0.0;
        assert_eq!(process_pixel([0,19,27,55],0,0,&q),[0,19,27,55]);
    }
    #[test]
    fn zero_amount_is_passthrough() {
        let mut q = p();
        q.amount = 0.0;
        assert_eq!(
            process_pixel([255, 10, 20, 30], 1, 1, &q),
            [255, 10, 20, 30]
        );
    }
}
