use after_effects as ae;
use ae::sys as ae_sys;

// ---- Parameter IDs ----

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    MaskIndex,
    FlipH,
    FlipV,
    Rotation,
    Apply,
}

// ---- Plugin global state ----

struct Plugin {
    plugin_id: Option<ae::aegp::PluginId>,
}

impl Default for Plugin {
    fn default() -> Self {
        Self { plugin_id: None }
    }
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
            Params::MaskIndex,
            "Mask Index (0=All)",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(128.0);
                f.set_slider_min(0.0);
                f.set_slider_max(16.0);
                f.set_default(0.0);
                f.set_precision(0);
            }),
        )?;

        params.add(
            Params::FlipH,
            "Flip Horizontal",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(false);
                f.set_label("Flip H");
            }),
        )?;

        params.add(
            Params::FlipV,
            "Flip Vertical",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(false);
                f.set_label("Flip V");
            }),
        )?;

        params.add(
            Params::Rotation,
            "Rotation (degrees)",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(-3600.0);
                f.set_valid_max(3600.0);
                f.set_slider_min(-360.0);
                f.set_slider_max(360.0);
                f.set_default(0.0);
                f.set_precision(1);
            }),
        )?;

        params.add_with_flags(
            Params::Apply,
            "Apply Transform",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(false);
                f.set_label("Click to Apply");
            }),
            ae::ParamFlag::SUPERVISE,
            ae::ParamUIFlags::empty(),
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
                    "Mask Transform v1.0\rFlip and rotate mask paths.\rWritten in Rust.",
                );
            }

            ae::Command::GlobalSetup => {
                out_data.set_out_flag(ae::OutFlags::NonParamVary, true);
                out_data.set_out_flag(ae::OutFlags::SendUpdateParamsUi, true);

                let utility = ae::aegp::suites::Utility::new()?;
                self.plugin_id = Some(utility.register_with_aegp("MaskTransform")?);
            }

            ae::Command::UserChangedParam { param_index } => {
                let apply_idx = Params::Apply as usize + 1;
                if param_index == apply_idx {
                    let apply_val = params.get(Params::Apply)?.as_checkbox()?.value();
                    if apply_val {
                        let result=self.apply_transform(params, &in_data);
                        let reset=(|| {
                            let mut apply=params.get_mut(Params::Apply)?;
                            apply.as_checkbox_mut()?.set_value(false);
                            apply.set_value_changed();
                            Ok(())
                        })();
                        result.and(reset)?;
                    }
                }
            }

            ae::Command::Render {
                in_layer,
                mut out_layer,
            } => {
                copy_world(&in_layer, &mut out_layer)?;
            }

            _ => {}
        }
        Ok(())
    }
}

// ---- Raw AEGP suite helpers ----

fn world_layout(world: &ae::Layer) -> Result<(usize,usize,usize,usize,bool),ae::Error> {
    let (w,h)=(world.width(),world.height());
    if w>i32::MAX as usize || h>i32::MAX as usize || world.row_bytes()==i32::MIN as isize {
        return Err(ae::Error::BadCallbackParameter);
    }
    let bpp=match world.pixel_format()? {
        ae::PixelFormat::Argb32=>4, ae::PixelFormat::Argb64=>8, ae::PixelFormat::Argb128=>16,
        _=>return Err(ae::Error::BadCallbackParameter),
    };
    let stride=world.buffer_stride();
    let bytes=stride.checked_mul(h).ok_or(ae::Error::OutOfMemory)?;
    if w!=0 && h!=0 && (stride<w.checked_mul(bpp).ok_or(ae::Error::OutOfMemory)?
        || bytes>isize::MAX as usize || unsafe{world.data_ptr()}.is_null()) {
        return Err(ae::Error::BadCallbackParameter);
    }
    Ok((w,h,bpp,stride,world.row_bytes()<0))
}

