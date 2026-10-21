use after_effects as ae;
use std::panic::{self, AssertUnwindSafe};
mod path_offset;

/// Data passed from SmartPreRender to SmartRender via AE's pre_render_data mechanism.
struct PreRenderPathData {
    positions: Vec<(f64, f64)>,
    angles: Vec<f64>,
}

// ---- Debug logging ----

fn log_step(msg: &str) {
    use std::fs::OpenOptions;
    use std::io::Write;
    let path = "C:\\Users\\optim\\AEPluginBuild\\path_array_debug.log";
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{}", msg);
    }
}

// ---- Parameter IDs ----

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    SourceLayer,
    MaskIndex,
    Copies,
    Offset,
    Expansion,
    ExpansionSeparateXY,
    ExpansionY,
    CornerRound,
    Scale,
    AutoOrient,
    FixedRotation,
    Opacity,
    TimeOffsetFrames,
    CompositeOnOrig,
    // Layer 2 group (collapsible)
    Layer2GroupStart,
    Layer2Enabled,
    SourceLayer2,
    Layer2CopyFromLayer1,
    Layer2Scale,
    Layer2Rotation,
    Layer2Opacity,
    Layer2GroupEnd,
}

// ---- Plugin state ----

struct Plugin {
    plugin_id: Option<ae::aegp::PluginId>,
}

impl Default for Plugin {
    fn default() -> Self {
        Self {
            plugin_id: None,
        }
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
        params.add(Params::SourceLayer, "Source Layer", ae::LayerDef::new())?;

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
            Params::Copies,
            "Copies",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(1.0);
                f.set_valid_max(1000.0);
                f.set_slider_min(1.0);
                f.set_slider_max(100.0);
                f.set_default(10.0);
                f.set_precision(0);
            }),
        )?;

        params.add(
            Params::Offset,
            "Offset (%)",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(-100000.0);
                f.set_valid_max(100000.0);
                f.set_slider_min(-1000.0);
                f.set_slider_max(1000.0);
                f.set_default(0.0);
                f.set_precision(1);
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
            Params::Scale,
            "Scale (%)",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(1.0);
                f.set_valid_max(1000.0);
                f.set_slider_min(1.0);
                f.set_slider_max(200.0);
                f.set_default(100.0);
                f.set_precision(1);
            }),
        )?;

        params.add(
            Params::AutoOrient,
            "Auto-Orient",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(true);
                f.set_label("Orient to Path");
            }),
        )?;

        params.add(
            Params::FixedRotation,
            "Rotation (deg)",
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
            Params::Opacity,
            "Opacity (%)",
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
            Params::TimeOffsetFrames,
            "Time Offset (frames)",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(-1000.0);
                f.set_valid_max(1000.0);
                f.set_slider_min(-30.0);
                f.set_slider_max(30.0);
                f.set_default(0.0);
                f.set_precision(0);
            }),
        )?;

        params.add(
            Params::CompositeOnOrig,
            "Composite on Original",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(true);
                f.set_label("On");
            }),
        )?;

        params.add_group(
            Params::Layer2GroupStart,
            Params::Layer2GroupEnd,
            "Layer 2 (Alternate)",
            true,
            |inner| {
                inner.add(
                    Params::Layer2Enabled,
                    "Enable",
                    ae::CheckBoxDef::setup(|f| {
                        f.set_default(false);
                        f.set_label("Use Layer 2 on even copies");
                    }),
                )?;

                inner.add(Params::SourceLayer2, "Source Layer 2", ae::LayerDef::new())?;

                inner.add(
                    Params::Layer2CopyFromLayer1,
                    "Copy Settings from Layer 1",
                    ae::CheckBoxDef::setup(|f| {
                        f.set_default(true);
                        f.set_label("Use Layer 1 Scale/Rotation/Opacity");
                    }),
                )?;

                inner.add(
                    Params::Layer2Scale,
                    "Layer 2 Scale (%)",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(1.0);
                        f.set_valid_max(1000.0);
                        f.set_slider_min(1.0);
                        f.set_slider_max(200.0);
                        f.set_default(100.0);
                        f.set_precision(1);
                    }),
                )?;

                inner.add(
                    Params::Layer2Rotation,
                    "Layer 2 Rotation (deg)",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(-3600.0);
                        f.set_valid_max(3600.0);
                        f.set_slider_min(-360.0);
                        f.set_slider_max(360.0);
                        f.set_default(0.0);
                        f.set_precision(1);
                    }),
                )?;

                inner.add(
                    Params::Layer2Opacity,
                    "Layer 2 Opacity (%)",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(100.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(100.0);
                        f.set_default(100.0);
                        f.set_precision(1);
                    }),
                )?;

                Ok(())
            },
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
                    "ONMK PathArray v1.2\rDistribute layers along a mask path.\rWritten in Rust.",
                );
                return Ok(());
            }

            ae::Command::GlobalSetup => {
                out_data.set_out_flag(ae::OutFlags::NonParamVary, true);
                out_data.set_out_flag2(ae::OutFlags2::DependsOnUnreferencedMasks, true);

                if let Ok(utility) = ae::aegp::suites::Utility::new() {
                    self.plugin_id = utility.register_with_aegp("PathArray").ok();
                }
                return Ok(());
            }

            ae::Command::SmartPreRender { mut extra } => {
                log_step("=== SmartPreRender ENTER ===");
                // Compute mask path on main thread and pass to SmartRender via pre_render_data.
                let plugin_id = self.plugin_id;
                let cached = panic::catch_unwind(AssertUnwindSafe(|| {
                    compute_mask_path_from_params(&in_data, plugin_id, params)
                })).unwrap_or_else(|_| (Vec::new(), Vec::new()));
                let time_offset_frames = match params.get(Params::TimeOffsetFrames) {
                    Ok(p) => match p.as_float_slider() {
                        Ok(s) => s.value() as i32,
                        Err(_) => 0,
                    },
                    Err(_) => 0,
                };
                let layer2_enabled = params.get(Params::Layer2Enabled)?.as_checkbox()?.value();
                smart_pre_render(&in_data, &mut extra,
                    if layer2_enabled { Some(time_offset_frames) } else { None })?;
                extra.set_pre_render_data(PreRenderPathData {
                    positions: cached.0,
                    angles: cached.1,
                });
                log_step("=== SmartPreRender DONE (data set) ===");
                Ok(())
            }

            ae::Command::SmartRender { extra } => {
                let path = match extra.pre_render_data::<PreRenderPathData>() {
                    Some(d) => (d.positions.clone(), d.angles.clone()),
                    None => {
                        log_step("[SmartRender] no pre_render_data");
                        (Vec::new(), Vec::new())
                    }
                };
                log_step(&format!("[SmartRender] path positions={}", path.0.len()));
                smart_render_impl(&extra, params, &path)
            }

            ae::Command::SmartRenderGpu { extra } => {
                let path = match extra.pre_render_data::<PreRenderPathData>() {
                    Some(d) => (d.positions.clone(), d.angles.clone()),
                    None => (Vec::new(), Vec::new()),
                };
                smart_render_impl(&extra, params, &path)
            }

            _ => Ok(()),
        }
    }
}

// ---- Passthrough for crash recovery ----

