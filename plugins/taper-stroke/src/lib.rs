use after_effects as ae;
#[cfg(feature = "coordinate-trace")]
mod coordinate_trace;
pub mod core;
use core::{Outline, Point, Style};

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Path,
    Width,
    TaperStart,
    EndWidth,
    TaperEase,
    Cap,
    Color,
    Opacity,
    TrimStart,
    TrimEnd,
}
#[derive(Default)]
struct Plugin;
struct Frame {
    outline: Outline,
    color: [f32; 3],
    opacity: f32,
}
ae::define_effect!(Plugin, (), Params);

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        p: &mut ae::Parameters<Params>,
        _: ae::InData,
        _: ae::OutData,
    ) -> Result<(), ae::Error> {
        p.add(
            Params::Path,
            "Path",
            ae::PathDef::setup(|v| {
                v.set_default(1);
            }),
        )?;
        for (id, name, max, slider, default) in [
            (Params::Width, "Width", 2000.0, 200.0, 30.0),
            (Params::TaperStart, "Taper Start (%)", 100.0, 100.0, 0.0),
            (Params::EndWidth, "End Width (%)", 100.0, 100.0, 0.0),
            (Params::TaperEase, "Taper Ease", 10.0, 5.0, 1.0),
        ] {
            p.add(
                id,
                name,
                ae::FloatSliderDef::setup(|v| {
                    v.set_valid_min(if id == Params::TaperEase { 0.1 } else { 0.0 });
                    v.set_valid_max(max);
                    v.set_slider_min(if id == Params::TaperEase { 0.1 } else { 0.0 });
                    v.set_slider_max(slider);
                    v.set_default(default);
                    v.set_precision(2);
                }),
            )?;
        }
        p.add(
            Params::Cap,
            "Cap",
            ae::PopupDef::setup(|v| {
                v.set_options(&["Round", "Flat"]);
                v.set_default(1);
            }),
        )?;
        p.add(
            Params::Color,
            "Color",
            ae::ColorDef::setup(|v| {
                v.set_default(ae::Pixel8 {
                    alpha: 255,
                    red: 255,
                    green: 255,
                    blue: 255,
                });
            }),
        )?;
        p.add(
            Params::Opacity,
            "Opacity (%)",
            ae::FloatSliderDef::setup(|v| {
                v.set_valid_min(0.0);
                v.set_valid_max(100.0);
                v.set_slider_min(0.0);
                v.set_slider_max(100.0);
                v.set_default(100.0);
                v.set_precision(1);
            }),
        )?;
        // Append slots: preserve saved parameter IDs 1 through 8.
        for (id, name, value) in [
            (Params::TrimStart, "Trim Start (%)", 0.0),
            (Params::TrimEnd, "Trim End (%)", 100.0),
        ] {
            p.add(
                id,
                name,
                ae::FloatSliderDef::setup(|v| {
                    v.set_valid_min(0.0);
                    v.set_valid_max(100.0);
                    v.set_slider_min(0.0);
                    v.set_slider_max(100.0);
                    v.set_default(value);
                    v.set_precision(2);
                }),
            )?;
        }
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
            ae::Command::About => o.set_return_msg(
                "ONMK TaperStroke 0.3.1\rVariable-width stroke from an open layer mask.",
            ),
            ae::Command::GlobalSetup => {
                o.set_out_flag(ae::OutFlags::DeepColorAware, true);
                o.set_out_flag(ae::OutFlags::NonParamVary, true);
                o.set_out_flag2(ae::OutFlags2::FloatColorAware, true);
                o.set_out_flag2(ae::OutFlags2::DependsOnUnreferencedMasks, true);
                o.set_out_flag2(ae::OutFlags2::RevealsZeroAlpha, true);
            }
            ae::Command::Render {
                out_layer: mut output,
                ..
            } => {
                #[cfg(feature = "coordinate-trace")]
                coordinate_trace::record(format_args!("selector read_frame"));
                let frame = read_frame(&i, p)?;
                render(&frame, &i, &mut output)?;
            }
            ae::Command::SmartPreRender { mut extra } => {
                #[cfg(feature = "coordinate-trace")]
                coordinate_trace::record(format_args!("selector read_frame"));
                let frame = read_frame(&i, p)?;
                // A generator must advertise its full layer, even with a transparent input.
                let full = ae::Rect {
                    left: 0,
                    top: 0,
                    right: (i.width() as f64 * f64::from(i.downsample_x())).ceil() as i32,
                    bottom: (i.height() as f64 * f64::from(i.downsample_y())).ceil() as i32,
                };
                let req: ae::Rect = extra.output_request().rect.into();
                #[cfg(feature = "coordinate-trace")]
                coordinate_trace::record(format_args!(
                    "pre ds=({}, {}) full={:?} request={:?}",
                    f64::from(i.downsample_x()),
                    f64::from(i.downsample_y()),
                    full,
                    req
                ));
                let result = ae::Rect {
                    left: full.left.max(req.left),
                    top: full.top.max(req.top),
                    right: full.right.min(req.right),
                    bottom: full.bottom.min(req.bottom),
                };
                extra.set_result_rect(
                    if result.right > result.left && result.bottom > result.top {
                        result
                    } else {
                        ae::Rect {
                            left: 0,
                            top: 0,
                            right: 0,
                            bottom: 0,
                        }
                    },
                );
                extra.set_max_result_rect(full);
                extra.set_pre_render_data(frame);
            }
            ae::Command::SmartRender { extra } => {
                let frame = extra
                    .pre_render_data::<Frame>()
                    .ok_or(ae::Error::InternalStructDamaged)?;
                let mut output = extra
                    .callbacks()
                    .checkout_output()?
                    .ok_or(ae::Error::BadCallbackParameter)?;
                render(frame, &i, &mut output)?;
            }
            _ => {}
        }
        Ok(())
    }
}