fn copy_rows(src:&[u8],dst:&mut[u8],row:usize,h:usize,ss:usize,ds:usize,sr:bool,dr:bool)->Result<(),ae::Error> {
    if row==0 || h==0 {return Ok(());}
    let extent=|stride:usize| (h-1).checked_mul(stride).and_then(|n|n.checked_add(row));
    if ss<row || ds<row || extent(ss).is_none_or(|n|n>src.len()) || extent(ds).is_none_or(|n|n>dst.len()) {
        return Err(ae::Error::BadCallbackParameter);
    }
    for y in 0..h {
        let s=(if sr {h-1-y} else {y})*ss;
        let d=(if dr {h-1-y} else {y})*ds;
        dst[d..d+row].copy_from_slice(&src[s..s+row]);
    }
    Ok(())
}

fn copy_world(src:&ae::Layer,dst:&mut ae::Layer)->Result<(),ae::Error> {
    let (w,h,bpp,ss,sr)=world_layout(src)?;
    let (dw,dh,dbpp,ds,dr)=world_layout(dst)?;
    if (w,h,bpp)!=(dw,dh,dbpp) {return Err(ae::Error::BadCallbackParameter);}
    if w==0 || h==0 {return Ok(());}
    copy_rows(src.buffer(),dst.buffer_mut(),w*bpp,h,ss,ds,sr,dr)
}

#[cfg(test)]
mod copy_tests {
    use super::*;
    #[test]
    fn passthrough_preserves_rows_padding_and_rejects_partial_output() {
        for bpp in [4,8,16] {
            let row=3*bpp;
            let ss=row+5; let ds=row+7;
            let src:Vec<u8>=(0..ss*3).map(|v|v as u8).collect();
            for sr in [false,true] {for dr in [false,true] {
                let mut dst=vec![255;ds*3];
                copy_rows(&src,&mut dst,row,3,ss,ds,sr,dr).unwrap();
                for y in 0..3 {
                    let s=(if sr {2-y} else {y})*ss;
                    let d=(if dr {2-y} else {y})*ds;
                    assert_eq!(&dst[d..d+row],&src[s..s+row]);
                    assert!(dst[d+row..d+ds].iter().all(|v|*v==255));
                }
            }}
            let mut short=vec![99;ds];
            assert!(copy_rows(&src,&mut short,row,3,ss,ds,false,false).is_err());
            assert!(short.iter().all(|v|*v==99));
            assert!(copy_rows(&src,&mut short,row,usize::MAX,ss,ds,false,false).is_err());
            assert!(copy_rows(&src,&mut short,row,1,row-1,ds,false,false).is_err());
        }
    }
}

/// Acquire a raw AEGP suite pointer from the PICA basic suite.
unsafe fn acquire_suite<T>(
    pica: *const ae_sys::SPBasicSuite,
    name: &[u8],
    version: u32,
) -> Result<*const T, ae::Error> {
    if pica.is_null() { return Err(ae::Error::BadCallbackParameter); }
    let mut suite_ptr: *const T = std::ptr::null();
    let acquire_fn = (*pica).AcquireSuite.ok_or(ae::Error::Generic)?;
    let err = acquire_fn(
        name.as_ptr() as *const i8,
        version as i32,
        &mut suite_ptr as *mut *const T as *mut *const std::ffi::c_void,
    );
    if err != 0 || suite_ptr.is_null() {
        return Err(ae::Error::Generic);
    }
    Ok(suite_ptr)
}

unsafe fn release_suite(
    pica: *const ae_sys::SPBasicSuite,
    name: &[u8],
    version: u32,
) -> Result<(),ae::Error> {
    if pica.is_null() { return Err(ae::Error::BadCallbackParameter); }
    let release_fn=(*pica).ReleaseSuite.ok_or(ae::Error::Generic)?;
    if release_fn(name.as_ptr() as *const i8,version as i32)!=0 { return Err(ae::Error::Generic); }
    Ok(())
}