#[allow(dead_code)]
fn passthrough_copy(src: &ae::Layer, dst: &mut ae::Layer) {
    let w = src.width().min(dst.width()) as usize;
    let h = src.height().min(dst.height()) as usize;
    let src_stride = src.buffer_stride();
    let dst_stride = dst.buffer_stride();
    let src_buf = src.buffer();
    let dst_buf = dst.buffer_mut();
    let bpp = match src.bit_depth() {
        16 => 8,
        32 => 16,
        _ => 4,
    };
    let row_bytes = w * bpp;
    for y in 0..h {
        let so = y * src_stride;
        let do_ = y * dst_stride;
        if so + row_bytes <= src_buf.len() && do_ + row_bytes <= dst_buf.len() {
            dst_buf[do_..do_ + row_bytes].copy_from_slice(&src_buf[so..so + row_bytes]);
        }
    }
}

// ---- SmartFX PreRender ----

fn smart_pre_render(
    in_data: &ae::InData,
    extra: &mut ae::pf::PreRenderExtra,
    time_offset_frames: Option<i32>,
) -> Result<(), ae::Error> {
    let req = extra.output_request();
    let cb = extra.callbacks();

    let current_time = in_data.current_time();
    let time_step = in_data.time_step();
    let time_scale = in_data.time_scale();
    let layer2_time = time_offset_frames.map(|frames|
        checked_offset_time(current_time, time_step, frames)).transpose()?;

    // Checkout input layer (checkout_id = 0, param index 0)
    let in_result = cb.checkout_layer(
        0, 0, &req, current_time, time_step, time_scale,
    )?;

    // Use input's result rects, clamped to the request rect
    let req_rect: ae::Rect = req.rect.into();
    let mut res: ae::Rect = in_result.result_rect.into();
    let mut max_res: ae::Rect = in_result.max_result_rect.into();

    res.left   = res.left.max(req_rect.left);
    res.top    = res.top.max(req_rect.top);
    res.right  = res.right.min(req_rect.right);
    res.bottom = res.bottom.min(req_rect.bottom);
    max_res.left   = max_res.left.max(req_rect.left);
    max_res.top    = max_res.top.max(req_rect.top);
    max_res.right  = max_res.right.min(req_rect.right);
    max_res.bottom = max_res.bottom.min(req_rect.bottom);

    extra.set_result_rect(res);
    extra.set_max_result_rect(max_res);

    // Checkout Source Layer 1 (param index 1, checkout_id 1) — optional
    let _ = cb.checkout_layer(
        1, 1, &req, current_time, time_step, time_scale,
    );

    // Checkout Source Layer 2 at offset time (param index 17, checkout_id 2) — optional
    if let Some(layer2_time) = layer2_time {
        let _ = cb.checkout_layer(
            SOURCE_LAYER2_PARAM_INDEX, 2, &req, layer2_time, time_step, time_scale,
        );
    }

    Ok(())
}

/// Param index of SourceLayer2 — must match its insertion order in params_setup.
/// Order: [implicit input=0][SourceLayer=1][MaskIndex=2][Copies=3][Offset=4][Expansion=5]
/// [ExpansionSeparateXY=6][ExpansionY=7][CornerRound=8][Scale=9][AutoOrient=10]
/// [FixedRotation=11][Opacity=12][TimeOffsetFrames=13][CompositeOnOrig=14]
/// [Layer2GroupStart=15][Layer2Enabled=16][SourceLayer2=17]
/// [Layer2CopyFromLayer1=18][Layer2Scale=19][Layer2Rotation=20][Layer2Opacity=21][Layer2GroupEnd=22]
const SOURCE_LAYER2_PARAM_INDEX: i32 = 17;

fn checked_offset_time(current: i32, step: i32, frames: i32) -> Result<i32, ae::Error> {
    let time = i64::from(current) + i64::from(step) * i64::from(frames);
    i32::try_from(time).map_err(|_| ae::Error::BadCallbackParameter)
}

#[test]
fn offset_time_preserves_signed_frames_and_rejects_wraparound() {
    assert_eq!(checked_offset_time(100, 10, 3), Ok(130));
    assert_eq!(checked_offset_time(100, 10, -3), Ok(70));
    assert_eq!(checked_offset_time(0, 10, -3), Ok(-30));
    assert_eq!(checked_offset_time(i32::MAX, 1, 0), Ok(i32::MAX));
    for (current, step, frames) in [(i32::MAX, 1, 1), (i32::MIN, 1, -1), (0, i32::MAX, 1000)] {
        assert_eq!(checked_offset_time(current, step, frames), Err(ae::Error::BadCallbackParameter));
    }
    // Intermediate product may exceed i32 while the final time remains valid.
    assert_eq!(checked_offset_time(-i32::MAX, i32::MAX, 2), Ok(i32::MAX));
}

// ---- SmartFX Render ----

/// Called from the main thread (SmartPreRender). Reads mask parameters and invokes AEGP.
fn compute_mask_path_from_params(
    in_data: &ae::InData,
    plugin_id: Option<ae::aegp::PluginId>,
    params: &ae::Parameters<Params>,
) -> (Vec<(f64, f64)>, Vec<f64>) {
    log_step("[compute] ENTER");
    let mask_index = match params.get(Params::MaskIndex) {
        Ok(p) => match p.as_float_slider() {
            Ok(s) => s.value() as i32,
            Err(_) => return (Vec::new(), Vec::new()),
        },
        Err(_) => return (Vec::new(), Vec::new()),
    };
    let copies = match params.get(Params::Copies) {
        Ok(p) => match p.as_float_slider() {
            Ok(s) => s.value() as usize,
            Err(_) => return (Vec::new(), Vec::new()),
        },
        Err(_) => return (Vec::new(), Vec::new()),
    };
    let offset = match params.get(Params::Offset) {
        Ok(p) => match p.as_float_slider() {
            Ok(s) => s.value() / 100.0,
            Err(_) => return (Vec::new(), Vec::new()),
        },
        Err(_) => return (Vec::new(), Vec::new()),
    };
    let expansion_x = match params.get(Params::Expansion) {
        Ok(p) => match p.as_float_slider() {
            Ok(s) => s.value(),
            Err(_) => return (Vec::new(), Vec::new()),
        },
        Err(_) => return (Vec::new(), Vec::new()),
    };
    let separate_xy = match params.get(Params::ExpansionSeparateXY) {
        Ok(p) => match p.as_checkbox() {
            Ok(s) => s.value(),
            Err(_) => false,
        },
        Err(_) => false,
    };
    let expansion_y = if separate_xy {
        match params.get(Params::ExpansionY) {
            Ok(p) => match p.as_float_slider() {
                Ok(s) => s.value(),
                Err(_) => expansion_x,
            },
            Err(_) => expansion_x,
        }
    } else {
        expansion_x
    };
    let corner_round = match params.get(Params::CornerRound) {
        Ok(p) => match p.as_float_slider() {
            Ok(s) => s.value(),
            Err(_) => 0.0,
        },
        Err(_) => 0.0,
    };
    log_step(&format!("[compute] params: mi={} copies={} offset={} exp_x={} exp_y={} corner={}", mask_index, copies, offset, expansion_x, expansion_y, corner_round));
    let r = read_mask_path_safe(in_data, plugin_id, mask_index, copies, offset, expansion_x, expansion_y, corner_round);
    log_step(&format!("[compute] DONE positions={}", r.0.len()));
    r
}

