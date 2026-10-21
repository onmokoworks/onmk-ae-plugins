//! ONMK FocusLines: manga/anime concentration lines for After Effects.
//!
//! The AE layer here only converts host parameters, time and coordinates into
//! [`layout::Settings`], [`layout::Mapping`] and a frame index (once, explicitly),
//! then writes straight ARGB through [`pixel`].

use after_effects as ae;

pub mod backend;
pub mod gpu;
pub mod hash;
pub mod layout;
pub mod pixel;
pub mod raster;

use backend::Backend;
use layout::{Composite, Evolution, Mapping, Settings};
use pixel::Shading;
use raster::PixelRect;
use std::sync::Mutex;

const INPUT_ID: u32 = 1;
const VERSION: &str = "0.1.2";

/// Variant names are hashed into AE parameter IDs by the pinned crate: never
/// rename or reorder; append new variants (and groups) at the end only.
#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    GroupFocusStart,
    FocusPoint,
    CenterSize,
    CenterAspect,
    CenterRotation,
    CenterOffsetX,
    CenterOffsetY,
    GroupFocusEnd,
    GroupLinesStart,
    LineCount,
    LineWidth,
    WidthRandom,
    TipWidth,
    TaperLength,
    TaperShape,
    LengthRandom,
    GroupLinesEnd,
    GroupDistributionStart,
    Seed,
    AngleJitter,
    Clustering,
    ClusterCount,
    ClusterSpread,
    DensityVariation,
    GapAngle,
    GapWidth,
    GapSoftness,
    GroupDistributionEnd,
    GroupTimeStart,
    Evolution,
    HoldFrames,
    ChangeAmount,
    Stagger,
    GroupTimeEnd,
    GroupColorStart,
    Color,
    Intensity,
    Opacity,
    Composite,
    GroupColorEnd,
    GroupRenderStart,
    Backend,
    Quality,
    GroupRenderEnd,
    TipCap,
    GroupShortStart,
    ShortMin,
    ShortMax,
    ShortMix,
    GroupShortEnd,
}

#[derive(Default)]
struct Plugin;

#[derive(Clone, Copy, Debug)]
struct Frame {
    settings: Settings,
    mapping: Mapping,
    frame: i64,
    backend: Backend,
}

ae::define_effect!(Plugin, (), Params);

static LAST_ERROR: Mutex<Option<String>> = Mutex::new(None);

fn record_error(message: String) {
    if let Ok(mut last) = LAST_ERROR.lock() {
        *last = Some(message);
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

fn popup(options: &'static [&'static str], default: i32) -> ae::PopupDef<'static> {
    ae::PopupDef::setup(|f| {
        f.set_options(options);
        f.set_default(default);
    })
}