// Covers Result exits; it intentionally makes no panic/unwind guarantee.
fn with_cleanup<T,E>(work:impl FnOnce()->Result<T,E>,cleanup:impl FnOnce()->Result<(),E>) -> Result<T,E> {
    let result=work();
    let released=cleanup();
    match result { Err(error)=>Err(error), Ok(value)=>released.map(|_|value) }
}

// ---- Mask Transform Logic ----

fn with_value_cleanup<V,T,E>(value:&mut V,work:impl FnOnce(&mut V)->Result<T,E>,cleanup:impl FnOnce(&mut V)->Result<(),E>) -> Result<T,E> {
    let result=work(value);
    let released=cleanup(value);
    match result { Err(error)=>Err(error), Ok(output)=>released.map(|_|output) }
}

impl Plugin {
    fn apply_transform(
        &self,
        params: &ae::Parameters<Params>,
        in_data: &ae::InData,
    ) -> Result<(), ae::Error> {
        let plugin_id = self.plugin_id.ok_or(ae::Error::Generic)?;

        let flip_h = params.get(Params::FlipH)?.as_checkbox()?.value();
        let flip_v = params.get(Params::FlipV)?.as_checkbox()?.value();
        let rotation_deg = params.get(Params::Rotation)?.as_float_slider()?.value();
        let mask_index_f = params.get(Params::MaskIndex)?.as_float_slider()?.value();
        let target_mask = mask_index_f as i32;

        if !flip_h && !flip_v && rotation_deg.abs() < 0.001 {
            return Ok(());
        }

        let rotation_rad = rotation_deg.to_radians();
        let cos_r = rotation_rad.cos();
        let sin_r = rotation_rad.sin();

        let pica = in_data.pica_basic_suite_ptr() as *const ae_sys::SPBasicSuite;

        // Get the layer this effect is applied to
        let pf_iface = ae::aegp::suites::PFInterface::new()?;
        let layer = pf_iface.effect_layer(in_data.effect_ref())?;

        let mask_suite = ae::aegp::suites::Mask::new()?;
        let outline_suite = ae::aegp::suites::MaskOutline::new()?;
        let stream_suite = ae::aegp::suites::Stream::new()?;

        // Acquire raw suites for stream value and keyframe manipulation
        // (the Rust wrappers prematurely dispose mask outline handles)
        let raw_stream: *const ae_sys::AEGP_StreamSuite6 = unsafe {
            acquire_suite(pica, ae_sys::kAEGPStreamSuite, ae_sys::kAEGPStreamSuiteVersion6)?
        };
        with_cleanup(|| {
        let raw_kf: *const ae_sys::AEGP_KeyframeSuite5 = unsafe {
            acquire_suite(pica, ae_sys::kAEGPKeyframeSuite, ae_sys::kAEGPKeyframeSuiteVersion5)?
        };
        with_cleanup(|| {
        let utility = ae::aegp::suites::Utility::new()?;
        utility.start_undo_group("Mask Transform")?;
        with_cleanup(|| {
        let num_masks = mask_suite.layer_num_masks(&layer)?;

        self.transform_all_masks(
            plugin_id, &mask_suite, &outline_suite, &stream_suite,
            raw_stream, raw_kf,
            &layer, num_masks, target_mask,
            in_data, flip_h, flip_v, cos_r, sin_r,
        )
        }, ||utility.end_undo_group())
        }, ||unsafe {release_suite(pica,ae_sys::kAEGPKeyframeSuite,ae_sys::kAEGPKeyframeSuiteVersion5)})
        }, ||unsafe {release_suite(pica,ae_sys::kAEGPStreamSuite,ae_sys::kAEGPStreamSuiteVersion6)})
    }