fn smart_render_impl(
    extra: &ae::pf::SmartRenderExtra,
    params: &ae::Parameters<Params>,
    path: &(Vec<(f64, f64)>, Vec<f64>),
) -> Result<(), ae::Error> {
    // Read all (non-mask) parameters — these are safe in any thread
    let scale = params.get(Params::Scale)?.as_float_slider()?.value() / 100.0;
    let auto_orient = params.get(Params::AutoOrient)?.as_checkbox()?.value();
    let fixed_rotation = params.get(Params::FixedRotation)?.as_float_slider()?.value().to_radians();
    let opacity = params.get(Params::Opacity)?.as_float_slider()?.value() / 100.0;
    let layer2_enabled = params.get(Params::Layer2Enabled)?.as_checkbox()?.value();
    let layer2_copy = params.get(Params::Layer2CopyFromLayer1)?.as_checkbox()?.value();
    let layer2_scale = if layer2_copy { scale } else {
        params.get(Params::Layer2Scale)?.as_float_slider()?.value() / 100.0
    };
    let layer2_rotation = if layer2_copy { fixed_rotation } else {
        params.get(Params::Layer2Rotation)?.as_float_slider()?.value().to_radians()
    };
    let layer2_opacity = if layer2_copy { opacity } else {
        params.get(Params::Layer2Opacity)?.as_float_slider()?.value() / 100.0
    };
    let composite_on_orig = params.get(Params::CompositeOnOrig)?.as_checkbox()?.value();

    let cb = extra.callbacks();
    let input_world = cb.checkout_layer_pixels(0)?.ok_or(ae::Error::Generic)?;
    with_pixel_checkin(|| {
    let mut output_world = cb.checkout_output()?.ok_or(ae::Error::Generic)?;
    validate_world(&output_world)?;

    let out_w = output_world.width() as usize;
    let out_h = output_world.height() as usize;
    if out_w == 0 || out_h == 0 {
        return Ok(());
    }

    // Read source layers via smart checkout
    let (s1_px, s1_w, s1_h) = match cb.checkout_layer_pixels(1) {
        Ok(Some(w)) => with_pixel_checkin(|| layer_to_flat(&w), || cb.checkin_layer_pixels(1))?,
        _ => (Vec::new(), 0, 0),
    };
    let (s2_px, s2_w, s2_h) = if layer2_enabled {
        match cb.checkout_layer_pixels(2) {
            Ok(Some(w)) => with_pixel_checkin(|| layer_to_flat(&w), || cb.checkin_layer_pixels(2))?,
            _ => (Vec::new(), 0, 0),
        }
    } else {
        (Vec::new(), 0, 0)
    };

    let has_s1 = !s1_px.is_empty() && s1_w > 0 && s1_h > 0;
    let has_s2 = !s2_px.is_empty() && s2_w > 0 && s2_h > 0;

    // If no source layer → passthrough input to output
    if !has_s1 && !has_s2 {
        let input_flat = layer_to_flat_padded(&input_world, out_w, out_h)?;
        flat_to_layer(&input_flat, &mut output_world, out_w, out_h)?;
        return Ok(());
    }

    // Use mask path pre-computed in pre_render (main thread)
    let positions = &path.0;
    let tangent_angles = &path.1;

    // If no mask path → passthrough
    if positions.is_empty() {
        let input_flat = layer_to_flat_padded(&input_world, out_w, out_h)?;
        flat_to_layer(&input_flat, &mut output_world, out_w, out_h)?;
        return Ok(());
    }

    // Build output buffer: start with original or transparent
    let mut output = if composite_on_orig {
        layer_to_flat_padded(&input_world, out_w, out_h)?
    } else {
        allocate_argb8(out_w,out_h)?
    };

    // Composite each copy
    for (i, &(px, py)) in positions.iter().enumerate() {
        let is_even = i % 2 == 0;
        // Even copies (0-indexed: i=1,3,5...) use Layer 2 when enabled and assigned
        let use_layer2 = layer2_enabled && !is_even && has_s2;
        let (src, sw, sh, this_scale, this_rot, this_opacity) = if use_layer2 {
            (s2_px.as_slice(), s2_w, s2_h, layer2_scale, layer2_rotation, layer2_opacity)
        } else if has_s1 {
            (s1_px.as_slice(), s1_w, s1_h, scale, fixed_rotation, opacity)
        } else {
            continue;
        };

        let angle = if auto_orient {
            tangent_angles.get(i).copied().unwrap_or(0.0) + this_rot
        } else {
            this_rot
        };

        composite_one(
            &mut output, out_w, out_h,
            src, sw, sh,
            px, py, angle,
            this_scale, this_opacity,
        );
    }

    // Write to output world
    flat_to_layer(&output, &mut output_world, out_w, out_h)?;

    Ok(())
    }, || cb.checkin_layer_pixels(0))
}

fn with_pixel_checkin<T>(work:impl FnOnce()->Result<T,ae::Error>, checkin:impl FnOnce()->Result<(),ae::Error>)->Result<T,ae::Error> {
    let result=panic::catch_unwind(AssertUnwindSafe(work)).unwrap_or(Err(ae::Error::Generic));
    let released=checkin();
    match result {Err(error)=>Err(error), Ok(value)=>released.map(|()|value)}
}

// ---- Mask path reading via AEGP suites (never panics) ----

fn read_mask_path_safe(
    in_data: &ae::InData,
    plugin_id: Option<ae::aegp::PluginId>,
    mask_index: i32,
    copies: usize,
    offset: f64,
    expansion_x: f64,
    expansion_y: f64,
    corner_round: f64,
) -> (Vec<(f64, f64)>, Vec<f64>) {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        read_mask_path_inner(in_data, plugin_id, mask_index, copies, offset, expansion_x, expansion_y, corner_round)
    }));
    match result {
        Ok(v) => v,
        Err(_) => (Vec::new(), Vec::new()),
    }
}

fn read_mask_path_inner(
    in_data: &ae::InData,
    plugin_id: Option<ae::aegp::PluginId>,
    mask_index: i32,
    copies: usize,
    offset: f64,
    expansion_x: f64,
    expansion_y: f64,
    corner_round: f64,
) -> (Vec<(f64, f64)>, Vec<f64>) {
    log_step("[read_mask] ENTER");
    let plugin_id_val = match plugin_id {
        Some(id) => id,
        None => { log_step("[read_mask] no plugin_id"); return (Vec::new(), Vec::new()); }
    };
    log_step("[read_mask] plugin_id ok");

    // Use crate wrappers to acquire the effect layer, mask ref, and stream ref.
    // These are safe — the bug is only in `new_stream_value`'s early disposal.
    let pf_iface = match ae::aegp::suites::PFInterface::new() {
        Ok(s) => s,
        Err(_) => { log_step("[read_mask] PFInterface::new FAIL"); return (Vec::new(), Vec::new()); }
    };
    let layer = match pf_iface.effect_layer(in_data.effect_ref()) {
        Ok(l) => l,
        Err(_) => { log_step("[read_mask] effect_layer FAIL"); return (Vec::new(), Vec::new()); }
    };
    let mask_suite = match ae::aegp::suites::Mask::new() {
        Ok(s) => s,
        Err(_) => { log_step("[read_mask] Mask::new FAIL"); return (Vec::new(), Vec::new()); }
    };
    let stream_suite = match ae::aegp::suites::Stream::new() {
        Ok(s) => s,
        Err(_) => { log_step("[read_mask] Stream::new FAIL"); return (Vec::new(), Vec::new()); }
    };
    log_step("[read_mask] crate suites ok");

    let num_masks = match mask_suite.layer_num_masks(&layer) {
        Ok(n) if n > 0 => n,
        Ok(_) => { log_step("[read_mask] num_masks=0"); return (Vec::new(), Vec::new()); }
        Err(_) => { log_step("[read_mask] layer_num_masks FAIL"); return (Vec::new(), Vec::new()); }
    };
    log_step(&format!("[read_mask] num_masks={}", num_masks));

    let mi = (mask_index - 1).max(0).min(num_masks - 1);
    let mask = match mask_suite.layer_mask_by_index(&layer, mi) {
        Ok(m) => m,
        Err(_) => { log_step("[read_mask] layer_mask_by_index FAIL"); return (Vec::new(), Vec::new()); }
    };

    let mask_stream = match stream_suite.new_mask_stream(
        &mask, plugin_id_val, ae::aegp::MaskStream::Outline,
    ) {
        Ok(s) => s,
        Err(_) => { log_step("[read_mask] new_mask_stream FAIL"); return (Vec::new(), Vec::new()); }
    };
    log_step("[read_mask] new_mask_stream ok");

    // ---- Direct FFI (works around crate bug where new_stream_value disposes the mask handle too early) ----
    let (vertices, is_closed) = unsafe {
        read_outline_raw_ffi(in_data, &mask_stream, plugin_id_val)
    };

    log_step(&format!("[read_mask] vertices read: {}, is_closed={}", vertices.len(), is_closed));
    if vertices.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let mut vertices = vertices;

    // Apply corner rounding: add bezier handles to sharp corners
    apply_corner_rounding(&mut vertices, is_closed, corner_round);

    log_step("[read_mask] calling sample_bezier_path");
    let result = sample_bezier_path(&vertices, is_closed, copies, offset, expansion_x, expansion_y);
    log_step(&format!("[read_mask] DONE positions={} angles={}", result.0.len(), result.1.len()));
    result
}

