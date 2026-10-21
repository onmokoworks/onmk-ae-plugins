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
                f.set_valid_max(1000);
                f.set_slider_min(2);
                f.set_slider_max(240);
                f.set_default(30);
            }),
        )?;

        params.add(
            Params::FrameStep,
            "Frame Step",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(1);
                f.set_valid_max(100);
                f.set_slider_min(1);
                f.set_slider_max(30);
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
                let (src, w, h) = layer_to_flat(&in_layer)?;
                let sp = get_params(params, &in_data)?;
                let result = classic_sequence(&sp, &src, w, h, in_data.current_time(),
                    in_data.time_step(), |index, time| {
                        classic_checkout(&in_data, index, time, &in_layer)
                    })?;
                flat_to_layer(&result, &mut out_layer, w, h)?;
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

fn checked_layout(w:usize,h:usize,row_bytes:isize)->Result<usize,ae::Error> {
    let stride=row_bytes.checked_abs().ok_or(ae::Error::BadCallbackParameter)? as usize;
    let extent=stride.checked_mul(h).ok_or(ae::Error::BadCallbackParameter)?;
    if w>i32::MAX as usize || h>i32::MAX as usize || row_bytes==i32::MIN as isize
        || extent>isize::MAX as usize || (w!=0 && h!=0 && stride<w*4) {
        return Err(ae::Error::BadCallbackParameter);
    }
    Ok(stride)
}

fn world_layout(layer:&ae::Layer)->Result<usize,ae::Error> {
    let stride=checked_layout(layer.width(),layer.height(),layer.row_bytes())?;
    if layer.pixel_format()? != ae::PixelFormat::Argb32
        || (layer.width()!=0 && layer.height()!=0 && unsafe{layer.data_ptr()}.is_null()) {
        return Err(ae::Error::BadCallbackParameter);
    }
    Ok(stride)
}

#[test]
fn world_geometry_rejects_short_rows_and_overflow() {
    for stride in [68,75,-68,-75] {assert_eq!(checked_layout(17,31,stride),Ok(stride.unsigned_abs()));}
    for (w,h,s) in [(17,31,67),(1,2,isize::MAX),(1,1,isize::MIN),
        (1,1,i32::MIN as isize),(usize::MAX,1,4)] {
        assert_eq!(checked_layout(w,h,s),Err(ae::Error::BadCallbackParameter));
    }
}

fn layer_to_flat(layer: &ae::Layer) -> Result<(Vec<u8>, usize, usize),ae::Error> {
    let w = layer.width() as usize;
    let h = layer.height() as usize;
    let stride = world_layout(layer)?;
    if w==0 || h==0 { return Ok((Vec::new(),w,h)); }
    let buf = layer.buffer();
    let count=w.checked_mul(h).and_then(|n|n.checked_mul(4)).ok_or(ae::Error::BadCallbackParameter)?;
    let mut flat=Vec::new();
    flat.try_reserve_exact(count).map_err(|_|ae::Error::OutOfMemory)?;
    flat.resize(count,0u8);
    for y in 0..h {
        let src_off = storage_row(y,h,layer.row_bytes()<0) * stride;
        let dst_off = y * w * 4;
        let row_len = w * 4;
        if src_off + row_len <= buf.len() && dst_off + row_len <= flat.len() {
            flat[dst_off..dst_off + row_len].copy_from_slice(&buf[src_off..src_off + row_len]);
        }
    }
    Ok((flat, w, h))
}

fn flat_to_layer(flat: &[u8], layer: &mut ae::Layer, w: usize, h: usize) -> Result<(),ae::Error> {
    let stride = world_layout(layer)?;
    if (w,h)!=(layer.width(),layer.height()) || w.checked_mul(h).and_then(|n|n.checked_mul(4))!=Some(flat.len()) {
        return Err(ae::Error::BadCallbackParameter);
    }
    if w==0 || h==0 {return Ok(());}
    let bottom_up = layer.row_bytes()<0;
    let buf = layer.buffer_mut();
    for y in 0..h {
        let src_off = y * w * 4;
        let dst_off = storage_row(y,h,bottom_up) * stride;
        let row_len = w * 4;
        if src_off + row_len <= flat.len() && dst_off + row_len <= buf.len() {
            buf[dst_off..dst_off + row_len].copy_from_slice(&flat[src_off..src_off + row_len]);
        }
    }
    Ok(())
}

fn storage_row(y:usize,h:usize,bottom_up:bool)->usize {
    if bottom_up { h-1-y } else { y }
}

#[test]
fn signed_rows_preserve_distinct_scanlines_and_padding() {
    for h in [1,2,31] {
        for source_bottom_up in [false,true] {
            for destination_bottom_up in [false,true] {
                let row_bytes=12;
                let stride=19;
                let mut source=vec![203u8;stride*h];
                for physical in 0..h {
                    let logical=if source_bottom_up {h-1-physical} else {physical};
                    source[physical*stride..physical*stride+row_bytes].fill(logical as u8);
                }
                let mut flat=vec![0;row_bytes*h];
                for y in 0..h {
                    let offset=storage_row(y,h,source_bottom_up)*stride;
                    flat[y*row_bytes..(y+1)*row_bytes].copy_from_slice(&source[offset..offset+row_bytes]);
                }
                let mut dest=vec![211u8;stride*h];
                for y in 0..h {
                    let offset=storage_row(y,h,destination_bottom_up)*stride;
                    dest[offset..offset+row_bytes].copy_from_slice(&flat[y*row_bytes..(y+1)*row_bytes]);
                }
                for physical in 0..h {
                    let logical=if destination_bottom_up {h-1-physical} else {physical};
                    assert!(dest[physical*stride..physical*stride+row_bytes].iter().all(|&v|v==logical as u8));
                    assert!(dest[physical*stride+row_bytes..(physical+1)*stride].iter().all(|&v|v==211));
                }
            }
        }
    }
}

// ---- Luminance map ----

/// Place a temporal checkout in the current input's coordinate system.
/// Cropped/empty regions are transparent, never stretched to fill the frame.
fn rgba_len(w:usize,h:usize)->Result<usize,ae::Error> {
    w.checked_mul(h).and_then(|n|n.checked_mul(4)).filter(|&n|n<=isize::MAX as usize)
        .ok_or(ae::Error::BadCallbackParameter)
}

fn byte_plane(count:usize)->Result<Vec<u8>,ae::Error> {
    let mut out=Vec::new();
    out.try_reserve_exact(count).map_err(|_|ae::Error::OutOfMemory)?;
    out.resize(count,0);
    Ok(out)
}

fn copy_frame(src:&[u8])->Result<Vec<u8>,ae::Error> {
    let mut out=byte_plane(src.len())?;
    out.copy_from_slice(src);
    Ok(out)
}

fn align_temporal_frame(src: &[u8], sw: usize, sh: usize, w: usize, h: usize, dx: i64, dy: i64) -> Result<Vec<u8>,ae::Error> {
    if rgba_len(sw,sh)?!=src.len() {return Err(ae::Error::BadCallbackParameter);}
    let mut out=byte_plane(rgba_len(w,h)?)?;
    if sw==0 || sh==0 || w==0 || h==0 {return Ok(out);}
    let (dx,dy)=(i128::from(dx),i128::from(dy));
    for sy in 0..sh {
        let y=sy as i128+dy;
        if y<0 || y>=h as i128 { continue; }
        let start=(-dx).max(0).min(sw as i128) as usize;
        let end=(w as i128-dx).max(0).min(sw as i128) as usize;
        if start>=end { continue; }
        let from=(sy*sw+start)*4;
        let to=(y as usize*w+(start as i128+dx) as usize)*4;
        let len=(end-start)*4;
        out[to..to+len].copy_from_slice(&src[from..from+len]);
    }
    Ok(out)
}

#[test]
fn temporal_tiles_preserve_position_and_transparent_exterior() {
    let tile=[255u8,10,20,30,255,40,50,60];
    let aligned=align_temporal_frame(&tile,2,1,3,2,1,1).unwrap();
    assert_eq!(&aligned[16..24],&tile);
    assert!(aligned[..16].iter().all(|&v|v==0));
    assert_eq!(align_temporal_frame(&tile,2,1,1,1,-1,0).unwrap(),tile[4..]);
    assert_eq!(align_temporal_frame(&[],0,0,3,2,0,0).unwrap(),vec![0;24]);
    assert_eq!(align_temporal_frame(&tile,2,1,3,2,20,-20).unwrap(),vec![0;24]);
}

#[test]
fn temporal_workspace_rejects_bad_shape_and_capacity() {
    assert_eq!(byte_plane(usize::MAX),Err(ae::Error::OutOfMemory));
    assert_eq!(align_temporal_frame(&[1;3],1,1,1,1,0,0),Err(ae::Error::BadCallbackParameter));
    assert_eq!(align_temporal_frame(&[],0,0,usize::MAX,2,0,0),Err(ae::Error::BadCallbackParameter));
    for dx in [i64::MIN,i64::MAX] {for dy in [0,i64::MIN,i64::MAX] {
        assert_eq!(align_temporal_frame(&[1;4],1,1,1,1,dx,dy).unwrap(),[0;4]);
    }}
    assert_eq!(copy_frame(&[0,17,254,255]).unwrap(),[0,17,254,255]);
    assert_eq!(align_temporal_frame(&[],0,usize::MAX,1,1,0,0).unwrap(),[0;4]);
}

fn map_plane(w:usize,h:usize)->Result<Vec<f64>,ae::Error> {
    let count=w.checked_mul(h).filter(|&n|n<=isize::MAX as usize/8)
        .ok_or(ae::Error::BadCallbackParameter)?;
    let mut map=Vec::new();
    map.try_reserve_exact(count).map_err(|_|ae::Error::OutOfMemory)?;
    map.resize(count,0.0);
    Ok(map)
}

fn build_luminance_map(buf: &[u8], w: usize, h: usize, invert: bool) -> Result<Vec<f64>,ae::Error> {
    if rgba_len(w,h)?!=buf.len() {return Err(ae::Error::BadCallbackParameter);}
    let mut map = map_plane(w,h)?;
    for i in 0..(w * h) {
        let off = i * 4;
        let r = buf[off + 1] as f64;
        let g = buf[off + 2] as f64;
        let b = buf[off + 3] as f64;
        let luma = (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255.0;
        map[i] = if invert { 1.0 - luma } else { luma };
    }
    Ok(map)
}

fn build_luminance_map_resample(
    buf: &[u8],
    mw: usize,
    mh: usize,
    w: usize,
    h: usize,
    invert: bool,
) -> Result<Vec<f64>,ae::Error> {
    if mw==0 || mh==0 {return Err(ae::Error::BadCallbackParameter);}
    let src_map = build_luminance_map(buf, mw, mh, invert)?;
    let mut map = map_plane(w,h)?;
    if map.is_empty() {return Ok(map);}
    for y in 0..h {
        let sy = (y as u128 * mh as u128 / h as u128) as usize;
        for x in 0..w {
            let sx = (x as u128 * mw as u128 / w as u128) as usize;
            map[y * w + x] = src_map[sy * mw + sx];
        }
    }
    Ok(map)
}

// ---- SmartFX ----

const MAP_CHECKOUT_ID: i32 = 1;
const FRAME_BASE_ID: i32 = 10;
const MAP_PARAM_INDEX: i32 = 9;

fn has_pixels(rect: ae::Rect) -> bool {
    rect.right > rect.left && rect.bottom > rect.top
}

fn map_or_horizontal(buf: Option<(&[u8],usize,usize)>, w:usize,h:usize,invert:bool) -> Result<Vec<f64>,ae::Error> {
    match buf {
        Some((pixels,mw,mh)) if mw>0 && mh>0 => {
            if (mw,mh)==(w,h) { build_luminance_map(pixels,w,h,invert) }
            else { build_luminance_map_resample(pixels,mw,mh,w,h,invert) }
        }
        _ => slit_scan::generate_gradient_map(MODE_HORIZONTAL,w,h,0.0,(0.5,0.5),invert),
    }
}

#[test]
fn missing_map_and_empty_map_use_horizontal_time_gradient() {
    assert!(!has_pixels(ae::Rect{left:0,top:0,right:0,bottom:0}));
    assert!(has_pixels(ae::Rect{left:-2,top:0,right:2,bottom:1}));
    for invert in [false,true] {
        let expected=slit_scan::generate_gradient_map(MODE_HORIZONTAL,3,1,0.0,(0.5,0.5),invert);
        assert_eq!(map_or_horizontal(None,3,1,invert),expected);
        assert_eq!(map_or_horizontal(Some((&[],0,0)),3,1,invert),expected);
    }
    assert_eq!(map_or_horizontal(Some((&[255,0,0,0],1,1)),3,1,false).unwrap(),vec![0.0;3]);
}

#[test]
fn map_workspace_checks_shape_and_preserves_resampling() {
    assert!(map_plane(usize::MAX,2).is_err());
    assert!(map_plane(isize::MAX as usize/8+1,1).is_err());
    assert!(build_luminance_map(&[1;3],1,1,false).is_err());
    assert!(build_luminance_map_resample(&[],0,0,1,1,false).is_err());
    assert!(slit_scan::generate_gradient_map(1,0,usize::MAX,0.0,(0.5,0.5),false).unwrap().is_empty());
    let pixels=[255,0,0,0,255,255,255,255];
    for invert in [false,true] {
        let actual=build_luminance_map_resample(&pixels,2,1,5,3,invert).unwrap();
        for y in 0..3 {for x in 0..5 {
            let expected=if (x>=3)^invert {1.0} else {0.0};
            assert!((actual[y*5+x]-expected).abs()<1e-12);
        }}
    }
}

fn compute_frame_time(
    current_time: i32,
    time_step: i32,
    time_frames: usize,
    actual_frames: usize,
    direction: i32,
    frame_index: usize,
) -> i32 {
    let denominator=actual_frames.saturating_sub(1).max(1) as u128;
    let index=if actual_frames<=1 {0} else {frame_index.min(actual_frames-1) as u128};
    let range=time_frames.saturating_sub(1) as u128;
    // Quotient/remainder rounding avoids both floating half-tick drift and
    // doubling a potentially wide product. Every rounded offset is <= range.
    let rounded=|position:u128| {
        let product=position*range;
        (product/denominator + u128::from(product%denominator >= denominator.div_ceil(2))) as i128
    };
    let offset=match direction {
        2=>rounded(index),
        3=>rounded(index)-range.div_ceil(2) as i128,
        _=>-rounded(denominator-index),
    };
    // AE uses signed 32-bit time ticks. Widen before arithmetic so long
    // timelines cannot wrap a future sample into an unrelated past frame.
    (i128::from(current_time)+offset*i128::from(time_step))
        .clamp(i128::from(i32::MIN),i128::from(i32::MAX)) as i32
}

#[test]
fn temporal_offsets_do_not_wrap_at_host_time_limits() {
    assert_eq!(compute_frame_time(i32::MAX - 10, 100, 5, 5, 2, 4), i32::MAX);
    assert_eq!(compute_frame_time(i32::MIN + 10, 100, 5, 5, 1, 0), i32::MIN);
}

#[test]
fn temporal_direction_and_endpoints() {
    for (direction, expected) in [(1,[60,80,100]),(2,[100,120,140]),(3,[80,100,120])] {
        let actual: Vec<i32> = (0..3).map(|i|compute_frame_time(100,10,5,3,direction,i)).collect();
        assert_eq!(actual,expected);
    }
}

#[test]
fn temporal_ticks_match_integer_rounding_reference() {
    for range_frames in [1usize,2,5,64,3601] {
        for count in [1usize,2,7] {
            for direction in [1,2,3] {
                for current in [0,100,i32::MIN+10,i32::MAX-10] {
                    for step in [1,100] {
                        for index in 0..count {
                            let denominator=if count>1 {(count-1) as i128} else {1};
                            let numerator=if count>1 {index as i128} else {0};
                            let span=(range_frames-1) as i128;
                            let round_ratio=|n:i128| (2*n+denominator)/(2*denominator);
                            let offset=match direction {
                                2=>round_ratio(numerator*span),
                                3=>round_ratio(numerator*span)-(span+1)/2,
                                _=>-round_ratio((denominator-numerator)*span),
                            };
                            let expected=(current as i128+offset*step as i128)
                                .clamp(i32::MIN as i128,i32::MAX as i128) as i32;
                            assert_eq!(compute_frame_time(current,step,range_frames,count,direction,index),expected,
                                "range={range_frames} count={count} direction={direction} current={current} step={step} index={index}");
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn corrected_checkout_times_select_distinct_frame_pixels_on_cpu_and_gpu() {
    let gpu=gpu::GpuProcessor::new();
    let map:Vec<f64>=(0..7).map(|i|i as f64/6.0).chain([0.75]).collect();
    let pixel=|tick:i32| [255,tick as u8,(tick*2%256) as u8,(tick*3%256) as u8];
    for (direction,ticks) in [
        (1,[37,47,58,68,79,89,100]),
        (2,[100,111,121,132,142,153,163]),
        (3,[68,79,89,100,110,121,131]),
    ] {
        // Synthetic checkout provider returns distinct pixels for requested time.
        // Explicit reference ticks do not reuse compute_frame_time.
        let frames:Vec<Vec<u8>>=(0..7).map(|index| {
            let tick=compute_frame_time(100,1,64,7,direction,index);
            (0..map.len()).flat_map(|_|pixel(tick)).collect()
        }).collect();
        let original:Vec<u8>=(0..map.len()).flat_map(|_|pixel(100)).collect();
        for interpolation in [1,2] {
            let params=SlitScanParams {time_frames:64,frame_step:1,time_direction:direction,
                slice_mode:4,gradient_phase:0.0,center:(0.5,0.5),interpolation,mix:1.0,invert_map:false};
            let mut expected:Vec<u8>=ticks.into_iter().flat_map(pixel).collect();
            if interpolation==1 {expected.extend(pixel(ticks[5]));}
            else {
                let (a,b)=(pixel(ticks[4]),pixel(ticks[5]));
                expected.extend((0..4).map(|c|((u16::from(a[c])+u16::from(b[c])+1)/2) as u8));
            }
            assert_eq!(slit_scan::render(&params,&frames,map.len(),1,&map,&original).unwrap(),expected,
                "CPU direction={direction} interpolation={interpolation}");
            assert_eq!(gpu.process(&params,&frames,map.len(),1,&map,&original).expect("GPU must execute"),expected,
                "GPU direction={direction} interpolation={interpolation}");
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

    let map_has_pixels = cb.checkout_layer(
        MAP_PARAM_INDEX,
        MAP_CHECKOUT_ID,
        &req,
        current_time,
        time_step,
        time_scale,
    ).map(|r|has_pixels(r.result_rect.into())).unwrap_or(false);
    extra.set_pre_render_data(map_has_pixels);

    extra.set_result_rect(res);
    extra.set_max_result_rect(max_res);
    Ok(())
}

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
fn checkout_cleanup_runs_once_on_success_error_and_unwind() {
    use std::cell::Cell;
    for fail_cleanup in [false,true] {
        for mode in 0..3 {
            let calls=Cell::new(0);
            let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| with_checkin(|| {
                match mode {0=>Ok(7),1=>Err("work"),_=>panic!("injected")}
            },|| {calls.set(calls.get()+1); if fail_cleanup {Err("cleanup")} else {Ok(())}})));
            assert_eq!(calls.get(),1);
            match mode {
                0=>assert_eq!(result.unwrap(),if fail_cleanup {Err("cleanup")} else {Ok(7)}),
                1=>assert_eq!(result.unwrap(),Err("work")),
                _=>assert!(result.is_err()),
            }
        }
    }
}

#[test]
fn nested_temporal_cleanup_returns_child_before_input() {
    use std::cell::RefCell;
    for mode in 0..3 {
        let order=RefCell::new(Vec::new());
        let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||
            with_checkin(||with_checkin(||match mode {
                0=>Ok(()),1=>Err("conversion"),_=>panic!("injected nested failure"),
            },||{order.borrow_mut().push("temporal"); Err("child cleanup")}),
            ||{order.borrow_mut().push("input"); Err("input cleanup")})
        ));
        assert_eq!(*order.borrow(),["temporal","input"]);
        match mode {
            0=>assert_eq!(result.unwrap(),Err("child cleanup")),
            1=>assert_eq!(result.unwrap(),Err("conversion")),
            _=>assert!(result.is_err()),
        }
    }
}

// Classic checkout owns the parameter only until its pixels have been copied.
// Use explicit checkin so callback errors do not panic in ParamDef::drop.
fn classic_checkout(in_data: &ae::InData, index: i32, time: i32, input: &ae::Layer)
    -> Result<Option<(Vec<u8>, usize, usize)>, ae::Error>
{
    let cb = in_data.interact();
    let param = cb.checkout_param(index, time, in_data.time_step(), in_data.time_scale())?;
    with_checkin(|| {
        if param.param_type != ae::sys::PF_Param_LAYER {
            return Err(ae::Error::BadCallbackParameter);
        }
        let mut raw = unsafe { param.u.ld };
        if raw.data.is_null() && (raw.width == 0 || raw.height == 0) { return Ok(None); }
        let world = ae::Layer::from_raw(&mut raw, in_data, None);
        let (flat, w, h) = layer_to_flat(&world)?;
        if index != 0 { return Ok(Some((flat, w, h))); }
        let origin = world.origin();
        let current = input.origin();
        let (dx, dy) = (i64::from(origin.h)-i64::from(current.h),
            i64::from(origin.v)-i64::from(current.v));
        let aligned = if w == input.width() && h == input.height() && dx == 0 && dy == 0 { flat }
            else { align_temporal_frame(&flat,w,h,input.width(),input.height(),dx,dy)? };
        Ok(Some((aligned,input.width(),input.height())))
    }, || cb.checkin_param(&param))
}

fn classic_sequence(
    sp: &SlitScanParams, src: &[u8], w: usize, h: usize, time: i32, step: i32,
    mut checkout: impl FnMut(i32, i32) -> Result<Option<(Vec<u8>, usize, usize)>, ae::Error>,
) -> Result<Vec<u8>, ae::Error> {
    if w == 0 || h == 0 { return copy_frame(src); }
    let count = calc_actual_frames(sp.time_frames,sp.frame_step);
    let mut frames = Vec::new();
    frames.try_reserve_exact(count).map_err(|_|ae::Error::OutOfMemory)?;
    for i in 0..count {
        let tick = compute_frame_time(time,step,sp.time_frames,count,sp.time_direction,i);
        // Missing temporal pixels use the current frame, as in Smart Render.
        let frame = match checkout(0,tick)? {
            Some((pixels,fw,fh)) if fw == w && fh == h => pixels,
            Some(_) => return Err(ae::Error::BadCallbackParameter),
            None => copy_frame(src)?,
        };
        frames.push(frame);
    }
    let map = if sp.slice_mode == MODE_MAP_LAYER {
        let layer = checkout(MAP_PARAM_INDEX,time)?;
        map_or_horizontal(layer.as_ref().map(|(p,mw,mh)|(p.as_slice(),*mw,*mh)),w,h,sp.invert_map)?
    } else {
        slit_scan::generate_gradient_map(sp.slice_mode,w,h,sp.gradient_phase,sp.center,sp.invert_map)?
    };
    render_frames(sp,&frames,w,h,&map,src)
}

fn render_frames(sp: &SlitScanParams, frames: &[Vec<u8>], w: usize, h: usize,
    map: &[f64], src: &[u8]) -> Result<Vec<u8>,ae::Error> {
    match get_gpu().and_then(|g|g.process(sp,frames,w,h,map,src)) {
        Some(result) => Ok(result),
        None => slit_scan::render(sp,frames,w,h,map,src),
    }
}

#[test]
fn classic_render_uses_temporal_pixels_and_map_instead_of_passthrough() {
    let mut sp=SlitScanParams {time_frames:3,frame_step:1,time_direction:1,
        slice_mode:MODE_HORIZONTAL,gradient_phase:0.0,center:(0.5,0.5),
        interpolation:1,mix:1.0,invert_map:false};
    let src=vec![255,200,200,200].repeat(3);
    let mut ticks=Vec::new();
    let result=classic_sequence(&sp,&src,3,1,10,1,|index,tick| {
        assert_eq!(index,0); ticks.push(tick);
        Ok(Some((vec![255,(tick*10) as u8,0,0].repeat(3),3,1)))
    }).unwrap();
    assert_eq!(ticks,[8,9,10]);
    assert_eq!([result[1],result[5],result[9]],[80,90,100]);
    assert_ne!(result,src);
    sp.slice_mode=MODE_MAP_LAYER;
    let result=classic_sequence(&sp,&src,3,1,10,1,|index,tick| {
        Ok(Some((if index==MAP_PARAM_INDEX {vec![255,0,0,0].repeat(3)}
            else {vec![255,(tick*10) as u8,0,0].repeat(3)},3,1)))
    }).unwrap();
    assert_eq!([result[1],result[5],result[9]],[80,80,80]);
    assert_eq!(classic_sequence(&sp,&src,3,1,10,1,|_,_|Ok(None)).unwrap(),src);
    assert!(classic_sequence(&sp,&src,3,1,10,1,|_,_|Err(ae::Error::OutOfMemory)).is_err());
}

fn smart_render(
    extra: &ae::pf::SmartRenderExtra,
    params: &ae::Parameters<Params>,
    in_data: &ae::InData,
) -> Result<(), ae::Error> {
    let sp = get_params(params, in_data)?;
    let cb = extra.callbacks();

    let input_world = cb.checkout_layer_pixels(0)?.ok_or(ae::Error::Generic)?;
    with_checkin(|| {
    let mut output_world = cb.checkout_output()?.ok_or(ae::Error::Generic)?;
    let (src, w, h) = layer_to_flat(&input_world)?;

    if w == 0 || h == 0 {
        return flat_to_layer(&src, &mut output_world, w, h);
    }

    let actual_frames = calc_actual_frames(sp.time_frames, sp.frame_step);
    let mut frames: Vec<Vec<u8>> = Vec::new();
    frames.try_reserve_exact(actual_frames).map_err(|_|ae::Error::OutOfMemory)?;
    for i in 0..actual_frames {
        let checkout_id = (FRAME_BASE_ID + i as i32) as u32;
        if let Ok(Some(frame_world)) = cb.checkout_layer_pixels(checkout_id) {
            let aligned=with_checkin(|| {
            let (flat, fw, fh) = layer_to_flat(&frame_world)?;
            let origin=frame_world.origin();
            let input_origin=input_world.origin();
            let dx=i64::from(origin.h)-i64::from(input_origin.h);
            let dy=i64::from(origin.v)-i64::from(input_origin.v);
            Ok(if fw==w && fh==h && dx==0 && dy==0 { flat }
                else { align_temporal_frame(&flat,fw,fh,w,h,dx,dy)? })
            },||cb.checkin_layer_pixels(checkout_id))?;
            frames.push(aligned);
        } else {
            frames.push(copy_frame(&src)?);
        }
    }

    let map = if sp.slice_mode == MODE_MAP_LAYER {
        let map_world = if extra.pre_render_data::<bool>().copied().unwrap_or(false) {
            cb.checkout_layer_pixels(MAP_CHECKOUT_ID as u32).ok().flatten()
        } else { None };
        if let Some(map_world) = map_world {
            let (map_flat, mw, mh) = with_checkin(||layer_to_flat(&map_world),
                ||cb.checkin_layer_pixels(MAP_CHECKOUT_ID as u32))?;
            map_or_horizontal(Some((&map_flat,mw,mh)),w,h,sp.invert_map)?
        } else {
            map_or_horizontal(None,w,h,sp.invert_map)?
        }
    } else {
        slit_scan::generate_gradient_map(
            sp.slice_mode,
            w,
            h,
            sp.gradient_phase,
            sp.center,
            sp.invert_map,
        )?
    };

    let result = render_frames(&sp, &frames, w, h, &map, &src)?;
    flat_to_layer(&result, &mut output_world, w, h)
    },||cb.checkin_layer_pixels(0))
}