fn fixed() -> (ae::ParamFlag, ae::ParamUIFlags) {
    (ae::ParamFlag::CANNOT_TIME_VARY, ae::ParamUIFlags::empty())
}

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        p: &mut ae::Parameters<Params>,
        _: ae::InData,
        _: ae::OutData,
    ) -> Result<(), ae::Error> {
        p.add_group(
            Params::GroupFocusStart,
            Params::GroupFocusEnd,
            "Focus",
            false,
            |p| {
                p.add(
                    Params::FocusPoint,
                    "Focus Point",
                    ae::PointDef::setup(|f| {
                        f.set_default((50.0, 50.0));
                        f.set_restrict_bounds(false);
                    }),
                )?;
                p.add(
                    Params::CenterSize,
                    "Center Size (% of half height)",
                    fs(0.0, 1000.0, 0.0, 150.0, 45.0, 1),
                )?;
                p.add(
                    Params::CenterAspect,
                    "Center Aspect",
                    fs(0.1, 10.0, 0.25, 4.0, 1.6, 2),
                )?;
                p.add(
                    Params::CenterRotation,
                    "Center Rotation",
                    ae::AngleDef::setup(|f| {
                        f.set_default(0.0);
                    }),
                )?;
                p.add(
                    Params::CenterOffsetX,
                    "Center Offset X (%)",
                    fs(-100.0, 100.0, -95.0, 95.0, 0.0, 1),
                )?;
                p.add(
                    Params::CenterOffsetY,
                    "Center Offset Y (%)",
                    fs(-100.0, 100.0, -95.0, 95.0, 0.0, 1),
                )?;
                Ok(())
            },
        )?;
        p.add_group(
            Params::GroupLinesStart,
            Params::GroupLinesEnd,
            "Lines",
            false,
            |p| {
                p.add(
                    Params::LineCount,
                    "Line Count",
                    fs(0.0, 5000.0, 0.0, 1000.0, 220.0, 1),
                )?;
                p.add(
                    Params::LineWidth,
                    "Line Width (px)",
                    fs(0.0, 500.0, 0.0, 60.0, 9.0, 2),
                )?;
                p.add(
                    Params::WidthRandom,
                    "Width Randomness (%)",
                    fs(0.0, 100.0, 0.0, 100.0, 45.0, 1),
                )?;
                p.add(
                    Params::TipWidth,
                    "Tip Width (%)",
                    fs(0.0, 100.0, 0.0, 100.0, 0.0, 1),
                )?;
                p.add(
                    Params::TaperLength,
                    "Taper Length (% of half height)",
                    fs(0.0, 1000.0, 0.0, 200.0, 70.0, 1),
                )?;
                p.add(
                    Params::TaperShape,
                    "Taper Shape",
                    fs(0.25, 4.0, 0.25, 4.0, 1.0, 2),
                )?;
                p.add(
                    Params::LengthRandom,
                    "Length Randomness (%)",
                    fs(0.0, 400.0, 0.0, 150.0, 35.0, 1),
                )?;
                Ok(())
            },
        )?;
        p.add_group(
            Params::GroupDistributionStart,
            Params::GroupDistributionEnd,
            "Distribution",
            false,
            |p| {
                let (flag, ui) = fixed();
                p.add_with_flags(
                    Params::Seed,
                    "Random Seed",
                    ae::SliderDef::setup(|f| {
                        f.set_valid_min(0);
                        f.set_valid_max(30000);
                        f.set_slider_min(0);
                        f.set_slider_max(1000);
                        f.set_default(1);
                    }),
                    flag,
                    ui,
                )?;
                p.add(
                    Params::AngleJitter,
                    "Angle Jitter (deg)",
                    fs(0.0, 30.0, 0.0, 5.0, 0.6, 2),
                )?;
                p.add(
                    Params::Clustering,
                    "Clustering (%)",
                    fs(0.0, 100.0, 0.0, 100.0, 15.0, 1),
                )?;
                let (flag, ui) = fixed();
                p.add_with_flags(
                    Params::ClusterCount,
                    "Cluster Count",
                    ae::SliderDef::setup(|f| {
                        f.set_valid_min(1);
                        f.set_valid_max(64);
                        f.set_slider_min(1);
                        f.set_slider_max(64);
                        f.set_default(12);
                    }),
                    flag,
                    ui,
                )?;
                p.add(
                    Params::ClusterSpread,
                    "Cluster Spread (deg)",
                    fs(0.0, 90.0, 0.0, 30.0, 4.0, 1),
                )?;
                p.add(
                    Params::DensityVariation,
                    "Density Variation (%)",
                    fs(0.0, 100.0, 0.0, 100.0, 25.0, 1),
                )?;
                p.add(
                    Params::GapAngle,
                    "Gap Direction",
                    ae::AngleDef::setup(|f| {
                        f.set_default(180.0);
                    }),
                )?;
                p.add(
                    Params::GapWidth,
                    "Gap Width (deg)",
                    fs(0.0, 360.0, 0.0, 360.0, 0.0, 1),
                )?;
                p.add(
                    Params::GapSoftness,
                    "Gap Softness (deg)",
                    fs(0.0, 180.0, 0.0, 90.0, 20.0, 1),
                )?;
                Ok(())
            },
        )?;
        p.add_group(
            Params::GroupTimeStart,
            Params::GroupTimeEnd,
            "Evolution",
            false,
            |p| {
                let (flag, ui) = fixed();
                p.add_with_flags(
                    Params::Evolution,
                    "Evolution",
                    popup(&["Static", "Hold Frames"], 1),
                    flag,
                    ui,
                )?;
                let (flag, ui) = fixed();
                p.add_with_flags(
                    Params::HoldFrames,
                    "Hold Frames",
                    ae::SliderDef::setup(|f| {
                        f.set_valid_min(1);
                        f.set_valid_max(10000);
                        f.set_slider_min(1);
                        f.set_slider_max(12);
                        f.set_default(2);
                    }),
                    flag,
                    ui,
                )?;
                p.add(
                    Params::ChangeAmount,
                    "Change Amount (%)",
                    fs(0.0, 100.0, 0.0, 100.0, 100.0, 1),
                )?;
                p.add(
                    Params::Stagger,
                    "Stagger (%)",
                    fs(0.0, 100.0, 0.0, 100.0, 0.0, 1),
                )?;
                Ok(())
            },
        )?;
        p.add_group(
            Params::GroupColorStart,
            Params::GroupColorEnd,
            "Color & Composite",
            false,
            |p| {
                p.add(
                    Params::Color,
                    "Color",
                    ae::ColorDef::setup(|f| {
                        f.set_default(ae::Pixel8 {
                            alpha: 255,
                            red: 0,
                            green: 0,
                            blue: 0,
                        });
                    }),
                )?;
                p.add(
                    Params::Intensity,
                    "Intensity (32 bpc HDR)",
                    fs(0.0, 100.0, 0.0, 4.0, 1.0, 2),
                )?;
                p.add(
                    Params::Opacity,
                    "Opacity (%)",
                    fs(0.0, 100.0, 0.0, 100.0, 100.0, 1),
                )?;
                p.add(
                    Params::Composite,
                    "Composite",
                    popup(&["Lines Only", "Over Source"], 1),
                )?;
                Ok(())
            },
        )?;
        p.add_group(
            Params::GroupRenderStart,
            Params::GroupRenderEnd,
            "Render",
            false,
            |p| {
                let (flag, ui) = fixed();
                p.add_with_flags(
                    Params::Backend,
                    "Backend",
                    popup(
                        &[
                            "Auto (GPU, CPU fallback)",
                            "GPU (private wgpu device)",
                            "CPU",
                        ],
                        1,
                    ),
                    flag,
                    ui,
                )?;
                let (flag, ui) = fixed();
                p.add_with_flags(
                    Params::Quality,
                    "Antialiasing",
                    popup(&["Standard (8 scanlines)", "High (16 scanlines)"], 1),
                    flag,
                    ui,
                )?;
                Ok(())
            },
        )?;
        p.add(Params::TipCap, "Tip Cap", popup(&["Flat", "Round"], 1))?;
        p.add_group(
            Params::GroupShortStart,
            Params::GroupShortEnd,
            "Short Lines",
            false,
            |p| {
                p.add(
                    Params::ShortMin,
                    "Minimum Length (px)",
                    fs(0.0, 10000.0, 0.0, 500.0, 30.0, 1),
                )?;
                p.add(
                    Params::ShortMax,
                    "Maximum Length (px)",
                    fs(0.0, 10000.0, 0.0, 500.0, 120.0, 1),
                )?;
                p.add(
                    Params::ShortMix,
                    "Mix (%)",
                    fs(0.0, 100.0, 0.0, 100.0, 0.0, 1),
                )?;
                Ok(())
            },
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
            ae::Command::About => {
                let gpu = match gpu::initialized_info() {
                    None => "GPU: not initialized in this session".to_string(),
                    Some(Ok(info)) => format!("GPU: {info}"),
                    Some(Err(e)) => format!("GPU unavailable: {e}"),
                };
                let fallback = backend::last_fallback()
                    .map(|r| format!("\rLast Auto CPU fallback: {r}"))
                    .unwrap_or_default();
                let error = LAST_ERROR
                    .lock()
                    .ok()
                    .and_then(|e| e.clone())
                    .map(|e| format!("\rLast render error: {e}"))
                    .unwrap_or_default();
                o.set_return_msg(&format!(
                    "ONMK FocusLines {VERSION}\rConcentration lines generator.\r{gpu}{fallback}{error}"
                ));
            }
            ae::Command::GlobalSetup => {
                o.set_out_flag(ae::OutFlags::DeepColorAware, true);
                o.set_out_flag(ae::OutFlags::NonParamVary, true);
                o.set_out_flag2(ae::OutFlags2::FloatColorAware, true);
                o.set_out_flag2(ae::OutFlags2::RevealsZeroAlpha, true);
            }
            ae::Command::Render {
                in_layer,
                out_layer: mut output,
            } => {
                let frame = read_frame(&i, p)?;
                let input =
                    (frame.settings.composite == Composite::OverSource).then_some(&in_layer);
                render(&frame, &mut output, input)?;
            }
            ae::Command::SmartPreRender { mut extra } => {
                let frame = read_frame(&i, p)?;
                // Generator: advertise the whole (downsampled) layer.
                let full = ae::Rect {
                    left: 0,
                    top: 0,
                    right: (i.width() as f64 * frame.mapping.dsx).ceil() as i32,
                    bottom: (i.height() as f64 * frame.mapping.dsy).ceil() as i32,
                };
                let req: ae::Rect = extra.output_request().rect.into();
                let lines = intersect(full, req);
                let (result, max) = if frame.settings.composite == Composite::OverSource {
                    let request = extra.output_request();
                    let got = extra.callbacks().checkout_layer(
                        0,
                        INPUT_ID as i32,
                        &request,
                        i.current_time(),
                        i.time_step(),
                        i.time_scale(),
                    )?;
                    let in_result: ae::Rect = got.result_rect.into();
                    let in_max: ae::Rect = got.max_result_rect.into();
                    (union(lines, intersect(in_result, req)), union(full, in_max))
                } else {
                    (lines, full)
                };
                extra.set_result_rect(result);
                extra.set_max_result_rect(max);
                extra.set_pre_render_data(frame);
            }
            ae::Command::SmartRender { extra } => {
                let frame = *extra
                    .pre_render_data::<Frame>()
                    .ok_or(ae::Error::InternalStructDamaged)?;
                let cb = extra.callbacks();
                if frame.settings.composite == Composite::OverSource {
                    let input = cb.checkout_layer_pixels(INPUT_ID)?;
                    with_input_checkin(
                        || {
                            let mut output = cb
                                .checkout_output()?
                                .ok_or(ae::Error::BadCallbackParameter)?;
                            render(&frame, &mut output, input.as_ref())
                        },
                        || cb.checkin_layer_pixels(INPUT_ID),
                    )?;
                } else {
                    let mut output = cb
                        .checkout_output()?
                        .ok_or(ae::Error::BadCallbackParameter)?;
                    render(&frame, &mut output, None)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}

fn is_empty(r: &ae::Rect) -> bool {
    r.right <= r.left || r.bottom <= r.top
}

fn empty_rect() -> ae::Rect {
    ae::Rect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    }
}

fn intersect(a: ae::Rect, b: ae::Rect) -> ae::Rect {
    let r = ae::Rect {
        left: a.left.max(b.left),
        top: a.top.max(b.top),
        right: a.right.min(b.right),
        bottom: a.bottom.min(b.bottom),
    };
    if is_empty(&r) {
        empty_rect()
    } else {
        r
    }
}

fn union(a: ae::Rect, b: ae::Rect) -> ae::Rect {
    match (is_empty(&a), is_empty(&b)) {
        (true, true) => empty_rect(),
        (true, false) => b,
        (false, true) => a,
        (false, false) => ae::Rect {
            left: a.left.min(b.left),
            top: a.top.min(b.top),
            right: a.right.max(b.right),
            bottom: a.bottom.max(b.bottom),
        },
    }
}

/// Always check the input back in; keep the primary render failure.
fn with_input_checkin<T>(
    work: impl FnOnce() -> Result<T, ae::Error>,
    checkin: impl FnOnce() -> Result<(), ae::Error>,
) -> Result<T, ae::Error> {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work));
    let cleanup = checkin();
    match result.unwrap_or(Err(ae::Error::InternalStructDamaged)) {
        Err(error) => Err(error),
        Ok(value) => cleanup.map(|()| value),
    }
}