    #[allow(clippy::too_many_arguments)]
    fn transform_all_masks(
        &self,
        plugin_id: ae::aegp::PluginId,
        mask_suite: &ae::aegp::suites::Mask,
        outline_suite: &ae::aegp::suites::MaskOutline,
        stream_suite: &ae::aegp::suites::Stream,
        raw_stream: *const ae_sys::AEGP_StreamSuite6,
        raw_kf: *const ae_sys::AEGP_KeyframeSuite5,
        layer: &ae::aegp::LayerHandle,
        num_masks: i32,
        target_mask: i32,
        in_data: &ae::InData,
        flip_h: bool,
        flip_v: bool,
        cos_r: f64,
        sin_r: f64,
    ) -> Result<(), ae::Error> {
        #[allow(unused_imports)]
        use ae::AsPtr;

        for mi in 0..num_masks {
            if target_mask > 0 && mi != (target_mask - 1) {
                continue;
            }

            let mask = mask_suite.layer_mask_by_index(layer, mi)?;
            let mask_stream = stream_suite.new_mask_stream(
                &mask,
                plugin_id,
                ae::aegp::MaskStream::Outline,
            )?;

            let stream_h = mask_stream.as_ptr();

            // Check keyframe count
            let mut num_kfs: ae_sys::A_long = 0;
            unsafe {
                let get_num = (*raw_kf).AEGP_GetStreamNumKFs.ok_or(ae::Error::Generic)?;
                let err = get_num(stream_h, &mut num_kfs);
                if err != 0 { return Err(ae::Error::Generic); }
            }

            // Require disposal before acquiring any owned stream value.
            let dispose = unsafe { (*raw_stream).AEGP_DisposeStreamValue.ok_or(ae::Error::Generic)? };
            let values = if num_kfs == 0 { 1 } else { num_kfs };
            for ki in 0..values {
                let mut stream_val: ae_sys::AEGP_StreamValue2 = unsafe { std::mem::zeroed() };
                unsafe {
                    let err = if num_kfs == 0 {
                        let time = ae_sys::A_Time {
                            value: in_data.current_time(), scale: in_data.time_scale(),
                        };
                        let get = (*raw_stream).AEGP_GetNewStreamValue.ok_or(ae::Error::Generic)?;
                        get(plugin_id, stream_h, ae_sys::AEGP_LTimeMode_LayerTime as i16,
                            &time, 0, &mut stream_val)
                    } else {
                        let get = (*raw_kf).AEGP_GetNewKeyframeValue.ok_or(ae::Error::Generic)?;
                        get(plugin_id, stream_h, ki, &mut stream_val)
                    };
                    if err != 0 { return Err(ae::Error::Generic); }
                }
                with_value_cleanup(&mut stream_val, |value| {
                    let outline_h = unsafe { value.val.mask };
                    if outline_h.is_null() { return Ok(()); }
                    let outline = ae::aegp::MaskOutlineHandle::from_raw(outline_h);
                    self.transform_outline(outline_suite, &outline, flip_h, flip_v, cos_r, sin_r)?;
                    let err = unsafe {
                        if num_kfs == 0 {
                            let set = (*raw_stream).AEGP_SetStreamValue.ok_or(ae::Error::Generic)?;
                            set(plugin_id, stream_h, value)
                        } else {
                            let set = (*raw_kf).AEGP_SetKeyframeValue.ok_or(ae::Error::Generic)?;
                            set(stream_h, ki, value)
                        }
                    };
                    if err != 0 { return Err(ae::Error::Generic); }
                    Ok(())
                }, |value| {
                    if unsafe { dispose(value) } != 0 { return Err(ae::Error::Generic); }
                    Ok(())
                })?;
            }
        }

        Ok(())
    }

