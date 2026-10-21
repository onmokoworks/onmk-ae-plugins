use after_effects as ae;

mod filters;
mod pixels;
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
                smart_pre_render(&in_data, params, &mut extra)?;
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

// ---- Build luminance map from a flat ARGB buffer ----

fn build_luminance_map<T: pixels::Channel>(buf: &[T], w: usize, h: usize, invert: bool) -> Result<Vec<f64>,ae::Error> {
    let count=w.checked_mul(h).filter(|&n|n<=isize::MAX as usize/8).ok_or(ae::Error::BadCallbackParameter)?;
    if count.checked_mul(4)!=Some(buf.len()) {return Err(ae::Error::BadCallbackParameter);}
    let mut map=Vec::new();
    map.try_reserve_exact(count).map_err(|_|ae::Error::OutOfMemory)?;
    map.resize(count,0.0);
    for i in 0..(w * h) {
        let off = i * 4;
        let r = buf[off + 1].normalized() * 255.0;
        let g = buf[off + 2].normalized() * 255.0;
        let b = buf[off + 3].normalized() * 255.0;
        let luma = ((0.2126 * r + 0.7152 * g + 0.0722 * b) / 255.0).clamp(0.0, 1.0);
        map[i] = if invert { 1.0 - luma } else { luma };
    }
    Ok(map)
}

// ---- Process filters on flat buffer ----

#[test]
fn spatial_radius_map_matches_constant_radius_bank() {
    fn check<T: filters::Channel + PartialEq + std::fmt::Debug>(source: Vec<T>, w:usize, h:usize) {
        // Literal expectations straddle every effective-radius transition for base=4.
        let levels=[0.0,0.374999,0.375,0.375001,0.624999,0.625,0.625001,0.874999,0.875,0.875001,1.0];
        let radii=[1usize,1,2,2,2,3,3,3,4,4,4];
        let map:Vec<f64>=(0..w*h).map(|i|levels[(i%w+3*(i/w))%levels.len()]).collect();
        for mode in 1..=3 {
            for iterations in [1,2] {
                let fp=FilterParams {filter_type:mode,radius:4,edge_preserve:50.0,iterations,mix:1.0,invert_map:false};
                let actual=process_filters(&fp,&source,w,h,Some(&map)).unwrap();
                let mut expected=source.clone();
                for _ in 0..iterations {
                    let bank:Vec<Vec<T>>=(1..=4).map(|radius| {
                        let mut output=vec![T::default();source.len()];
                        filters::dispatch_filter(mode,&expected,&mut output,w,h,radius,50.0,None);
                        output
                    }).collect();
                    for i in 0..w*h {
                        let radius=radii[(i%w+3*(i/w))%radii.len()];
                        expected[i*4..i*4+4].copy_from_slice(&bank[radius-1][i*4..i*4+4]);
                    }
                }
                assert_eq!(actual,expected,"mode={mode}, iterations={iterations}, {w}x{h}");
            }
        }
    }
    for (w,h) in [(1,31),(17,1),(17,13)] {
        let deep:Vec<u16>=(0..w*h*4).map(|i|if i%4==0 {32768} else {((i*137+16385)%32769) as u16}).collect();
        let hdr:Vec<f32>=(0..w*h*4).map(|i|if i%4==0 {0.75} else {(i%29) as f32*0.125-0.375}).collect();
        check(deep,w,h);
        check(hdr,w,h);
    }
}