fn read_frame(i: &ae::InData, p: &ae::Parameters<Params>) -> Result<Frame, ae::Error> {
    let get = |id| -> Result<f64, ae::Error> {
        let v = p.get(id)?.as_float_slider()?.value();
        if v.is_finite() {
            Ok(v)
        } else {
            Err(ae::Error::BadCallbackParameter)
        }
    };
    let style = Style {
        width: get(Params::Width)?.clamp(0.0, 2000.0),
        start: get(Params::TaperStart)?.clamp(0.0, 100.0) / 100.0,
        end: get(Params::EndWidth)?.clamp(0.0, 100.0) / 100.0,
        ease: get(Params::TaperEase)?,
        round: p.get(Params::Cap)?.as_popup()?.value() == 1,
    };
    let c = p.get(Params::Color)?.as_color()?.value();
    let mut frame = Frame {
        outline: Outline::default(),
        color: [
            c.red as f32 / 255.0,
            c.green as f32 / 255.0,
            c.blue as f32 / 255.0,
        ],
        opacity: (get(Params::Opacity)? / 100.0).clamp(0.0, 1.0) as f32,
    };
    let trim_start = get(Params::TrimStart)?.clamp(0.0, 100.0) / 100.0;
    let trim_end = get(Params::TrimEnd)?.clamp(0.0, 100.0) / 100.0;
    let id = p.get(Params::Path)?.as_path()?.path_id();
    if id == 0 || style.width == 0.0 || frame.opacity == 0.0 || trim_start >= trim_end {
        return Ok(frame);
    }
    let query = ae::pf::suites::PathQuery::new()?;
    let Some(path) = query.checkout_path(
        i.effect_ref(),
        id,
        i.current_time(),
        i.time_step(),
        i.time_scale(),
    )?
    else {
        return Ok(frame);
    };
    if !path.is_open()? {
        return Ok(frame);
    }
    let count = path.num_segments()?;
    if !(0..=10000).contains(&count) {
        return Err(ae::Error::BadCallbackParameter);
    }
    let mut points = Vec::new();
    // Host arc-length evaluation preserves Bezier handles without assuming tangent encoding.
    for segment in 0..count {
        let mut prep = path.prepare_seg_length(segment, 100)?;
        let length = prep.length()?;
        if !length.is_finite() || length < 0.0 {
            return Err(ae::Error::BadCallbackParameter);
        }
        if length <= 1e-8 {
            continue;
        }
        let steps = (length / 0.5).ceil().max(1.0) as usize;
        if steps > 1_000_000 || points.len().saturating_add(steps) > 1_000_000 {
            return Err(ae::Error::OutOfMemory);
        }
        for j in 0..=steps {
            if !points.is_empty() && j == 0 {
                continue;
            }
            let (x, y) = prep.eval(length * j as f64 / steps as f64)?;
            if !x.is_finite() || !y.is_finite() {
                return Err(ae::Error::BadCallbackParameter);
            }
            points.push(Point::new(x, y));
        }
    }
    #[cfg(feature = "coordinate-trace")]
    coordinate_trace::record(format_args!(
        "path time={}/{} ds=({}, {}) layer=({}, {}) input_origin=({}, {}) first_vertex={:?} last_vertex={:?} evaluated_first={:?} evaluated_last={:?} width={}",
        i.current_time(), i.time_scale(), f64::from(i.downsample_x()), f64::from(i.downsample_y()),
        i.width(), i.height(), i.as_ref().output_origin_x, i.as_ref().output_origin_y,
        path.vertex(0).map(|v| (v.x, v.y)), path.vertex(count).map(|v| (v.x, v.y)),
        points.first(), points.last(), style.width,
    ));
    frame.outline = Outline::new_trimmed(
        &points,
        style,
        i.downsample_x().into(),
        i.downsample_y().into(),
        trim_start,
        trim_end,
    );
    Ok(frame)
}