/// The single place where host time, coordinates and parameters are interpreted.
fn read_frame(i: &ae::InData, p: &ae::Parameters<Params>) -> Result<Frame, ae::Error> {
    let get = |id| -> Result<f64, ae::Error> {
        let v = p.get(id)?.as_float_slider()?.value();
        if v.is_finite() {
            Ok(v)
        } else {
            Err(ae::Error::BadCallbackParameter)
        }
    };
    let angle = |id| -> Result<f64, ae::Error> {
        let v = p.get(id)?.as_angle()?.value() as f64;
        if v.is_finite() {
            Ok(v)
        } else {
            Err(ae::Error::BadCallbackParameter)
        }
    };
    let popup = |id| -> Result<i32, ae::Error> { Ok(p.get(id)?.as_popup()?.value()) };
    let slider = |id| -> Result<i32, ae::Error> { Ok(p.get(id)?.as_slider()?.value()) };
    // in_data width/height are taken as full-resolution layer size (same
    // assumption as TaperStroke's SmartPreRender; unconfirmed in real AE).
    let mapping = Mapping::new(
        i.width() as f64,
        i.height() as f64,
        f64::from(i.pixel_aspect_ratio()),
        f64::from(i.downsample_x()),
        f64::from(i.downsample_y()),
    );
    // Point values arrive downsample-adjusted (crate/SDK doc); undo once here.
    // Not yet confirmed in real AE (see docs/design-gpu.md section 4).
    let (px, py) = p.get(Params::FocusPoint)?.as_point()?.value();
    let focus = [px as f64 / mapping.dsx, py as f64 / mapping.dsy];
    let c = p.get(Params::Color)?.as_color()?.value();
    let settings = Settings {
        focus: Some(focus),
        center_size: get(Params::CenterSize)?,
        center_aspect: get(Params::CenterAspect)?,
        center_rotation: angle(Params::CenterRotation)?,
        center_offset: [get(Params::CenterOffsetX)?, get(Params::CenterOffsetY)?],
        line_count: get(Params::LineCount)?,
        line_width: get(Params::LineWidth)?,
        width_random: get(Params::WidthRandom)?,
        tip_width: get(Params::TipWidth)?,
        round_tip: popup(Params::TipCap)? == 2,
        taper_length: get(Params::TaperLength)?,
        taper_shape: get(Params::TaperShape)?,
        length_random: get(Params::LengthRandom)?,
        short_min: get(Params::ShortMin)?,
        short_max: get(Params::ShortMax)?,
        short_mix: get(Params::ShortMix)?,
        seed: slider(Params::Seed)?.max(0) as u32,
        angle_jitter: get(Params::AngleJitter)?,
        clustering: get(Params::Clustering)?,
        cluster_count: slider(Params::ClusterCount)?.clamp(1, 64) as u32,
        cluster_spread: get(Params::ClusterSpread)?,
        density_variation: get(Params::DensityVariation)?,
        gap_angle: angle(Params::GapAngle)?,
        gap_width: get(Params::GapWidth)?,
        gap_softness: get(Params::GapSoftness)?,
        evolution: if popup(Params::Evolution)? == 2 {
            Evolution::HoldFrames
        } else {
            Evolution::Static
        },
        hold_frames: slider(Params::HoldFrames)?.max(1) as u32,
        change_amount: get(Params::ChangeAmount)?,
        stagger: get(Params::Stagger)?,
        color: [
            f32::from(c.red) / 255.0,
            f32::from(c.green) / 255.0,
            f32::from(c.blue) / 255.0,
        ],
        intensity: get(Params::Intensity)? as f32,
        opacity: get(Params::Opacity)? as f32,
        composite: if popup(Params::Composite)? == 2 {
            Composite::OverSource
        } else {
            Composite::LinesOnly
        },
        samples: if popup(Params::Quality)? == 2 { 16 } else { 8 },
    }
    .sanitized();
    let backend = match popup(Params::Backend)? {
        2 => Backend::Gpu,
        3 => Backend::Cpu,
        _ => Backend::Auto,
    };
    Ok(Frame {
        settings,
        mapping,
        frame: layout::frame_index(i64::from(i.current_time()), i64::from(i.time_step())),
        backend,
    })
}