/// Directly call AEGP_StreamSuite6 + AEGP_MaskOutlineSuite3 via the PICA basic suite,
/// keeping the AEGP_StreamValue2 alive for the entire outline read. The crate's
/// `new_stream_value` disposes stream_value2 before returning, which invalidates the
/// mask outline handle — we can't use it for mask outlines.
unsafe fn read_outline_raw_ffi(
    in_data: &ae::InData,
    mask_stream: &ae::aegp::StreamReferenceHandle,
    plugin_id: ae::aegp::PluginId,
) -> (Vec<MaskVertex>, bool) {
    use ae::sys::*;

    let pica = in_data.pica_basic_suite_ptr();
    if pica.is_null() {
        log_step("[raw_ffi] pica null");
        return (Vec::new(), true);
    }

    let acquire = match (*pica).AcquireSuite {
        Some(f) => f,
        None => { log_step("[raw_ffi] AcquireSuite None"); return (Vec::new(), true); }
    };
    let release = (*pica).ReleaseSuite;

    // Acquire Stream Suite 6
    let stream_suite_name = kAEGPStreamSuite.as_ptr() as *const std::os::raw::c_char;
    let stream_suite_version = kAEGPStreamSuiteVersion6 as i32;
    let mut stream_suite_ptr: *const std::ffi::c_void = std::ptr::null();
    if acquire(stream_suite_name, stream_suite_version, &mut stream_suite_ptr) != 0
        || stream_suite_ptr.is_null()
    {
        log_step("[raw_ffi] AcquireSuite Stream6 FAIL");
        return (Vec::new(), true);
    }
    let stream_suite = &*(stream_suite_ptr as *const AEGP_StreamSuite6);
    log_step("[raw_ffi] Stream6 acquired");

    // Acquire Mask Outline Suite 3
    let outline_suite_name = kAEGPMaskOutlineSuite.as_ptr() as *const std::os::raw::c_char;
    let outline_suite_version = kAEGPMaskOutlineSuiteVersion3 as i32;
    let mut outline_suite_ptr: *const std::ffi::c_void = std::ptr::null();
    if acquire(outline_suite_name, outline_suite_version, &mut outline_suite_ptr) != 0
        || outline_suite_ptr.is_null()
    {
        log_step("[raw_ffi] AcquireSuite MaskOutline3 FAIL");
        if let Some(r) = release {
            r(stream_suite_name, stream_suite_version);
        }
        return (Vec::new(), true);
    }
    let outline_suite = &*(outline_suite_ptr as *const AEGP_MaskOutlineSuite3);
    log_step("[raw_ffi] MaskOutline3 acquired");

    // Build A_Time
    let time = A_Time {
        value: in_data.current_time(),
        scale: in_data.time_scale(),
    };

    // Get new stream value — and DO NOT dispose until after we read the outline
    let mut stream_val2: AEGP_StreamValue2 = std::mem::zeroed();
    let get_val_fn = match stream_suite.AEGP_GetNewStreamValue {
        Some(f) => f,
        None => {
            log_step("[raw_ffi] AEGP_GetNewStreamValue None");
            if let Some(r) = release {
                r(outline_suite_name, outline_suite_version);
                r(stream_suite_name, stream_suite_version);
            }
            return (Vec::new(), true);
        }
    };

    let err = get_val_fn(
        plugin_id,
        <ae::aegp::StreamReferenceHandle as after_effects::AsPtr<_>>::as_ptr(mask_stream),
        AEGP_LTimeMode_LayerTime as AEGP_LTimeMode,
        &time as *const A_Time,
        0,
        &mut stream_val2,
    );
    if err != 0 {
        log_step(&format!("[raw_ffi] AEGP_GetNewStreamValue err={}", err));
        if let Some(r) = release {
            r(outline_suite_name, outline_suite_version);
            r(stream_suite_name, stream_suite_version);
        }
        return (Vec::new(), true);
    }
    log_step("[raw_ffi] AEGP_GetNewStreamValue ok");

    let outline_h: AEGP_MaskOutlineValH = stream_val2.val.mask;
    if outline_h.is_null() {
        log_step("[raw_ffi] outline_h NULL");
        if let Some(dispose) = stream_suite.AEGP_DisposeStreamValue {
            dispose(&mut stream_val2);
        }
        if let Some(r) = release {
            r(outline_suite_name, outline_suite_version);
            r(stream_suite_name, stream_suite_version);
        }
        return (Vec::new(), true);
    }
    log_step(&format!("[raw_ffi] outline_h ptr={:p}", outline_h));

    // Open paths have one more unique vertex than segments. Reject the whole
    // outline on a failed read; skipped vertices would create a different path.
    let mut is_open: A_Boolean = 0;
    let mut num_segments: A_long = 0;
    let vertices = (|| {
        if outline_suite.AEGP_IsMaskOutlineOpen?(outline_h, &mut is_open) != 0 {return None;}
        if outline_suite.AEGP_GetMaskOutlineNumSegments?(outline_h, &mut num_segments) != 0 {return None;}
        let read = outline_suite.AEGP_GetMaskOutlineVertexInfo?;
        collect_outline_vertices(num_segments, is_open != 0, |index| {
            let mut vtx: AEGP_MaskVertex = std::mem::zeroed();
            if read(outline_h, index, &mut vtx) != 0 {return None;}
            Some(MaskVertex {x:vtx.x, y:vtx.y, tan_in_x:vtx.tan_in_x,
                tan_in_y:vtx.tan_in_y, tan_out_x:vtx.tan_out_x, tan_out_y:vtx.tan_out_y})
        })
    })().unwrap_or_default();
    log_step(&format!("[raw_ffi] vertices collected: {}", vertices.len()));

    let is_closed = is_open == 0;

    // Now safe to dispose — we're done reading the outline
    if let Some(dispose) = stream_suite.AEGP_DisposeStreamValue {
        dispose(&mut stream_val2);
    }
    log_step("[raw_ffi] DisposeStreamValue ok");

    // Release suites
    if let Some(r) = release {
        r(outline_suite_name, outline_suite_version);
        r(stream_suite_name, stream_suite_version);
    }
    log_step("[raw_ffi] suites released");

    (vertices, is_closed)
}

