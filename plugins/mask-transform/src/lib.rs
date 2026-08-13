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

        params.add(
            Params::Apply,
            "Apply Transform",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(false);
                f.set_label("Click to Apply");
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
                        self.apply_transform(params, &in_data)?;
                    }
                }
            }

            ae::Command::Render {
                in_layer,
                mut out_layer,
            } => {
                // Passthrough
                let w = in_layer.width() as usize;
                let h = in_layer.height() as usize;
                let src_stride = in_layer.buffer_stride();
                let dst_stride = out_layer.buffer_stride();
                let src_buf = in_layer.buffer();
                let dst_buf = out_layer.buffer_mut();
                for y in 0..h {
                    let src_off = y * src_stride;
                    let dst_off = y * dst_stride;
                    let row_len = w * 4;
                    if src_off + row_len <= src_buf.len() && dst_off + row_len <= dst_buf.len() {
                        dst_buf[dst_off..dst_off + row_len]
                            .copy_from_slice(&src_buf[src_off..src_off + row_len]);
                    }
                }
            }

            _ => {}
        }
        Ok(())
    }
}

// ---- Raw AEGP suite helpers ----

/// Acquire a raw AEGP suite pointer from the PICA basic suite.
unsafe fn acquire_suite<T>(
    pica: *const ae_sys::SPBasicSuite,
    name: &[u8],
    version: u32,
) -> Result<*const T, ae::Error> {
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
) {
    if let Some(release_fn) = (*pica).ReleaseSuite {
        let _ = release_fn(name.as_ptr() as *const i8, version as i32);
    }
}

// ---- Mask Transform Logic ----

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
        let raw_kf: *const ae_sys::AEGP_KeyframeSuite5 = unsafe {
            acquire_suite(pica, ae_sys::kAEGPKeyframeSuite, ae_sys::kAEGPKeyframeSuiteVersion5)?
        };

        let utility = ae::aegp::suites::Utility::new()?;
        utility.start_undo_group("Mask Transform")?;

        let num_masks = mask_suite.layer_num_masks(&layer)?;

        let result = self.transform_all_masks(
            plugin_id, &mask_suite, &outline_suite, &stream_suite,
            raw_stream, raw_kf,
            &layer, num_masks, target_mask,
            in_data, flip_h, flip_v, cos_r, sin_r,
        );

        utility.end_undo_group()?;

        // Release raw suites
        unsafe {
            release_suite(pica, ae_sys::kAEGPKeyframeSuite, ae_sys::kAEGPKeyframeSuiteVersion5);
            release_suite(pica, ae_sys::kAEGPStreamSuite, ae_sys::kAEGPStreamSuiteVersion6);
        }

        result
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

            if num_kfs == 0 {
                // No keyframes: get current value, modify, set back, dispose
                let time = ae_sys::A_Time {
                    value: in_data.current_time(),
                    scale: in_data.time_scale(),
                };

                let mut stream_val: ae_sys::AEGP_StreamValue2 = unsafe { std::mem::zeroed() };
                unsafe {
                    let get_val = (*raw_stream).AEGP_GetNewStreamValue.ok_or(ae::Error::Generic)?;
                    let err = get_val(
                        plugin_id, stream_h,
                        ae_sys::AEGP_LTimeMode_LayerTime as i16,
                        &time, 0, &mut stream_val,
                    );
                    if err != 0 { return Err(ae::Error::Generic); }
                }

                // The outline handle is valid until we dispose
                let outline_h = unsafe { stream_val.val.mask };
                if !outline_h.is_null() {
                    let outline = ae::aegp::MaskOutlineHandle::from_raw(outline_h);
                    self.transform_outline(outline_suite, &outline, flip_h, flip_v, cos_r, sin_r)?;

                    // Write modified value back using SetStreamValue
                    unsafe {
                        let set_val = (*raw_stream).AEGP_SetStreamValue.ok_or(ae::Error::Generic)?;
                        let err = set_val(plugin_id, stream_h, &mut stream_val);
                        if err != 0 {
                            // Dispose even on error
                            let dispose = (*raw_stream).AEGP_DisposeStreamValue.ok_or(ae::Error::Generic)?;
                            let _ = dispose(&mut stream_val);
                            return Err(ae::Error::Generic);
                        }
                    }

                    // Prevent Rust from doing anything with the handle on drop
                    let _ = outline;
                }

                // Dispose the stream value
                unsafe {
                    let dispose = (*raw_stream).AEGP_DisposeStreamValue.ok_or(ae::Error::Generic)?;
                    let _ = dispose(&mut stream_val);
                }
            } else {
                // Has keyframes: transform each
                for ki in 0..num_kfs {
                    let mut stream_val: ae_sys::AEGP_StreamValue2 = unsafe { std::mem::zeroed() };
                    unsafe {
                        let get_kf_val = (*raw_kf).AEGP_GetNewKeyframeValue.ok_or(ae::Error::Generic)?;
                        let err = get_kf_val(plugin_id, stream_h, ki, &mut stream_val);
                        if err != 0 { return Err(ae::Error::Generic); }
                    }

                    let outline_h = unsafe { stream_val.val.mask };
                    if !outline_h.is_null() {
                        let outline = ae::aegp::MaskOutlineHandle::from_raw(outline_h);
                        self.transform_outline(outline_suite, &outline, flip_h, flip_v, cos_r, sin_r)?;

                        // Write back
                        unsafe {
                            let set_kf_val = (*raw_kf).AEGP_SetKeyframeValue.ok_or(ae::Error::Generic)?;
                            let err = set_kf_val(stream_h, ki, &stream_val);
                            if err != 0 {
                                let dispose = (*raw_stream).AEGP_DisposeStreamValue.ok_or(ae::Error::Generic)?;
                                let _ = dispose(&mut stream_val);
                                let _ = outline;
                                return Err(ae::Error::Generic);
                            }
                        }

                        let _ = outline;
                    }

                    unsafe {
                        let dispose = (*raw_stream).AEGP_DisposeStreamValue.ok_or(ae::Error::Generic)?;
                        let _ = dispose(&mut stream_val);
                    }
                }
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
        if num_segments == 0 {
            return Ok(());
        }

        // First pass: compute centroid
        let mut cx = 0.0f64;
        let mut cy = 0.0f64;

        for i in 0..num_segments {
            let vtx = suite.mask_outline_vertex_info(outline, i)?;
            cx += vtx.x;
            cy += vtx.y;
        }

        cx /= num_segments as f64;
        cy /= num_segments as f64;

        // Second pass: transform all vertices
        for i in 0..num_segments {
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