#[derive(Clone, Copy)]
enum Raw {
    P8([u8; 4]),
    P16([u16; 4]),
    P32([f32; 4]),
}

fn bytes_per_pixel(format: ae::PixelFormat) -> Result<usize, ae::Error> {
    match format {
        ae::PixelFormat::Argb32 => Ok(4),
        ae::PixelFormat::Argb64 => Ok(8),
        ae::PixelFormat::Argb128 => Ok(16),
        _ => Err(ae::Error::BadCallbackParameter),
    }
}

/// Straight ARGB source pixel at absolute buffer coordinates (None = outside).
struct Source<'a> {
    data: &'a [u8],
    w: usize,
    h: usize,
    stride: usize,
    bpp: usize,
    bottom_up: bool,
    ox: i64,
    oy: i64,
}

impl Source<'_> {
    fn get(&self, ax: i64, ay: i64) -> Option<Raw> {
        let x = ax - self.ox;
        let y = ay - self.oy;
        if x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h {
            return None;
        }
        let row = if self.bottom_up {
            self.h - 1 - y as usize
        } else {
            y as usize
        };
        let o = row * self.stride + x as usize * self.bpp;
        let b = self.data.get(o..o + self.bpp)?;
        Some(match self.bpp {
            4 => Raw::P8([b[0], b[1], b[2], b[3]]),
            8 => Raw::P16([0, 1, 2, 3].map(|c| u16::from_le_bytes([b[2 * c], b[2 * c + 1]]))),
            _ => {
                Raw::P32([0, 1, 2, 3].map(|c| {
                    f32::from_le_bytes([b[4 * c], b[4 * c + 1], b[4 * c + 2], b[4 * c + 3]])
                }))
            }
        })
    }
}