fn process_filters<T: filters::Channel>(
    fp: &FilterParams,
    original: &[T],
    w: usize,
    h: usize,
    luma_map: Option<&[f64]>,
) -> Result<Vec<T>,ae::Error> {
    let count=w.checked_mul(h).and_then(|n|n.checked_mul(4)).ok_or(ae::Error::BadCallbackParameter)?;
    if original.len()!=count || w>i32::MAX as usize-20 || h>i32::MAX as usize-20
        || !(1..=20).contains(&fp.radius) || !(1..=10).contains(&fp.iterations)
        || luma_map.is_some_and(|map|map.len()!=count/4 || map.iter().any(|v|!v.is_finite() || !(0.0..=1.0).contains(v))) {
        return Err(ae::Error::BadCallbackParameter);
    }
    fn plane<T:filters::Channel>(n:usize)->Result<Vec<T>,ae::Error> {
        let mut v=Vec::new();v.try_reserve_exact(n).map_err(|_|ae::Error::OutOfMemory)?;
        v.resize(n,T::default());Ok(v)
    }
    let mut buf_a=plane(count)?;
    buf_a.copy_from_slice(original);
    let mut buf_b=plane(count)?;

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

    filters::mix_buffers(original, &buf_a, &mut buf_b, fp.mix);
    Ok(buf_b)
}

#[test]
fn filter_workspace_validates_and_preserves_iteration_mix() {
    let mut p=FilterParams{filter_type:1,radius:2,edge_preserve:50.0,iterations:2,mix:0.5,invert_map:false};
    assert!(process_filters(&p,&[0u8;3],1,1,None).is_err());
    assert!(process_filters::<u8>(&p,&[],usize::MAX,2,None).is_err());
    for map in [&[][..],&[f64::NAN][..],&[2.0][..]] {assert!(process_filters(&p,&[0u8;4],1,1,Some(map)).is_err());}
    let original:Vec<u16>=(0..7*5*4).map(|i|((i*233)%32769) as u16).collect();
    for mode in 1..=3 {for iterations in [1,2,3] {
        p.filter_type=mode;p.iterations=iterations;
        let mut a=original.clone();let mut b=vec![0;original.len()];
        for _ in 0..iterations {filters::dispatch_filter(mode,&a,&mut b,7,5,2,50.0,None);std::mem::swap(&mut a,&mut b);}
        let mut expected=vec![0;original.len()];
        filters::mix_buffers(&original,&a,&mut expected,0.5);
        assert_eq!(process_filters(&p,&original,7,5,None).unwrap(),expected);
    }}
}

// ---- Legacy Render path ----

fn render_cpu(
    params: &ae::Parameters<Params>,
    in_data: &ae::InData,
    in_layer: &ae::Layer,
    out_layer: &mut ae::Layer,
) -> Result<(), ae::Error> {
    match pixels::channel_bytes(in_layer)? {
        1 => render_typed::<u8>(params, in_data, in_layer, out_layer),
        2 => render_typed::<u16>(params, in_data, in_layer, out_layer),
        4 => render_typed::<f32>(params, in_data, in_layer, out_layer),
        _ => Err(ae::Error::BadCallbackParameter),
    }
}

fn render_typed<T: pixels::Channel + filters::Channel>(
    params: &ae::Parameters<Params>, in_data: &ae::InData,
    in_layer: &ae::Layer, out_layer: &mut ae::Layer,
) -> Result<(), ae::Error> {
    let fp = get_params(params)?;
    let (original, w, h) = pixels::read::<T>(in_layer)?;

    let luma_map = get_map_layer_luma(params, in_data, w, h, fp.invert_map)?;

    let result = process_filters(&fp, &original, w, h, luma_map.as_deref())?;
    pixels::write(&result, out_layer, w, h)
}

/// Read the map layer via legacy checkout and build a luminance map.
fn get_map_layer_luma(
    params: &ae::Parameters<Params>,
    in_data: &ae::InData,
    w: usize,
    h: usize,
    invert: bool,
) -> Result<Option<Vec<f64>>,ae::Error> {
    let checkout = params.checkout_at(Params::MapLayer, Some(in_data.current_time()), None, None)?;
    let layer_def = checkout.as_layer()?;
    convert_optional_map(Ok(layer_def.value()),|layer|map_layer_luma(&layer,w,h,invert))
}

fn convert_optional_map<T,U,E>(value:Result<Option<T>,E>,convert:impl FnOnce(T)->Result<U,E>)->Result<Option<U>,E> {
    value?.map(convert).transpose()
}