    fn transform_outline(
        &self,
        suite: &ae::aegp::suites::MaskOutline,
        outline: &ae::aegp::MaskOutlineHandle,
        flip_h: bool,
        flip_v: bool,
        cos_r: f64,
        sin_r: f64,
    ) -> Result<(), ae::Error> {
        let num_segments = suite.mask_outline_num_segments(outline)?;
        let num_vertices=outline_vertex_count(num_segments,suite.is_mask_outline_open(outline)?)?;
        if num_vertices == 0 {
            return Ok(());
        }

        // First pass: compute centroid
        let mut cx = 0.0f64;
        let mut cy = 0.0f64;

        for i in 0..num_vertices {
            let vtx = suite.mask_outline_vertex_info(outline, i)?;
            cx += vtx.x;
            cy += vtx.y;
        }

        cx /= num_vertices as f64;
        cy /= num_vertices as f64;

        // Second pass: transform all vertices
        for i in 0..num_vertices {
            let mut vtx = suite.mask_outline_vertex_info(outline, i)?;

            let (nx, ny) = transform_point(vtx.x, vtx.y, cx, cy, flip_h, flip_v, cos_r, sin_r);
            let (ntix, ntiy) = transform_tangent(vtx.tan_in_x, vtx.tan_in_y, flip_h, flip_v, cos_r, sin_r);
            let (ntox, ntoy) = transform_tangent(vtx.tan_out_x, vtx.tan_out_y, flip_h, flip_v, cos_r, sin_r);

            vtx.x = nx;
            vtx.y = ny;
            vtx.tan_in_x = ntix;
            vtx.tan_in_y = ntiy;
            vtx.tan_out_x = ntox;
            vtx.tan_out_y = ntoy;

            suite.set_mask_outline_vertex_info(outline, i, &vtx)?;
        }

        Ok(())
    }
}

fn transform_point(
    x: f64, y: f64, cx: f64, cy: f64,
    flip_h: bool, flip_v: bool, cos_r: f64, sin_r: f64,
) -> (f64, f64) {
    let mut dx = x - cx;
    let mut dy = y - cy;
    if flip_h { dx = -dx; }
    if flip_v { dy = -dy; }
    let rx = dx * cos_r - dy * sin_r;
    let ry = dx * sin_r + dy * cos_r;
    (rx + cx, ry + cy)
}

fn outline_vertex_count(segments:i32,open:bool) -> Result<i32,ae::Error> {
    if segments<0 { return Err(ae::Error::BadCallbackParameter); }
    // Closed paths alias vertex[segments] to vertex[0]; open paths do not.
    segments.checked_add(i32::from(open)).ok_or(ae::Error::BadCallbackParameter)
}