fn source<'a>(
    layer: &'a ae::Layer,
    format: ae::PixelFormat,
) -> Result<Option<Source<'a>>, ae::Error> {
    if layer.pixel_format()? != format {
        return Err(ae::Error::BadCallbackParameter);
    }
    let (w, h) = (layer.width(), layer.height());
    if w == 0 || h == 0 {
        return Ok(None);
    }
    let bpp = bytes_per_pixel(format)?;
    let stride = layer.row_bytes().unsigned_abs();
    if unsafe { layer.data_ptr() }.is_null() || w.checked_mul(bpp).is_none_or(|n| n > stride) {
        return Err(ae::Error::BadCallbackParameter);
    }
    let data = layer.buffer();
    if data.len() < stride.saturating_mul(h - 1) + w * bpp {
        return Err(ae::Error::BadCallbackParameter);
    }
    let origin = layer.origin();
    Ok(Some(Source {
        data,
        w,
        h,
        stride,
        bpp,
        bottom_up: layer.row_bytes() < 0,
        ox: i64::from(origin.h),
        oy: i64::from(origin.v),
    }))
}

fn decode(raw: Raw) -> pixel::Argb {
    match raw {
        Raw::P8(v) => pixel::decode8(v),
        Raw::P16(v) => pixel::decode16(v),
        Raw::P32(v) => v,
    }
}