// ---- Vertex helper ----

fn collect_outline_vertices(segments:i32, open:bool, mut read:impl FnMut(i32)->Option<MaskVertex>) -> Option<Vec<MaskVertex>> {
    if segments < 0 {return None;}
    let count=segments.checked_add(i32::from(open))?;
    let mut result=Vec::new();
    result.try_reserve_exact(count as usize).ok()?;
    for index in 0..count {result.push(read(index)?);}
    Some(result)
}

#[cfg(test)]
mod outline_tests {
    use super::*;
    #[test]
    fn world_layout_rejects_invalid_geometry_and_maps_reversed_rows() {
        for bpp in [4,8,16] {
            let stride=3*bpp+8;
            assert_eq!(checked_world_layout(3,5,stride as isize,bpp),Ok(stride));
            assert_eq!(checked_world_layout(3,5,-(stride as isize),bpp),Ok(stride));
            assert!(checked_world_layout(3,5,(3*bpp-1) as isize,bpp).is_err());
            for y in 0..5 {
                assert_eq!(row_offset(y,5,stride,false),y*stride);
                assert_eq!(row_offset(y,5,stride,true),(4-y)*stride);
            }
        }
        assert!(checked_world_layout(usize::MAX,1,4,4).is_err());
        assert!(checked_world_layout(1,1,i32::MIN as isize,4).is_err());
        assert!(checked_world_layout(1,1,isize::MIN,4).is_err());
        assert!(checked_world_layout(1,2,isize::MAX,4).is_err());
        assert_eq!(checked_world_layout(0,5,0,4),Ok(0));
    }
    #[test]
    fn frame_allocation_checks_overflow_and_capacity() {
        assert_eq!(allocate_argb8(3,5).unwrap(),vec![0;60]);
        assert_eq!(allocate_argb8(0,usize::MAX).unwrap(),Vec::<u8>::new());
        assert_eq!(allocate_argb8(usize::MAX,2),Err(ae::Error::OutOfMemory));
        assert_eq!(allocate_argb8(usize::MAX/4,1),Err(ae::Error::OutOfMemory));
    }
    #[test]
    fn zero_scale_draws_nothing() {
        let src=[255,100,50,20].repeat(4);
        let mut dst=vec![0;16];
        composite_one(&mut dst,2,2,&src,2,2,1.0,1.0,0.0,0.0,1.0);
        assert!(dst.iter().all(|v|*v==0));
    }
    #[test]
    fn malformed_and_extreme_composites_do_not_write() {
        let src=[255,100,50,20].repeat(4);
        for (px,scale) in [(f64::NAN,1.0),(f64::INFINITY,1.0),(1e30,1.0),(-1e30,1.0),(1.0,f64::INFINITY),(1.0,-1.0)] {
            let mut dst=vec![99;16];
            composite_one(&mut dst,2,2,&src,2,2,px,1.0,0.0,scale,1.0);
            assert_eq!(dst,[99;16]);
        }
        for (w,h,len) in [(2,2,3),(usize::MAX,2,16)] {
            let mut dst=vec![99;16];
            composite_one(&mut dst,2,2,&src[..len],w,h,1.0,1.0,0.0,1.0,1.0);
            assert_eq!(dst,[99;16]);
        }
    }
    #[test]
    fn identity_copy_keeps_all_borders_and_single_pixel_axes() {
        for (w,h) in [(1,1),(1,7),(9,1),(3,5)] {
            let src:Vec<u8>=(0..w*h).flat_map(|i|[255,(i*7) as u8,(i*3) as u8,42]).collect();
            let mut dst=vec![0;src.len()];
            composite_one(&mut dst,w,h,&src,w,h,w as f64/2.0,h as f64/2.0,0.0,1.0,1.0);
            assert_eq!(dst,src,"identity {w}x{h}");
        }
        let src=[255,0,0,0, 255,100,0,0, 255,0,100,0, 255,100,100,0];
        assert_eq!(bilinear_sample(&src,2,2,0,0,0.5,0.5),[255,50,50,0]);
        assert_eq!(bilinear_sample(&src,2,2,1,0,0.75,0.5),[255,100,50,0]);
        assert_eq!(bilinear_sample(&src,2,2,1,1,0.75,0.75),[255,100,100,0]);
    }
    #[test]
    fn composite_preserves_premultiplied_source_without_double_alpha() {
        let src=[128,100,64,2].repeat(4);
        for opacity in [1.0,0.5,0.0] {
            let mut output=vec![0;16];
            composite_one(&mut output,2,2,&src,2,2,1.0,1.0,0.0,1.0,opacity);
            let op=(opacity*255.0) as u32;
            let expected:Vec<u8>=[128,100,64,2].map(|v|((v*op+127)/255) as u8).to_vec();
            assert_eq!(&output[..4],expected.as_slice());
        }
        let mut output=[128,32,16,8].repeat(4);
        composite_one(&mut output,2,2,&src,2,2,1.0,1.0,0.0,1.0,1.0);
        assert_eq!(&output[..4], &[192,116,72,6]);
    }
    #[test]
    fn acquired_pixels_released_on_success_error_and_panic() {
        for stage in 0..3 {for release_fails in [false,true] {
            let calls=std::cell::Cell::new(0);
            let result=with_pixel_checkin(|| match stage {
                0=>Ok(7), 1=>Err(ae::Error::BadCallbackParameter), _=>panic!("injected work panic"),
            },|| {calls.set(calls.get()+1); if release_fails {Err(ae::Error::OutOfMemory)} else {Ok(())}});
            assert_eq!(calls.get(),1);
            assert_eq!(result,match stage {0=>if release_fails {Err(ae::Error::OutOfMemory)} else {Ok(7)},1=>Err(ae::Error::BadCallbackParameter),_=>Err(ae::Error::Generic)});
        }}
        let order=std::cell::RefCell::new(Vec::new());
        let result:Result<(),ae::Error>=with_pixel_checkin(|| {
            with_pixel_checkin(|| Err(ae::Error::BadCallbackParameter),|| {order.borrow_mut().push(1); Err(ae::Error::OutOfMemory)})
        },|| {order.borrow_mut().push(0); Ok(())});
        assert_eq!(result,Err(ae::Error::BadCallbackParameter));
        assert_eq!(*order.borrow(),[1,0]);
    }
    fn vertex(x:i32)->MaskVertex {MaskVertex {x:x as f64,y:0.0,tan_in_x:0.0,tan_in_y:0.0,tan_out_x:0.0,tan_out_y:0.0}}
    #[test]
    fn open_terminal_included_closed_duplicate_excluded() {
        let open=collect_outline_vertices(2,true,|i|Some(vertex(i))).unwrap();
        assert_eq!(open.iter().map(|v|v.x).collect::<Vec<_>>(),[0.0,1.0,2.0]);
        assert_eq!(collect_outline_vertices(2,false,|i|Some(vertex(i))).unwrap().len(),2);
        assert_eq!(collect_outline_vertices(0,true,|i|Some(vertex(i))).unwrap().len(),1);
    }
    #[test]
    fn failed_read_discards_partial_path() {
        assert!(collect_outline_vertices(3,false,|i|if i==1 {None} else {Some(vertex(i))}).is_none());
        assert!(collect_outline_vertices(-1,true,|_|panic!("unexpected read")).is_none());
        assert!(collect_outline_vertices(i32::MAX,true,|_|panic!("unexpected read")).is_none());
    }
}

