use after_effects as ae;

mod minimax;

// ---- Parameter IDs ----

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Amount,
    RatioX,
    RatioY,
    Direction,
    Channel,
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
        params.add(
            Params::Amount,
            "Amount",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(-100.0);
                f.set_valid_max(100.0);
                f.set_slider_min(-20.0);
                f.set_slider_max(20.0);
                f.set_default(0.0);
                f.set_precision(2);
            }),
        )?;

        params.add(
            Params::RatioX,
            "Ratio X",
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
            Params::RatioY,
            "Ratio Y",
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
            Params::Direction,
            "Direction",
            ae::PopupDef::setup(|f| {
                f.set_options(&["Horizontal", "Vertical", "Both"]);
                f.set_default(3); // Both
            }),
        )?;

        params.add(
            Params::Channel,
            "Channel",
            ae::PopupDef::setup(|f| {
                f.set_options(&["Color", "Alpha", "Color and Alpha"]);
                f.set_default(1); // Color
            }),
        )?;

        params.add(
            Params::RepeatEdge,
            "Repeat Edge Pixels",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(true);
                f.set_label("Repeat");
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

        params.add(Params::MapLayer, "Radius Map", ae::LayerDef::setup(|_f| {}))?;

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
                    "MinimaxMap v1.1\rSigned linear Minimax with Radius Map support.\rNegative Amount = Minimum, positive Amount = Maximum.\rWritten in Rust.",
                );
            }
            ae::Command::Render {
                in_layer,
                mut out_layer,
            } => {
                render_cpu(params, &in_data, &in_layer, &mut out_layer)?;
            }
            ae::Command::SmartPreRender { mut extra } => {
                smart_pre_render(&in_data, params, &mut extra)?;
            }
            ae::Command::SmartRender { extra } => {
                smart_render_cpu(&extra, &in_data, params)?;
            }
            ae::Command::SmartRenderGpu { extra } => {
                smart_render_cpu(&extra, &in_data, params)?;
            }
            _ => {}
        }
        Ok(())
    }
}

// ---- Extract parameters ----

pub struct MinimaxParams {
    pub amount: f32,
    pub ratio_x: f32,
    pub ratio_y: f32,
    pub direction: i32,
    pub channel: i32,
    pub repeat_edge: bool,
    pub mix: f32,
    pub invert_map: bool,
}