fn render(
    frame: &Frame,
    output: &mut ae::Layer,
    input: Option<&ae::Layer>,
) -> Result<(), ae::Error> {
    let format = output.pixel_format()?;
    let bpp = bytes_per_pixel(format)?;
    let (w, h) = (output.width(), output.height());
    if w == 0 || h == 0 {
        return Ok(());
    }
    if w > i32::MAX as usize
        || h > i32::MAX as usize
        // The accessor only returns the host pointer; do not dereference it here.
        || unsafe { output.data_ptr() }.is_null()
        || w.checked_mul(bpp).is_none_or(|n| n > output.row_bytes().unsigned_abs())
    {
        return Err(ae::Error::BadCallbackParameter);
    }
    let origin = output.origin();
    let rect = PixelRect {
        x0: origin.h,
        y0: origin.v,
        w: w as u32,
        h: h as u32,
    };
    let (scene, _) = layout::build_scene(&frame.settings, &frame.mapping, frame.frame);
    let coverage = match backend::coverage(&scene, rect, frame.settings.samples, frame.backend) {
        Ok(c) => c,
        Err(e) => {
            record_error(e);
            return Err(ae::Error::Generic);
        }
    };
    let s = &frame.settings;
    let shading = Shading::new(s.color, s.intensity, s.opacity);
    let src = match input {
        Some(layer) => source(layer, format)?,
        None => None,
    };
    let over = s.composite == Composite::OverSource;
    for y in 0..h {
        for x in 0..w {
            let cov = coverage.values[y * w + x];
            let raw_src = if over {
                src.as_ref().and_then(|s| {
                    s.get(
                        i64::from(origin.h) + x as i64,
                        i64::from(origin.v) + y as i64,
                    )
                })
            } else {
                None
            };
            let value = if over {
                let straight = raw_src.map_or([0.0; 4], decode);
                match pixel::over_source(cov, &shading, straight) {
                    Some(v) => v,
                    None => {
                        // Bit-exact pass-through (or transparent outside the input).
                        write_raw(
                            output,
                            x,
                            y,
                            raw_src.unwrap_or(match format {
                                ae::PixelFormat::Argb32 => Raw::P8([0; 4]),
                                ae::PixelFormat::Argb64 => Raw::P16([0; 4]),
                                _ => Raw::P32([0.0; 4]),
                            }),
                        );
                        continue;
                    }
                }
            } else {
                pixel::lines_only(cov, &shading)
            };
            let raw = match format {
                ae::PixelFormat::Argb32 => Raw::P8(pixel::encode8(value)),
                ae::PixelFormat::Argb64 => Raw::P16(pixel::encode16(value)),
                _ => Raw::P32(pixel::encode32(value)),
            };
            write_raw(output, x, y, raw);
        }
    }
    Ok(())
}