#[derive(Clone)]
struct MaskVertex {
    x: f64,
    y: f64,
    tan_in_x: f64,
    tan_in_y: f64,
    tan_out_x: f64,
    tan_out_y: f64,
}

// ---- Corner rounding ----
// Splits sharp vertices into two points with an arc between them, cutting the corner.

fn apply_corner_rounding(vertices: &mut Vec<MaskVertex>, is_closed: bool, round_px: f64) {
    if round_px < 0.5 || vertices.len() < 2 {
        return;
    }
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

        // Point A: on incoming edge, pulled back from corner
        let ax = vtx.x + dir_in_x * pull_in;
        let ay = vtx.y + dir_in_y * pull_in;

        // Point B: on outgoing edge, pushed forward from corner
        let bx = vtx.x + dir_out_x * pull_out;
        let by = vtx.y + dir_out_y * pull_out;

        let kappa = 0.5523;

        result.push(MaskVertex {
            x: ax, y: ay,
            tan_in_x: 0.0, tan_in_y: 0.0,
            tan_out_x: -dir_in_x * pull_in * kappa,
            tan_out_y: -dir_in_y * pull_in * kappa,
        });

        result.push(MaskVertex {
            x: bx, y: by,
            tan_in_x: -dir_out_x * pull_out * kappa,
            tan_in_y: -dir_out_y * pull_out * kappa,
            tan_out_x: 0.0, tan_out_y: 0.0,
        });
    }

    *vertices = result;
}

// ---- Bezier path sampling ----

fn sample_bezier_path(
    vertices: &[MaskVertex],
    is_closed: bool,
    copies: usize,
    offset: f64,
    expansion_x: f64,
    expansion_y: f64,
) -> (Vec<(f64, f64)>, Vec<f64>) {
    if vertices.is_empty() || copies == 0 {
        return (Vec::new(), Vec::new());
    }

    let samples_per_seg = 64usize;
    let mut polyline: Vec<(f64, f64)> = Vec::new();
    let mut arc_lengths: Vec<f64> = Vec::new();
    let mut tangents: Vec<(f64, f64)> = Vec::new();

    let seg_count = if is_closed { vertices.len() } else { vertices.len().saturating_sub(1) };

    for seg in 0..seg_count {
        let v0 = &vertices[seg];
        let v1 = if is_closed {
            &vertices[(seg + 1) % vertices.len()]
        } else {
            &vertices[seg + 1]
        };

        let p0 = (v0.x, v0.y);
        let p1 = (v0.x + v0.tan_out_x, v0.y + v0.tan_out_y);
        let p2 = (v1.x + v1.tan_in_x, v1.y + v1.tan_in_y);
        let p3 = (v1.x, v1.y);

        for si in 0..samples_per_seg {
            let t = si as f64 / samples_per_seg as f64;
            let pt = cubic_bezier_f64(p0, p1, p2, p3, t);
            let tan = cubic_bezier_tangent(p0, p1, p2, p3, t);

            if polyline.is_empty() {
                arc_lengths.push(0.0);
            } else {
                let prev = *polyline.last().unwrap();
                let dx = pt.0 - prev.0;
                let dy = pt.1 - prev.1;
                let dist = (dx * dx + dy * dy).sqrt();
                arc_lengths.push(arc_lengths.last().unwrap() + dist);
            }
            polyline.push(pt);
            tangents.push(tan);
        }
    }

    // Add final point for open paths
    if !is_closed {
        if let Some(last) = vertices.last() {
            let pt = (last.x, last.y);
            if let Some(&prev) = polyline.last() {
                let dx = pt.0 - prev.0;
                let dy = pt.1 - prev.1;
                let dist = (dx * dx + dy * dy).sqrt();
                arc_lengths.push(arc_lengths.last().unwrap_or(&0.0) + dist);
            } else {
                arc_lengths.push(0.0);
            }
            polyline.push(pt);
            tangents.push(tangents.last().copied().unwrap_or((1.0, 0.0)));
        }
    }

    if expansion_x.abs() > 0.001 || expansion_y.abs() > 0.001 {
        polyline = path_offset::offset(&polyline, is_closed, expansion_x, expansion_y);
        // Include the closing edge when redistributing along the expanded path.
        // Leave the zero-expansion sampling and tangent interpolation untouched.
        if is_closed && !polyline.is_empty() { polyline.push(polyline[0]); }
        arc_lengths.clear();
        tangents.clear();
        let mut length = 0.0;
        for i in 0..polyline.len() {
            if i > 0 { length += (polyline[i].0-polyline[i-1].0).hypot(polyline[i].1-polyline[i-1].1); }
            arc_lengths.push(length);
            let (a,b) = if i+1 < polyline.len() {(polyline[i],polyline[i+1])}
                else if i > 0 {(polyline[i-1],polyline[i])} else {(polyline[i],polyline[i])};
            tangents.push((b.0-a.0,b.1-a.1));
        }
    }

    if polyline.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let total_length = *arc_lengths.last().unwrap();
    if total_length < 0.001 {
        let pt = polyline[0];
        return (vec![pt; copies], vec![0.0; copies]);
    }

    let mut positions = Vec::with_capacity(copies);
    let mut angles = Vec::with_capacity(copies);

    for i in 0..copies {
        let frac = if is_closed {
            i as f64 / copies as f64
        } else if copies == 1 {
            0.5
        } else {
            i as f64 / (copies - 1) as f64
        };

        let mut target_frac = frac + offset;
        if is_closed {
            target_frac = target_frac.rem_euclid(1.0);
        } else {
            target_frac = target_frac.clamp(0.0, 1.0);
        }

        let target_len = target_frac * total_length;

        let idx = match arc_lengths.binary_search_by(|a| a.partial_cmp(&target_len).unwrap()) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };
        let idx = idx.min(polyline.len() - 1);

        if idx + 1 < polyline.len() {
            let seg_start = arc_lengths[idx];
            let seg_end = arc_lengths[idx + 1];
            let seg_len = seg_end - seg_start;
            let local_t = if seg_len > 0.001 {
                (target_len - seg_start) / seg_len
            } else {
                0.0
            };
            let x = polyline[idx].0 + (polyline[idx + 1].0 - polyline[idx].0) * local_t;
            let y = polyline[idx].1 + (polyline[idx + 1].1 - polyline[idx].1) * local_t;
            positions.push((x, y));

            let tx = tangents[idx].0 + (tangents[idx + 1].0 - tangents[idx].0) * local_t;
            let ty = tangents[idx].1 + (tangents[idx + 1].1 - tangents[idx].1) * local_t;
            angles.push(ty.atan2(tx));
        } else {
            positions.push(polyline[idx]);
            let tan = tangents[idx];
            angles.push(tan.1.atan2(tan.0));
        }
    }

    (positions, angles)
}

// ---- Cubic bezier ----