fn get_params(params: &ae::Parameters<Params>) -> Result<MinimaxParams, ae::Error> {
    Ok(MinimaxParams {
        amount: params
            .get(Params::Amount)?
            .as_float_slider()?
            .value()
            .clamp(-100.0, 100.0) as f32,
        ratio_x: params.get(Params::RatioX)?.as_float_slider()?.value() as f32,
        ratio_y: params.get(Params::RatioY)?.as_float_slider()?.value() as f32,
        direction: params.get(Params::Direction)?.as_popup()?.value() as i32,
        channel: params.get(Params::Channel)?.as_popup()?.value() as i32,
        repeat_edge: params.get(Params::RepeatEdge)?.as_checkbox()?.value(),
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

fn copy_argb8_rows(buf: &[u8], w: usize, h: usize, stride: usize, reverse: bool) -> Result<Vec<u8>, ae::Error> {
    let row_len = w.checked_mul(4).ok_or(ae::Error::OutOfMemory)?;
    let count = row_len.checked_mul(h).ok_or(ae::Error::OutOfMemory)?;
    if w == 0 || h == 0 { return Ok(Vec::new()); }
    let required = (h-1).checked_mul(stride).and_then(|v|v.checked_add(row_len)).ok_or(ae::Error::OutOfMemory)?;
    if stride < row_len || required > buf.len() { return Err(ae::Error::BadCallbackParameter); }
    let mut flat = Vec::new();
    flat.try_reserve_exact(count).map_err(|_|ae::Error::OutOfMemory)?;
    for y in 0..h {
        let row = if reverse { h-1-y } else { y };
        flat.extend_from_slice(&buf[row*stride..row*stride+row_len]);
    }
    Ok(flat)
}

#[test]
fn argb_rows_reject_bad_geometry_and_preserve_padding_and_orientation() {
    assert!(copy_argb8_rows(&[],usize::MAX,2,4,false).is_err());
    assert!(copy_argb8_rows(&[0;8],2,1,4,false).is_err());
    assert!(copy_argb8_rows(&[0;7],1,2,4,false).is_err());
    let padded=[1,2,3,4,99,99,5,6,7,8];
    assert_eq!(copy_argb8_rows(&padded,1,2,6,false).unwrap(),[1,2,3,4,5,6,7,8]);
    assert_eq!(copy_argb8_rows(&padded,1,2,6,true).unwrap(),[5,6,7,8,1,2,3,4]);
    assert!(copy_argb8_rows(&[],0,0,0,false).unwrap().is_empty());
}

fn layer_to_flat(layer: &ae::Layer) -> Result<(Vec<u8>, usize, usize), ae::Error> {
    let w = layer.width() as usize;
    let h = layer.height() as usize;
    let stride = layer.buffer_stride();
    if w == 0 || h == 0 { return Ok((Vec::new(),w,h)); }
    stride.checked_mul(h).ok_or(ae::Error::OutOfMemory)?;
    if unsafe { layer.data_ptr() }.is_null() || layer.pixel_format()? != ae::PixelFormat::Argb32 {
        return Err(ae::Error::BadCallbackParameter);
    }
    let flat=copy_argb8_rows(layer.buffer(),w,h,stride,layer.row_bytes()<0)?;
    Ok((flat,w,h))
}

fn write_argb8_rows(flat: &[u8], buf: &mut [u8], w: usize, h: usize, stride: usize, reverse: bool) -> Result<(), ae::Error> {
    let row_len=w.checked_mul(4).ok_or(ae::Error::OutOfMemory)?;
    let count=row_len.checked_mul(h).ok_or(ae::Error::OutOfMemory)?;
    if w==0 || h==0 { return Ok(()); }
    let required=(h-1).checked_mul(stride).and_then(|v|v.checked_add(row_len)).ok_or(ae::Error::OutOfMemory)?;
    if flat.len()<count || stride<row_len || buf.len()<required { return Err(ae::Error::BadCallbackParameter); }
    for y in 0..h {
        let src_off=y*row_len;
        let dst_off=(if reverse { h-1-y } else { y })*stride;
        buf[dst_off..dst_off+row_len].copy_from_slice(&flat[src_off..src_off+row_len]);
    }
    Ok(())
}

#[test]
fn output_row_validation_is_atomic_and_padding_is_preserved() {
    let flat=[1,2,3,4,5,6,7,8];
    let mut short=[99;7];
    assert!(write_argb8_rows(&flat,&mut short,1,2,4,false).is_err());
    assert_eq!(short,[99;7]);
    let mut padded=[99;10];
    assert!(write_argb8_rows(&flat[..7],&mut padded,1,2,6,false).is_err());
    assert_eq!(padded,[99;10]);
    write_argb8_rows(&flat,&mut padded,1,2,6,false).unwrap();
    assert_eq!(padded,[1,2,3,4,99,99,5,6,7,8]);
    write_argb8_rows(&flat,&mut padded,1,2,6,true).unwrap();
    assert_eq!(padded,[5,6,7,8,99,99,1,2,3,4]);
    assert!(write_argb8_rows(&[],&mut [],usize::MAX,2,4,false).is_err());
}

fn flat_to_layer(flat: &[u8], layer: &mut ae::Layer, w: usize, h: usize) -> Result<(), ae::Error> {
    let stride=layer.buffer_stride();
    if w!=layer.width() || h!=layer.height() { return Err(ae::Error::BadCallbackParameter); }
    if w==0 || h==0 { return Ok(()); }
    stride.checked_mul(h).ok_or(ae::Error::OutOfMemory)?;
    if unsafe { layer.data_ptr() }.is_null() || layer.pixel_format()?!=ae::PixelFormat::Argb32 {
        return Err(ae::Error::BadCallbackParameter);
    }
    let reverse=layer.row_bytes()<0;
    write_argb8_rows(flat,layer.buffer_mut(),w,h,stride,reverse)
}

// ---- Luminance map ----

fn build_luminance_map(buf: &[u8], w: usize, h: usize, invert: bool) -> Option<Vec<f64>> {
    let count = w.checked_mul(h)?;
    if count == 0 || count.checked_mul(4)? > buf.len() { return None; }
    let mut map = Vec::new();
    map.try_reserve_exact(count).ok()?;
    map.resize(count, 0.0f64);
    for i in 0..count {
        let off = i * 4;
        let r = buf[off + 1] as f64;
        let g = buf[off + 2] as f64;
        let b = buf[off + 3] as f64;
        let luma = (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255.0;
        map[i] = if invert { 1.0 - luma } else { luma };
    }
    Some(map)
}

fn build_luminance_map_resample(
    buf: &[u8],
    mw: usize,
    mh: usize,
    w: usize,
    h: usize,
    invert: bool,
) -> Option<Vec<f64>> {
    let count = w.checked_mul(h)?;
    if count == 0 { return None; }
    let src_map = build_luminance_map(buf, mw, mh, invert)?;
    let mut map = Vec::new();
    map.try_reserve_exact(count).ok()?;
    map.resize(count, 0.0f64);
    for y in 0..h {
        let sy = (y.checked_mul(mh)? / h).min(mh - 1);
        for x in 0..w {
            let sx = (x.checked_mul(mw)? / w).min(mw - 1);
            map[y * w + x] = src_map[sy * mw + sx];
        }
    }
    Some(map)
}

// ---- Legacy Render ----

#[test]
fn empty_radius_map_does_not_panic() {
    assert!(build_luminance_map_resample(&[], 0, 0, 17, 31, false).is_none());
    assert!(build_luminance_map(&[255;3], 1, 1, false).is_none());
    assert!(build_luminance_map(&[], usize::MAX, 2, false).is_none());
    assert!(build_luminance_map_resample(&[255;4],1,1,0,31,false).is_none());
    let black = build_luminance_map_resample(&[255,0,0,0],1,1,17,31,false).unwrap();
    assert_eq!(black, vec![0.0;17*31]);
    assert_eq!(build_luminance_map(&[255,0,0,0],1,1,true),Some(vec![1.0]));
}

fn render_cpu(
    params: &ae::Parameters<Params>,
    in_data: &ae::InData,
    in_layer: &ae::Layer,
    out_layer: &mut ae::Layer,
) -> Result<(), ae::Error> {
    let mp = get_params(params)?;
    let (src, w, h) = layer_to_flat(in_layer)?;

    let luma_map = get_map_layer_luma(params, in_data, w, h, mp.invert_map);
    let result = minimax::minimax(&mp, &src, w, h, luma_map.as_deref());
    flat_to_layer(&result, out_layer, w, h)
}

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

// ---- SmartFX ----

const MAP_CHECKOUT_ID: i32 = 1;

struct InputPlane { rect: ae::Rect, input_id: u32, map_id: Option<u32> }

fn output_bounds(result: ae::Rect, maximum: ae::Rect, request: ae::Rect) -> (ae::Rect,ae::Rect) {
    let clipped=ae::Rect { left:result.left.max(request.left),top:result.top.max(request.top),
        right:result.right.min(request.right),bottom:result.bottom.min(request.bottom) };
    let clipped=if clipped.right<=clipped.left || clipped.bottom<=clipped.top {
        ae::Rect{left:0,top:0,right:0,bottom:0}
    } else { clipped };
    (clipped,maximum)
}

#[test]
fn maximum_output_bounds_do_not_shrink_with_roi() {
    let full=ae::Rect{left:-10,top:-20,right:100,bottom:80};
    let tuple=|r:ae::Rect|(r.left,r.top,r.right,r.bottom);
    for request in [ae::Rect{left:3,top:4,right:5,bottom:7},ae::Rect{left:200,top:0,right:220,bottom:20}] {
        let (_,maximum)=output_bounds(full,full,request);
        assert_eq!(tuple(maximum),tuple(full));
    }
    let (clipped,_)=output_bounds(full,full,ae::Rect{left:3,top:4,right:5,bottom:7});
    assert_eq!(tuple(clipped),(3,4,5,7));
    let (empty,_)=output_bounds(full,full,ae::Rect{left:200,top:0,right:220,bottom:20});
    assert_eq!(tuple(empty),(0,0,0,0));
}

fn dependency_rect(rect: ae::Rect, p: &MinimaxParams) -> ae::Rect {
    let horizontal=p.direction==1 || p.direction==3;
    let vertical=p.direction==2 || p.direction==3;
    let rx=if horizontal { (p.amount.abs()*p.ratio_x.max(0.0)).ceil() as i32 } else {0};
    let ry=if vertical { (p.amount.abs()*p.ratio_y.max(0.0)).ceil() as i32 } else {0};
    ae::Rect { left:rect.left.saturating_sub(rx),top:rect.top.saturating_sub(ry),
        right:rect.right.saturating_add(rx),bottom:rect.bottom.saturating_add(ry) }
}

fn crop_argb8(flat: &[u8], w: usize, h: usize, offset: (i64,i64), ow: usize, oh: usize) -> Result<Vec<u8>,ae::Error> {
    let count=w.checked_mul(h).and_then(|v|v.checked_mul(4)).ok_or(ae::Error::OutOfMemory)?;
    if flat.len()<count { return Err(ae::Error::BadCallbackParameter); }
    let n=ow.checked_mul(oh).and_then(|v|v.checked_mul(4)).ok_or(ae::Error::OutOfMemory)?;
    let mut out=Vec::new();
    out.try_reserve_exact(n).map_err(|_|ae::Error::OutOfMemory)?;
    out.resize(n,0);
    for y in 0..oh { for x in 0..ow {
        let sx=offset.0.saturating_add(x as i64);
        let sy=offset.1.saturating_add(y as i64);
        if sx>=0 && sy>=0 && sx<(w as i64) && sy<(h as i64) {
            let s=(sy as usize*w+sx as usize)*4;
            let d=(y*ow+x)*4;
            out[d..d+4].copy_from_slice(&flat[s..s+4]);
        }
    }}
    Ok(out)
}

#[test]
fn roi_dependency_rounds_up_and_crop_respects_offsets() {
    let p=MinimaxParams { amount:-1.25,ratio_x:2.0,ratio_y:1.0,direction:3,channel:1,repeat_edge:true,mix:1.0,invert_map:false };
    let r=dependency_rect(ae::Rect{left:3,top:4,right:5,bottom:7},&p);
    assert_eq!((r.left,r.top,r.right,r.bottom),(0,2,8,9));
    let src=[255,10,0,0,255,20,0,0,255,30,0,0];
    assert_eq!(crop_argb8(&src,3,1,(1,0),2,1).unwrap(),src[4..]);
    assert_eq!(crop_argb8(&src,3,1,(-1,0),2,1).unwrap(),[0,0,0,0,255,10,0,0]);
    assert!(crop_argb8(&[],3,1,(0,0),1,1).is_err());
}

fn smart_pre_render(
    in_data: &ae::InData,
    params: &ae::Parameters<Params>,
    extra: &mut ae::pf::PreRenderExtra,
) -> Result<(), ae::Error> {
    let req = extra.output_request();
    let mut input_req = extra.output_request();
    input_req.rect=dependency_rect(req.rect.into(),&get_params(params)?).into();
    let cb = extra.callbacks();

    let in_result = cb.checkout_layer(
        0,
        0,
        &input_req,
        in_data.current_time(),
        in_data.time_step(),
        in_data.time_scale(),
    )?;

    // Probe metadata first. A mapped radius must be evaluated in full-plane
    // coordinates, not stretched independently to each requested tile.
    let map_probe = cb.checkout_layer(8, MAP_CHECKOUT_ID, &req,
        in_data.current_time(),in_data.time_step(),in_data.time_scale()).ok();
    let map_bounds = map_probe.map(|v| ae::Rect::from(v.max_result_rect));
    let mut plane=InputPlane { rect:in_result.result_rect.into(),input_id:0,map_id:None };
    if let Some(bounds) = map_bounds.filter(|r|r.right>r.left && r.bottom>r.top) {
        let mut map_req=extra.output_request();
        map_req.rect=bounds.into();
        let got_map=cb.checkout_layer(8,2,&map_req,
            in_data.current_time(),in_data.time_step(),in_data.time_scale())?;
        if got_map.result_rect.right>got_map.result_rect.left && got_map.result_rect.bottom>got_map.result_rect.top {
            let mut full_req=extra.output_request();
            full_req.rect=in_result.max_result_rect;
            let got=cb.checkout_layer(0,3,&full_req,
                in_data.current_time(),in_data.time_step(),in_data.time_scale())?;
            plane=InputPlane { rect:got.result_rect.into(),input_id:3,map_id:Some(2) };
        }
    }
    extra.set_pre_render_data(plane);

    let (res,max_res)=output_bounds(in_result.result_rect.into(),in_result.max_result_rect.into(),req.rect.into());
    extra.set_result_rect(res);
    extra.set_max_result_rect(max_res);

    Ok(())
}

fn with_checkin<T, E>(render: impl FnOnce() -> Result<T, E>, checkin: impl FnOnce() -> Result<(), E>) -> Result<T, E> {
    let result = render();
    let cleanup = checkin();
    result.and_then(|value| cleanup.map(|()| value))
}

#[test]
fn input_checkin_runs_once_even_when_render_fails() {
    use std::cell::Cell;
    for render in [Ok(7), Err("render")] {
        for cleanup in [Ok(()), Err("checkin")] {
            let calls = Cell::new(0);
            let actual = with_checkin(|| render, || {
                calls.set(calls.get()+1);
                cleanup
            });
            assert_eq!(calls.get(),1);
            assert_eq!(actual,render.and_then(|v| cleanup.map(|()|v)));
        }
    }
}

fn smart_render_cpu(
    extra: &ae::pf::SmartRenderExtra,
    in_data: &ae::InData,
    params: &ae::Parameters<Params>,
) -> Result<(), ae::Error> {
    let mp = get_params(params)?;
    let cb = extra.callbacks();

    let metadata=extra.pre_render_data::<InputPlane>();
    let input_id=metadata.as_ref().map(|p|p.input_id).unwrap_or(0);
    let map_id=metadata.as_ref().and_then(|p|p.map_id);
    let input_world = cb.checkout_layer_pixels(input_id)?.ok_or(ae::Error::Generic)?;
    with_checkin(|| {
    let mut output_world = cb.checkout_output()?.ok_or(ae::Error::Generic)?;
    let (src, w, h) = layer_to_flat(&input_world)?;

    let invert = mp.invert_map;
    let luma_map = if let Some((map_id,map_world)) = map_id.and_then(|id|cb.checkout_layer_pixels(id).ok().flatten().map(|world|(id,world))) {
        with_checkin(|| {
        let (map_flat, mw, mh) = match layer_to_flat(&map_world) {
            Ok(frame) => frame,
            Err(_) => return Ok(None),
        };
        Ok(if mw == w && mh == h {
            build_luminance_map(&map_flat, w, h, invert)
        } else {
            build_luminance_map_resample(
                &map_flat, mw, mh, w, h, invert,
            )
        })
        }, || cb.checkin_layer_pixels(map_id))?
    } else {
        None
    };

    let result = minimax::minimax(&mp, &src, w, h, luma_map.as_deref());
    let plane=extra.pre_render_data::<InputPlane>().map(|v|v.rect)
        .unwrap_or(ae::Rect{left:0,top:0,right:w as i32,bottom:h as i32});
    let offset=(i64::from(in_data.output_origin().h)-i64::from(plane.left),
        i64::from(in_data.output_origin().v)-i64::from(plane.top));
    let (ow,oh)=(output_world.width(),output_world.height());
    if offset==(0,0) && (w,h)==(ow,oh) {
        flat_to_layer(&result,&mut output_world,w,h)?;
    } else {
        let cropped=crop_argb8(&result,w,h,offset,ow,oh)?;
        flat_to_layer(&cropped,&mut output_world,ow,oh)?;
    }

    Ok(())
    }, || cb.checkin_layer_pixels(input_id))
}
