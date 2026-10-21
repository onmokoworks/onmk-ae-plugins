use after_effects as ae;

mod scatter;
mod pixels;
use pixels::Channel;

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

fn process_worlds(sp: &ScatterParams, input: &ae::Layer, output: &mut ae::Layer, map: Option<&[f64]>) -> Result<(), ae::Error> {
    fn typed<T: Channel>(sp: &ScatterParams, input: &ae::Layer, output: &mut ae::Layer, map: Option<&[f64]>) -> Result<(), ae::Error> {
        let (src, w, h) = pixels::read::<T>(input)?;
        let result = scatter::scatter(sp, &src, w, h, map)?;
        pixels::write(&result, output, w, h)
    }
    match pixels::channel_bytes(input)? {
        1 => typed::<u8>(sp, input, output, map),
        2 => typed::<u16>(sp, input, output, map),
        4 => typed::<f32>(sp, input, output, map),
        _ => Err(ae::Error::BadCallbackParameter),
    }
}

// ---- Luminance map ----

fn map_buffer(w:usize,h:usize)->Result<Vec<f64>,ae::Error> {
    let n=w.checked_mul(h).ok_or(ae::Error::OutOfMemory)?;
    let mut map=Vec::new();
    map.try_reserve_exact(n).map_err(|_|ae::Error::OutOfMemory)?;
    map.resize(n,0.0);
    Ok(map)
}

fn build_luminance_map<T: Channel>(buf: &[T], w: usize, h: usize, invert: bool) -> Result<Vec<f64>,ae::Error> {
    let n=w.checked_mul(h).and_then(|v|v.checked_mul(4)).ok_or(ae::Error::OutOfMemory)?;
    if buf.len()!=n {return Err(ae::Error::BadCallbackParameter);}
    let mut map = map_buffer(w,h)?;
    for i in 0..map.len() {
        let off = i * 4;
        let r = buf[off + 1].normalized();
        let g = buf[off + 2].normalized();
        let b = buf[off + 3].normalized();
        let luma = (0.2126 * r + 0.7152 * g + 0.0722 * b).clamp(0.0, 1.0);
        map[i] = if invert { 1.0 - luma } else { luma };
    }
    Ok(map)
}

fn build_luminance_map_resample<T: Channel>(
    buf: &[T], mw: usize, mh: usize, w: usize, h: usize, invert: bool,
) -> Result<Vec<f64>,ae::Error> {
    if w==0 || h==0 {return Ok(Vec::new());}
    if mw==0 || mh==0 {return Err(ae::Error::BadCallbackParameter);}
    let src_map = build_luminance_map(buf, mw, mh, invert)?;
    if (mw,mh)==(w,h) {return Ok(src_map);}
    let mut map = map_buffer(w,h)?;
    for y in 0..h {
        let sy = ((y as u128 * mh as u128) / h as u128) as usize;
        for x in 0..w {
            let sx = ((x as u128 * mw as u128) / w as u128) as usize;
            map[y * w + x] = src_map[sy * mw + sx];
        }
    }
    Ok(map)
}

fn map_from_world(layer: &ae::Layer, w: usize, h: usize, invert: bool) -> Result<Vec<f64>, ae::Error> {
    fn typed<T: Channel>(layer: &ae::Layer, w: usize, h: usize, invert: bool) -> Result<Vec<f64>, ae::Error> {
        let (flat, mw, mh) = pixels::read::<T>(layer)?;
        build_luminance_map_resample(&flat, mw, mh, w, h, invert)
    }
    match pixels::channel_bytes(layer)? {
        1 => typed::<u8>(layer, w, h, invert),
        2 => typed::<u16>(layer, w, h, invert),
        4 => typed::<f32>(layer, w, h, invert),
        _ => Err(ae::Error::BadCallbackParameter),
    }
}

// ---- Legacy Render ----

fn render_cpu(
    params: &ae::Parameters<Params>,
    in_data: &ae::InData,
    in_layer: &ae::Layer,
    out_layer: &mut ae::Layer,
) -> Result<(), ae::Error> {
    let sp = get_params(params)?;
    let (w, h) = (in_layer.width(), in_layer.height());

    let luma_map = get_map_layer_luma(params, in_data, w, h, sp.invert_map);
    process_worlds(&sp, in_layer, out_layer, luma_map.as_deref())
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
    map_from_world(&layer, w, h, invert).ok()
}

// ---- SmartFX ----

const MAP_CHECKOUT_ID: i32 = 1;

#[cfg(test)]
mod map_tests {
    use super::*;
    #[test]
    fn map_geometry_and_nearest_resampling_are_checked() {
        assert!(build_luminance_map_resample::<u8>(&[],0,1,2,3,false).is_err());
        assert!(build_luminance_map::<u8>(&[0;3],1,1,false).is_err());
        assert!(build_luminance_map::<u8>(&[],usize::MAX,2,false).is_err());
        assert!(map_buffer(usize::MAX,2).is_err());
        assert!(map_buffer(usize::MAX,1).is_err());
        assert_eq!(build_luminance_map_resample::<u8>(&[],0,0,0,3,false).unwrap(),Vec::<f64>::new());
        let src=[255u8,0,0,0,255,255,255,255];
        let original=build_luminance_map(&src,2,1,false).unwrap();
        assert_eq!(build_luminance_map_resample(&src,2,1,2,1,false).unwrap(),original);
        let resized=build_luminance_map_resample(&src,2,1,3,2,false).unwrap();
        assert_eq!(resized,[original[0],original[0],original[1],original[0],original[0],original[1]]);
        let inv=build_luminance_map_resample(&src,2,1,3,2,true).unwrap();
        for (a,b) in resized.iter().zip(inv) {assert!((a+b-1.0).abs()<1e-12);}
    }
}

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
    let mut map_checked_out = false;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<(), ae::Error> {
    let mut output_world = cb.checkout_output()?.ok_or(ae::Error::Generic)?;
    let (w, h) = (input_world.width(), input_world.height());

    let invert = sp.invert_map;
    let luma_map = if let Ok(Some(map_world)) = cb.checkout_layer_pixels(MAP_CHECKOUT_ID as u32) {
        map_checked_out = true;
        Some(map_from_world(&map_world, w, h, invert)?)
    } else {
        None
    };

    process_worlds(&sp, &input_world, &mut output_world, luma_map.as_deref())
    })).unwrap_or(Err(ae::Error::InternalStructDamaged));
    let input_checkin = cb.checkin_layer_pixels(0);
    let map_checkin = if map_checked_out { cb.checkin_layer_pixels(MAP_CHECKOUT_ID as u32) } else { Ok(()) };
    result.and(input_checkin).and(map_checkin)
}