#[test]
fn map_conversion_errors_are_not_missing_maps() {
    assert_eq!(convert_optional_map(Ok(None::<()>), |_|Err::<(),_>("conversion")),Ok(None));
    assert_eq!(convert_optional_map(Ok(Some(())), |_|Err::<(),_>("conversion")),Err("conversion"));
    assert_eq!(convert_optional_map(Ok(Some(7)), Ok::<_,&str>),Ok(Some(7)));
    assert_eq!(convert_optional_map(Err::<Option<()>,_>("checkout"), |_| -> Result<(),&str> {panic!("must not decode failed checkout")}),Err("checkout"));
    assert!(build_luminance_map(&[0u8;3],1,1,false).is_err());
    assert!(build_luminance_map::<u8>(&[],usize::MAX,2,false).is_err());
}

fn map_layer_luma(layer: &ae::Layer, w: usize, h: usize, invert: bool) -> Result<Vec<f64>, ae::Error> {
    fn typed<T: pixels::Channel>(layer: &ae::Layer, w: usize, h: usize, invert: bool) -> Result<Vec<f64>, ae::Error> {
        let (flat, mw, mh) = pixels::read::<T>(layer)?;
        if mw==0 || mh==0 {return Err(ae::Error::BadCallbackParameter);}
        if (mw,mh)==(w,h) {build_luminance_map(&flat,w,h,invert)}
        else {build_luminance_map_resample(&flat,mw,mh,w,h,invert)}
    }
    match pixels::channel_bytes(layer)? {
        1 => typed::<u8>(layer,w,h,invert),
        2 => typed::<u16>(layer,w,h,invert),
        4 => typed::<f32>(layer,w,h,invert),
        _ => Err(ae::Error::BadCallbackParameter),
    }
}

/// Resample a luminance map from (mw,mh) to (w,h) using nearest-neighbor.
fn build_luminance_map_resample<T: pixels::Channel>(
    buf: &[T],
    mw: usize,
    mh: usize,
    w: usize,
    h: usize,
    invert: bool,
) -> Result<Vec<f64>,ae::Error> {
    let count=w.checked_mul(h).filter(|&n|n<=isize::MAX as usize/8).ok_or(ae::Error::BadCallbackParameter)?;
    if mw==0 || mh==0 || mw.checked_mul(mh).and_then(|n|n.checked_mul(4))!=Some(buf.len()) {
        return Err(ae::Error::BadCallbackParameter);
    }
    let mut map=Vec::new();
    map.try_reserve_exact(count).map_err(|_|ae::Error::OutOfMemory)?;
    map.resize(count,0.0);
    if count==0 {return Ok(map);}
    let src_map = build_luminance_map(buf, mw, mh, invert)?;
    for y in 0..h {
        let sy = (y as u128 * mh as u128 / h as u128) as usize;
        for x in 0..w {
            let sx = (x as u128 * mw as u128 / w as u128) as usize;
            map[y * w + x] = src_map[sy * mw + sx];
        }
    }
    Ok(map)
}

#[test]
fn map_resampling_rejects_empty_and_preserves_nearest_pixels() {
    assert!(build_luminance_map_resample::<u8>(&[],0,0,1,1,false).is_err());
    assert!(build_luminance_map_resample(&[0u8;3],1,1,1,1,false).is_err());
    assert!(build_luminance_map_resample(&[0u8;4],1,1,usize::MAX,2,false).is_err());
    let src=[255u8,0,0,0,255,255,255,255];
    for invert in [false,true] {
        let result=build_luminance_map_resample(&src,2,1,5,3,invert).unwrap();
        for y in 0..3 {for x in 0..5 {
            assert!((result[y*5+x]-if (x>=3)^invert {1.0} else {0.0}).abs()<1e-12);
        }}
    }
    assert!(build_luminance_map_resample(&src,2,1,0,usize::MAX,false).unwrap().is_empty());
}

// ---- SmartFX PreRender ----

const MAP_CHECKOUT_ID: i32 = 1;