#[test]
fn expanded_path_spacing_uses_offset_contour() {
    let vertex=|x,y| MaskVertex {x,y,tan_in_x:0.0,tan_in_y:0.0,tan_out_x:0.0,tan_out_y:0.0};
    let line=[vertex(0.0,0.0),vertex(10.0,0.0)];
    let (points,angles)=sample_bezier_path(&line,false,3,0.0,2.0,2.0);
    for (point,expected) in points.iter().zip([(0.0,-2.0),(5.0,-2.0),(10.0,-2.0)]) {
        assert!((point.0-expected.0).abs()<1e-8 && (point.1-expected.1).abs()<1e-8);
    }
    assert!(angles.iter().all(|angle|angle.abs()<1e-8));
    let square=[vertex(0.0,0.0),vertex(10.0,0.0),vertex(10.0,10.0),vertex(0.0,10.0)];
    let (points,_)=sample_bezier_path(&square,true,4,0.0,2.0,2.0);
    for (point,expected) in points.iter().zip([(-2.0,-2.0),(12.0,-2.0),(12.0,12.0),(-2.0,12.0)]) {
        assert!((point.0-expected.0).abs()<1e-8 && (point.1-expected.1).abs()<1e-8);
    }
}

fn cubic_bezier_f64(
    p0: (f64, f64), p1: (f64, f64), p2: (f64, f64), p3: (f64, f64), t: f64,
) -> (f64, f64) {
    let u = 1.0 - t;
    let uu = u * u;
    let tt = t * t;
    let a = uu * u;
    let b = 3.0 * uu * t;
    let c = 3.0 * u * tt;
    let d = tt * t;
    (
        a * p0.0 + b * p1.0 + c * p2.0 + d * p3.0,
        a * p0.1 + b * p1.1 + c * p2.1 + d * p3.1,
    )
}

fn cubic_bezier_tangent(
    p0: (f64, f64), p1: (f64, f64), p2: (f64, f64), p3: (f64, f64), t: f64,
) -> (f64, f64) {
    let u = 1.0 - t;
    let a = 3.0 * u * u;
    let b = 6.0 * u * t;
    let c = 3.0 * t * t;
    (
        a * (p1.0 - p0.0) + b * (p2.0 - p1.0) + c * (p3.0 - p2.0),
        a * (p1.1 - p0.1) + b * (p2.1 - p1.1) + c * (p3.1 - p2.1),
    )
}

// ---- Composite a single copy at (px, py) with rotation ----
// NOTE: Non-smart Render path — in_layer/out_layer share the same geometry,
//       so positions are directly in layer pixel coordinates (no origin offset).

fn composite_one(
    output: &mut [u8],
    out_w: usize,
    out_h: usize,
    src: &[u8],
    src_w: usize,
    src_h: usize,
    px: f64,
    py: f64,
    angle: f64,
    scale: f64,
    opacity: f64,
) {
    if src_w == 0 || src_h == 0 || out_w == 0 || out_h == 0
        || ![px,py,angle,scale,opacity].iter().all(|v|v.is_finite()) || scale<=0.0 || opacity<=0.0 {
        return;
    }
    let Some(src_len)=src_w.checked_mul(src_h).and_then(|n|n.checked_mul(4)) else {return;};
    let Some(out_len)=out_w.checked_mul(out_h).and_then(|n|n.checked_mul(4)) else {return;};
    if src.len()<src_len || output.len()<out_len {return;}

    let half_w = src_w as f64 * 0.5 * scale;
    let half_h = src_h as f64 * 0.5 * scale;
    let opacity_u8 = (opacity * 255.0).clamp(0.0, 255.0) as u32;

    let cos_a = angle.cos();
    let sin_a = angle.sin();

    // Bounding box of the rotated source in output space
    let corners = [
        (-half_w, -half_h), (half_w, -half_h),
        (-half_w, half_h), (half_w, half_h),
    ];
    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;
    for &(cx, cy) in &corners {
        let rx = cx * cos_a - cy * sin_a + px;
        let ry = cx * sin_a + cy * cos_a + py;
        if !rx.is_finite() || !ry.is_finite() {return;}
        min_x = min_x.min(rx);
        min_y = min_y.min(ry);
        max_x = max_x.max(rx);
        max_y = max_y.max(ry);
    }

    let ox_start = min_x.floor().clamp(0.0,out_w as f64) as usize;
    let oy_start = min_y.floor().clamp(0.0,out_h as f64) as usize;
    let ox_end = (max_x.ceil()+1.0).clamp(0.0,out_w as f64) as usize;
    let oy_end = (max_y.ceil()+1.0).clamp(0.0,out_h as f64) as usize;

    let inv_scale = 1.0 / scale;
    if !inv_scale.is_finite() {return;}

    for oy in oy_start..oy_end {
        let row_off = oy * out_w * 4;
        for ox in ox_start..ox_end {
            let dx = ox as f64 - px;
            let dy = oy as f64 - py;

            // Inverse rotate
            let rx = dx * cos_a + dy * sin_a;
            let ry = -dx * sin_a + dy * cos_a;

            // To source coordinates
            let sx = rx * inv_scale + src_w as f64 * 0.5;
            let sy = ry * inv_scale + src_h as f64 * 0.5;

            if !sx.is_finite() || !sy.is_finite() || sx<0.0 || sy<0.0 || sx>=src_w as f64 || sy>=src_h as f64 {
                continue;
            }
            let ix = sx.floor() as usize;
            let iy = sy.floor() as usize;
            let fx = sx - ix as f64;
            let fy = sy - iy as f64;

            let sample = bilinear_sample(src, src_w, src_h, ix as usize, iy as usize, fx, fy);

            let sa = ((sample[0] as u32 * opacity_u8 + 127) / 255) as u8;
            if sa == 0 {
                continue;
            }

            let dst_idx = row_off + ox * 4;
            if dst_idx + 3 >= output.len() {
                continue;
            }

            // Alpha composite (ARGB over)
            let sa32 = sa as u32;
            let inv_sa = 255 - sa32;
            let da = output[dst_idx] as u32;

            output[dst_idx]     = (sa32 + (da * inv_sa + 127) / 255).min(255) as u8;
            // Samples are already premultiplied. Apply only user opacity to
            // source RGB, not its alpha again.
            for ch in 1..4 {
                output[dst_idx + ch] = ((sample[ch] as u32 * opacity_u8
                    + output[dst_idx + ch] as u32 * inv_sa + 127) / 255).min(255) as u8;
            }
        }
    }
}

/// Bilinear sample with edge-clamped neighbors. Caller guarantees ix < w, iy < h.
#[inline]
fn bilinear_sample(src: &[u8], w: usize, h: usize, ix: usize, iy: usize, fx: f64, fy: f64) -> [u8; 4] {
    let nx=(ix+1).min(w-1);
    let ny=(iy+1).min(h-1);
    let i00 = (iy * w + ix) * 4;
    let i10 = (iy * w + nx) * 4;
    let i01 = (ny * w + ix) * 4;
    let i11 = (ny * w + nx) * 4;

    let fx32 = fx as f32;
    let fy32 = fy as f32;
    let inv_fx = 1.0 - fx32;
    let inv_fy = 1.0 - fy32;
    let w00 = inv_fx * inv_fy;
    let w10 = fx32 * inv_fy;
    let w01 = inv_fx * fy32;
    let w11 = fx32 * fy32;

    let mut result = [0u8; 4];
    for ch in 0..4 {
        let v = src[i00 + ch] as f32 * w00
            + src[i10 + ch] as f32 * w10
            + src[i01 + ch] as f32 * w01
            + src[i11 + ch] as f32 * w11;
        result[ch] = v as u8;
    }
    result
}

// ---- Layer I/O helpers ----