fn write_raw(output: &mut ae::Layer, x: usize, y: usize, raw: Raw) {
    match raw {
        Raw::P8([a, r, g, b]) => {
            *output.as_pixel8_mut(x, y) = ae::Pixel8 {
                alpha: a,
                red: r,
                green: g,
                blue: b,
            }
        }
        Raw::P16([a, r, g, b]) => {
            *output.as_pixel16_mut(x, y) = ae::Pixel16 {
                alpha: a,
                red: r,
                green: g,
                blue: b,
            }
        }
        Raw::P32([a, r, g, b]) => {
            *output.as_pixel32_mut(x, y) = ae::PixelF32 {
                alpha: a,
                red: r,
                green: g,
                blue: b,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_union_and_intersection() {
        let a = ae::Rect {
            left: 0,
            top: 0,
            right: 10,
            bottom: 10,
        };
        let b = ae::Rect {
            left: 5,
            top: -5,
            right: 20,
            bottom: 8,
        };
        let i = intersect(a, b);
        assert_eq!((i.left, i.top, i.right, i.bottom), (5, 0, 10, 8));
        let u = union(a, b);
        assert_eq!((u.left, u.top, u.right, u.bottom), (0, -5, 20, 10));
        let e = intersect(
            a,
            ae::Rect {
                left: 20,
                top: 20,
                right: 30,
                bottom: 30,
            },
        );
        assert!(is_empty(&e));
        let u = union(e, a);
        assert_eq!((u.left, u.top, u.right, u.bottom), (0, 0, 10, 10));
    }

    #[test]
    fn checkin_runs_once_and_keeps_primary_error() {
        use std::cell::Cell;
        let calls = Cell::new(0);
        let r: Result<i32, ae::Error> = with_input_checkin(
            || Err(ae::Error::OutOfMemory),
            || {
                calls.set(calls.get() + 1);
                Err(ae::Error::BadCallbackParameter)
            },
        );
        assert_eq!(calls.get(), 1);
        assert_eq!(r, Err(ae::Error::OutOfMemory));
    }

    #[test]
    fn parameter_variant_names_are_unique() {
        let all = [
            Params::GroupFocusStart,
            Params::FocusPoint,
            Params::CenterSize,
            Params::CenterAspect,
            Params::CenterRotation,
            Params::CenterOffsetX,
            Params::CenterOffsetY,
            Params::GroupFocusEnd,
            Params::GroupLinesStart,
            Params::LineCount,
            Params::LineWidth,
            Params::WidthRandom,
            Params::TipWidth,
            Params::TaperLength,
            Params::TaperShape,
            Params::LengthRandom,
            Params::GroupLinesEnd,
            Params::GroupDistributionStart,
            Params::Seed,
            Params::AngleJitter,
            Params::Clustering,
            Params::ClusterCount,
            Params::ClusterSpread,
            Params::DensityVariation,
            Params::GapAngle,
            Params::GapWidth,
            Params::GapSoftness,
            Params::GroupDistributionEnd,
            Params::GroupTimeStart,
            Params::Evolution,
            Params::HoldFrames,
            Params::ChangeAmount,
            Params::Stagger,
            Params::GroupTimeEnd,
            Params::GroupColorStart,
            Params::Color,
            Params::Intensity,
            Params::Opacity,
            Params::Composite,
            Params::GroupColorEnd,
            Params::GroupRenderStart,
            Params::Backend,
            Params::Quality,
            Params::GroupRenderEnd,
            Params::TipCap,
            Params::GroupShortStart,
            Params::ShortMin,
            Params::ShortMax,
            Params::ShortMix,
            Params::GroupShortEnd,
        ];
        let names: std::collections::HashSet<String> =
            all.iter().map(|p| format!("{p:?}")).collect();
        assert_eq!(names.len(), all.len());
    }
}