fn smart_pre_render(
    in_data: &ae::InData,
    params: &ae::Parameters<Params>,
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

    // An unassigned optional layer is not a failed assigned-layer checkout.
    let map_param = params.checkout_at(Params::MapLayer, Some(in_data.current_time()), None, None)?;
    let has_map = map_param.as_layer()?.value().is_some();
    if has_map {
        cb.checkout_layer(
        6,
        MAP_CHECKOUT_ID,
        &req,
        in_data.current_time(),
        in_data.time_step(),
        in_data.time_scale(),
        )?;
    }
    extra.set_pre_render_data(has_map);

    Ok(())
}

// ---- SmartFX CPU Render ----

fn with_checkin<T,E>(work:impl FnOnce()->Result<T,E>,checkin:impl FnOnce()->Result<(),E>)->Result<T,E> {
    let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(work));
    let cleanup=checkin();
    match result {
        Ok(Ok(value))=>cleanup.map(|()|value),
        Ok(Err(err))=>Err(err),
        Err(payload)=>std::panic::resume_unwind(payload),
    }
}

#[test]
fn acquired_worlds_are_returned_once_in_nested_failure_paths() {
    use std::cell::RefCell;
    for has_map in [false,true] {for mode in 0..3 {for cleanup_fails in [false,true] {
        let order=RefCell::new(Vec::new());
        let cleanup=|name| {order.borrow_mut().push(name); if cleanup_fails {Err("checkin")}else{Ok(())}};
        let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||with_checkin(|| {
            let work=||match mode {0=>Ok(7),1=>Err("work"),_=>panic!("injected")};
            if has_map {with_checkin(work,||cleanup("map"))}else{work()}
        },||cleanup("input"))));
        assert_eq!(*order.borrow(),if has_map {vec!["map","input"]}else{vec!["input"]});
        match mode {
            0=>assert_eq!(result.unwrap(),if cleanup_fails {Err("checkin")}else{Ok(7)}),
            1=>assert_eq!(result.unwrap(),Err("work")),
            _=>assert!(result.is_err()),
        }
    }}}
}

fn smart_render_cpu(
    extra: &ae::pf::SmartRenderExtra,
    params: &ae::Parameters<Params>,
) -> Result<(), ae::Error> {
    let fp = get_params(params)?;
    let cb = extra.callbacks();

    let input_world = cb.checkout_layer_pixels(0)?.ok_or(ae::Error::Generic)?;
    with_checkin(|| -> Result<(), ae::Error> {
        let mut output_world = cb.checkout_output()?.ok_or(ae::Error::Generic)?;
        let (w,h) = (input_world.width(), input_world.height());
        let map_world = if extra.pre_render_data::<bool>().copied().unwrap_or(false) {
            cb.checkout_layer_pixels(MAP_CHECKOUT_ID as u32)
        } else { Ok(None) };
        let luma_map = convert_optional_map(map_world,
            |map_world|with_checkin(||map_layer_luma(&map_world,w,h,fp.invert_map),
                ||cb.checkin_layer_pixels(MAP_CHECKOUT_ID as u32)))?;
        fn typed<T: pixels::Channel + filters::Channel>(
            input: &ae::Layer, output: &mut ae::Layer, fp: &FilterParams, map: Option<&[f64]>
        ) -> Result<(), ae::Error> {
            let (src,w,h) = pixels::read::<T>(input)?;
            let result = process_filters(fp,&src,w,h,map)?;
            pixels::write(&result,output,w,h)
        }
        match pixels::channel_bytes(&input_world)? {
            1 => typed::<u8>(&input_world,&mut output_world,&fp,luma_map.as_deref()),
            2 => typed::<u16>(&input_world,&mut output_world,&fp,luma_map.as_deref()),
            4 => typed::<f32>(&input_world,&mut output_world,&fp,luma_map.as_deref()),
            _ => Err(ae::Error::BadCallbackParameter),
        }
    },||cb.checkin_layer_pixels(0))
}