fn checked_world_layout(w:usize,h:usize,row_bytes:isize,bpp:usize)->Result<usize,ae::Error> {
    if w>i32::MAX as usize || h>i32::MAX as usize || row_bytes==i32::MIN as isize {return Err(ae::Error::BadCallbackParameter);}
    if w==0 || h==0 {return Ok(0);}
    let stride=row_bytes.checked_abs().ok_or(ae::Error::BadCallbackParameter)? as usize;
    let row=w.checked_mul(bpp).ok_or(ae::Error::OutOfMemory)?;
    let len=stride.checked_mul(h).ok_or(ae::Error::OutOfMemory)?;
    if stride<row || len>isize::MAX as usize {return Err(ae::Error::BadCallbackParameter);}
    Ok(stride)
}

fn validate_world(layer:&ae::Layer)->Result<i16,ae::Error> {
    let depth=if layer.width()==0 || layer.height()==0 {8} else {match layer.pixel_format()? {
        ae::PixelFormat::Argb32=>8, ae::PixelFormat::Argb64=>16, ae::PixelFormat::Argb128=>32,
        _=>return Err(ae::Error::BadCallbackParameter),
    }};
    checked_world_layout(layer.width(),layer.height(),layer.row_bytes(),depth as usize/2)?;
    if layer.width()!=0 && layer.height()!=0 && unsafe {layer.data_ptr()}.is_null() {return Err(ae::Error::BadCallbackParameter);}
    Ok(depth)
}

fn row_offset(y:usize,h:usize,stride:usize,reverse:bool)->usize {
    (if reverse {h-1-y} else {y})*stride
}

fn allocate_argb8(w:usize,h:usize)->Result<Vec<u8>,ae::Error> {
    let len=w.checked_mul(h).and_then(|n|n.checked_mul(4)).ok_or(ae::Error::OutOfMemory)?;
    let mut pixels=Vec::new();
    pixels.try_reserve_exact(len).map_err(|_|ae::Error::OutOfMemory)?;
    pixels.resize(len,0);
    Ok(pixels)
}

fn layer_to_flat(layer: &ae::Layer) -> Result<(Vec<u8>, usize, usize),ae::Error> {
    let depth = validate_world(layer)?;
    let w = layer.width() as usize;
    let h = layer.height() as usize;
    if w == 0 || h == 0 {
        return Ok((Vec::new(), 0, 0));
    }
    let stride = layer.buffer_stride();
    let reverse = layer.row_bytes()<0;
    let buf = layer.buffer();
    let mut flat = allocate_argb8(w,h)?;
    match depth {
        16 => {
            for y in 0..h {
                let src_row = row_offset(y,h,stride,reverse);
                let dst_row = y * w * 4;
                for x in 0..w {
                    let si = src_row + x * 8;
                    let di = dst_row + x * 4;
                    if si + 7 < buf.len() && di + 3 < flat.len() {
                        for ch in 0..4usize {
                            let v16 = u16::from_ne_bytes([buf[si + ch * 2], buf[si + ch * 2 + 1]]);
                            flat[di + ch] = ((v16 as u32 * 255 + 16384) / 32768).min(255) as u8;
                        }
                    }
                }
            }
        }
        32 => {
            for y in 0..h {
                let src_row = row_offset(y,h,stride,reverse);
                let dst_row = y * w * 4;
                for x in 0..w {
                    let si = src_row + x * 16;
                    let di = dst_row + x * 4;
                    if si + 15 < buf.len() && di + 3 < flat.len() {
                        for ch in 0..4usize {
                            let v = f32::from_ne_bytes([
                                buf[si + ch * 4], buf[si + ch * 4 + 1],
                                buf[si + ch * 4 + 2], buf[si + ch * 4 + 3],
                            ]);
                            flat[di + ch] = (v * 255.0).clamp(0.0, 255.0) as u8;
                        }
                    }
                }
            }
        }
        _ => {
            for y in 0..h {
                let src_off = row_offset(y,h,stride,reverse);
                let dst_off = y * w * 4;
                let row_len = w * 4;
                if src_off + row_len <= buf.len() && dst_off + row_len <= flat.len() {
                    flat[dst_off..dst_off + row_len].copy_from_slice(&buf[src_off..src_off + row_len]);
                }
            }
        }
    }
    Ok((flat, w, h))
}

/// Read in_layer into a flat buffer matching out_layer dimensions.
/// In non-smart Render, in/out share the same w/h, so this is a direct conversion.
fn layer_to_flat_padded(layer: &ae::Layer, out_w: usize, out_h: usize) -> Result<Vec<u8>,ae::Error> {
    let (flat, w, h) = layer_to_flat(layer)?;
    if w == out_w && h == out_h {
        return Ok(flat);
    }
    // Different size: blit what fits
    let mut canvas = allocate_argb8(out_w,out_h)?;
    let copy_w = w.min(out_w);
    let copy_h = h.min(out_h);
    for y in 0..copy_h {
        let src_off = y * w * 4;
        let dst_off = y * out_w * 4;
        let row = copy_w * 4;
        if src_off + row <= flat.len() && dst_off + row <= canvas.len() {
            canvas[dst_off..dst_off + row].copy_from_slice(&flat[src_off..src_off + row]);
        }
    }
    Ok(canvas)
}

fn flat_to_layer(flat: &[u8], layer: &mut ae::Layer, w: usize, h: usize) -> Result<(),ae::Error> {
    let depth = validate_world(layer)?;
    let len=w.checked_mul(h).and_then(|n|n.checked_mul(4)).ok_or(ae::Error::OutOfMemory)?;
    if (w,h)!=(layer.width(),layer.height()) || flat.len()!=len {return Err(ae::Error::BadCallbackParameter);}
    if w==0 || h==0 {return Ok(());}
    let stride = layer.buffer_stride();
    let reverse = layer.row_bytes()<0;
    let buf = layer.buffer_mut();
    match depth {
        16 => {
            for y in 0..h {
                let src_row = y * w * 4;
                let dst_row = row_offset(y,h,stride,reverse);
                for x in 0..w {
                    let si = src_row + x * 4;
                    let di = dst_row + x * 8;
                    if si + 3 < flat.len() && di + 7 < buf.len() {
                        for ch in 0..4usize {
                            let v8 = flat[si + ch] as u16;
                            let v16 = ((v8 as u32 * 32768 + 127) / 255) as u16;
                            let bytes = v16.to_ne_bytes();
                            buf[di + ch * 2] = bytes[0];
                            buf[di + ch * 2 + 1] = bytes[1];
                        }
                    }
                }
            }
        }
        32 => {
            for y in 0..h {
                let src_row = y * w * 4;
                let dst_row = row_offset(y,h,stride,reverse);
                for x in 0..w {
                    let si = src_row + x * 4;
                    let di = dst_row + x * 16;
                    if si + 3 < flat.len() && di + 15 < buf.len() {
                        for ch in 0..4usize {
                            let v = flat[si + ch] as f32 / 255.0;
                            let bytes = v.to_ne_bytes();
                            buf[di + ch * 4] = bytes[0];
                            buf[di + ch * 4 + 1] = bytes[1];
                            buf[di + ch * 4 + 2] = bytes[2];
                            buf[di + ch * 4 + 3] = bytes[3];
                        }
                    }
                }
            }
        }
        _ => {
            for y in 0..h {
                let src_off = y * w * 4;
                let dst_off = row_offset(y,h,stride,reverse);
                let row_len = w * 4;
                if src_off + row_len <= flat.len() && dst_off + row_len <= buf.len() {
                    buf[dst_off..dst_off + row_len].copy_from_slice(&flat[src_off..src_off + row_len]);
                }
            }
        }
    }
    Ok(())
}