fn transform_tangent(
    tx: f64, ty: f64,
    flip_h: bool, flip_v: bool, cos_r: f64, sin_r: f64,
) -> (f64, f64) {
    let mut dx = tx;
    let mut dy = ty;
    if flip_h { dx = -dx; }
    if flip_v { dy = -dy; }
    let rx = dx * cos_r - dy * sin_r;
    let ry = dx * sin_r + dy * cos_r;
    (rx, ry)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn open_outline_includes_terminal_vertex() {
        assert_eq!(outline_vertex_count(2,true).unwrap(),3);
        assert_eq!(outline_vertex_count(3,false).unwrap(),3);
        assert_eq!(outline_vertex_count(0,true).unwrap(),1);
        assert!(outline_vertex_count(-1,true).is_err());
        assert!(outline_vertex_count(i32::MAX,true).is_err());
        let points=[(0.0,0.0),(10.0,0.0),(20.0,0.0)];
        let count=outline_vertex_count(2,true).unwrap() as usize;
        let cx=points[..count].iter().map(|p|p.0).sum::<f64>()/count as f64;
        let transformed=points[..count].iter().map(|&(x,y)|transform_point(x,y,cx,0.0,true,false,1.0,0.0)).collect::<Vec<_>>();
        assert_eq!(transformed,[(20.0,0.0),(10.0,0.0),(0.0,0.0)]);
    }
    #[test]
    fn owned_value_is_disposed_once_after_work_or_error() {
        for work_ok in [false,true] { for dispose_ok in [false,true] {
            let mut value=(0,0);
            let result=with_value_cleanup(&mut value,|v| {v.0+=1; if work_ok {Ok(7)} else {Err("work")}},|v| {
                assert_eq!(v.0,1); v.1+=1; if dispose_ok {Ok(())} else {Err("dispose")}
            });
            assert_eq!(value,(1,1));
            assert_eq!(result,if !work_ok {Err("work")} else if !dispose_ok {Err("dispose")} else {Ok(7)});
        }}
    }
    #[test]
    fn nested_cleanup_runs_in_reverse_order_on_errors() {
        for fail in ["second acquire","undo begin","work","undo end","keyframe release","stream release",""] {
            let log=std::cell::RefCell::new(Vec::new());
            let step=|name| { log.borrow_mut().push(name); if fail==name {Err(name)} else {Ok(())} };
            let result=with_cleanup(|| {
                step("second acquire")?;
                with_cleanup(|| {
                    step("undo begin")?;
                    with_cleanup(||step("work"),||step("undo end"))
                },||step("keyframe release"))
            },||step("stream release"));
            assert_eq!(result,if fail.is_empty() {Ok(())} else {Err(fail)});
            let log=log.borrow();
            assert_eq!(log.last(),Some(&"stream release"));
            if fail!="second acquire" { assert!(log.contains(&"keyframe release")); }
            if fail!="second acquire" && fail!="undo begin" { assert!(log.contains(&"undo end")); }
        }
        let result: Result<(),&str>=with_cleanup(||Err("primary"),||Err("cleanup"));
        assert_eq!(result,Err("primary"));
    }
    fn close(actual:(f64,f64), expected:(f64,f64)) {
        assert!((actual.0-expected.0).abs()<1e-9 && (actual.1-expected.1).abs()<1e-9,
            "{actual:?} != {expected:?}");
    }
    #[test]
    fn flips_and_rotation_use_centroid_not_origin() {
        close(transform_point(12.0,23.0,10.0,20.0,true,false,1.0,0.0),(8.0,23.0));
        close(transform_point(12.0,23.0,10.0,20.0,false,true,1.0,0.0),(12.0,17.0));
        close(transform_point(12.0,23.0,10.0,20.0,false,false,0.0,1.0),(7.0,22.0));
        close(transform_point(12.0,23.0,10.0,20.0,true,false,0.0,1.0),(7.0,18.0));
    }
    #[test]
    fn tangent_matches_transformed_control_point_displacement() {
        for flip_h in [false,true] { for flip_v in [false,true] {
            for angle in [-720.0_f64,-37.0,0.0,90.0,281.0,3600.0] {
                let (sin,cos)=angle.to_radians().sin_cos();
                let point=transform_point(12.0,23.0,10.0,20.0,flip_h,flip_v,cos,sin);
                let control=transform_point(15.0,19.0,10.0,20.0,flip_h,flip_v,cos,sin);
                close(transform_tangent(3.0,-4.0,flip_h,flip_v,cos,sin),(control.0-point.0,control.1-point.1));
            }
        }}
    }
    #[test]
    fn transform_preserves_length_and_centroid() {
        let points=[(-4.0,-2.0),(8.0,-2.0),(8.0,10.0),(-4.0,10.0)];
        for flip_h in [false,true] { for flip_v in [false,true] {
            let (sin,cos)=0.713_f64.sin_cos();
            let transformed=points.map(|(x,y)|transform_point(x,y,2.0,4.0,flip_h,flip_v,cos,sin));
            close((transformed.iter().map(|p|p.0).sum::<f64>()/4.0,transformed.iter().map(|p|p.1).sum::<f64>()/4.0),(2.0,4.0));
            let tangent=transform_tangent(3.0,4.0,flip_h,flip_v,cos,sin);
            assert!((tangent.0.hypot(tangent.1)-5.0).abs()<1e-9);
        }}
    }
}