fn render(frame: &Frame, _i: &ae::InData, output: &mut ae::Layer) -> Result<(), ae::Error> {
    let format = output.pixel_format()?;
    if !matches!(
        format,
        ae::PixelFormat::Argb32 | ae::PixelFormat::Argb64 | ae::PixelFormat::Argb128
    ) {
        return Err(ae::Error::BadCallbackParameter);
    }
    let (w, h) = (output.width(), output.height());
    if w == 0 || h == 0 {
        return Ok(());
    }
    let bytes = match format {
        ae::PixelFormat::Argb32 => 4,
        ae::PixelFormat::Argb64 => 8,
        _ => 16,
    };
    if w > i32::MAX as usize
        || h > i32::MAX as usize
        // The accessor only returns the host pointer; do not dereference it here.
        || unsafe { output.data_ptr() }.is_null()
        || w.checked_mul(bytes)
            .is_none_or(|n| n > output.row_bytes().unsigned_abs())
    {
        return Err(ae::Error::BadCallbackParameter);
    }
    let origin = output.origin();
    #[cfg(feature = "coordinate-trace")]
    coordinate_trace::record(format_args!(
        "output size=({}, {}) origin=({}, {}) format={:?} render_ds=({}, {}) input_origin=({}, {})",
        w,
        h,
        origin.h,
        origin.v,
        format,
        f64::from(_i.downsample_x()),
        f64::from(_i.downsample_y()),
        _i.as_ref().output_origin_x,
        _i.as_ref().output_origin_y
    ));
    let mut row = vec![0.0; w];
    for y in 0..h {
        frame
            .outline
            .row(y as f64 + origin.v as f64, origin.h as f64, &mut row);
        for (x, &coverage) in row.iter().enumerate() {
            let a = coverage.clamp(0.0, 1.0) * frame.opacity;
            // AE effect pixels use straight RGB. Coverage/opacity belong only
            // in alpha; premultiplying here causes a second multiply at composite.
            // Retain the stroke color even at zero coverage for edge resampling.
            let [r, g, b] = frame.color;
            match format {
                ae::PixelFormat::Argb32 => {
                    *output.as_pixel8_mut(x, y) = ae::Pixel8 {
                        alpha: (a * 255.0).round() as u8,
                        red: (r * 255.0).round() as u8,
                        green: (g * 255.0).round() as u8,
                        blue: (b * 255.0).round() as u8,
                    }
                }
                ae::PixelFormat::Argb64 => {
                    *output.as_pixel16_mut(x, y) = ae::Pixel16 {
                        alpha: (a * 32768.0).round() as u16,
                        red: (r * 32768.0).round() as u16,
                        green: (g * 32768.0).round() as u16,
                        blue: (b * 32768.0).round() as u16,
                    }
                }
                ae::PixelFormat::Argb128 => {
                    *output.as_pixel32_mut(x, y) = ae::PixelF32 {
                        alpha: a,
                        red: r,
                        green: g,
                        blue: b,
                    }
                }
                _ => unreachable!(),
            }
        }
    }
    Ok(())
}
