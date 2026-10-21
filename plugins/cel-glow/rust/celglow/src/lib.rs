//! CelGlow Rust (SmartFX): soft source mask -> distance field -> generated cel bands.
//! Reference: `PathArrayRust` + `after-effects` examples (`simplest`, `supervisor`).

use after_effects as ae;
use std::sync::OnceLock;

mod edt;
mod gpu;

static GPU: OnceLock<Option<gpu::GpuProcessor>> = OnceLock::new();
const MAX_BANDS: usize = 8;

fn get_gpu() -> Option<&'static gpu::GpuProcessor> {
    GPU.get_or_init(|| std::panic::catch_unwind(gpu::GpuProcessor::new).ok())
        .as_ref()
}

#[derive(Clone, Copy, Debug)]
struct CelGlowPlane {
    input_plane: ae::Rect,
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    // AE stores effect parameters by ordinal stream position. Append new params
    // at the end once a version is used in real projects.
    SourceMode = 0,
    LumaThreshold = 1,
    LumaSoftness = 2,
    SourceColor = 3,
    ColorTolerance = 4,
    SourceBlur = 5,
    BandCount = 6,
    GlowStart = 7,
    TotalSize = 8,
    SizeMode = 9,
    EdgeSoftness = 10,
    PaletteMode = 11,
    ColorInner = 12,
    ColorMiddle = 13,
    ColorOuter = 14,
    OpacityInner = 15,
    OpacityOuter = 16,
    Amount = 17,
    GlobalBlend = 18,
    PreserveAlpha = 19,
    Copies = 20,
    Distribution = 21,
    RotationStep = 22,
    ScaleStep = 23,
    RingRadius = 24,
    LineX = 25,
    LineY = 26,
    CopyOpacityStep = 27,
    View = 28,
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
            Params::SourceMode,
            "Source Mode",
            ae::PopupDef::setup(|f| {
                f.set_options(&[
                    "Luma",
                    "Color",
                    "Luma + Color Max",
                    "Luma + Color Intersect",
                ]);
                f.set_default(1);
            }),
        )?;
        params.add(
            Params::LumaThreshold,
            "Luma Threshold",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(255.0);
                f.set_slider_min(0.0);
                f.set_slider_max(255.0);
                f.set_default(200.0);
                f.set_precision(0);
            }),
        )?;
        params.add(
            Params::LumaSoftness,
            "Luma Softness",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(50.0);
                f.set_slider_min(0.0);
                f.set_slider_max(50.0);
                f.set_default(10.0);
                f.set_precision(0);
            }),
        )?;
        params.add(
            Params::SourceColor,
            "Source Color",
            ae::ColorDef::setup(|f| {
                f.set_default(ae::Pixel8 {
                    red: 255,
                    green: 255,
                    blue: 255,
                    alpha: 255,
                });
            }),
        )?;
        params.add(
            Params::ColorTolerance,
            "Color Tolerance (%)",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_default(20.0);
                f.set_precision(1);
            }),
        )?;
        params.add(
            Params::SourceBlur,
            "Source Box Blur (px)",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(64.0);
                f.set_slider_min(0.0);
                f.set_slider_max(32.0);
                f.set_default(0.0);
                f.set_precision(1);
            }),
        )?;

        params.add(
            Params::BandCount,
            "Band Count",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(1.0);
                f.set_valid_max(MAX_BANDS as f32);
                f.set_slider_min(1.0);
                f.set_slider_max(MAX_BANDS as f32);
                f.set_default(3.0);
                f.set_precision(0);
            }),
        )?;
        params.add(
            Params::GlowStart,
            "Glow Start (%)",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(200.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_default(0.0);
                f.set_precision(1);
            }),
        )?;
        params.add(
            Params::TotalSize,
            "Total Size (%)",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.1);
                f.set_valid_max(300.0);
                f.set_slider_min(1.0);
                f.set_slider_max(120.0);
                f.set_default(55.0);
                f.set_precision(1);
            }),
        )?;
        params.add(
            Params::SizeMode,
            "Size Mode",
            ae::PopupDef::setup(|f| {
                f.set_options(&["Equal", "Taper", "Expand"]);
                f.set_default(2);
            }),
        )?;
        params.add(
            Params::EdgeSoftness,
            "Edge Softness (px)",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(8.0);
                f.set_slider_min(0.0);
                f.set_slider_max(8.0);
                f.set_default(1.0);
                f.set_precision(1);
            }),
        )?;
        params.add(
            Params::PaletteMode,
            "Palette Mode",
            ae::PopupDef::setup(|f| {
                f.set_options(&["Single", "Inner - Outer", "3 Color Ramp"]);
                f.set_default(3);
            }),
        )?;
        params.add(
            Params::ColorInner,
            "Color Inner",
            ae::ColorDef::setup(|f| {
                f.set_default(ae::Pixel8 {
                    red: 255,
                    green: 255,
                    blue: 255,
                    alpha: 255,
                });
            }),
        )?;
        params.add(
            Params::ColorMiddle,
            "Color Middle",
            ae::ColorDef::setup(|f| {
                f.set_default(ae::Pixel8 {
                    red: 255,
                    green: 220,
                    blue: 80,
                    alpha: 255,
                });
            }),
        )?;
        params.add(
            Params::ColorOuter,
            "Color Outer",
            ae::ColorDef::setup(|f| {
                f.set_default(ae::Pixel8 {
                    red: 255,
                    green: 120,
                    blue: 40,
                    alpha: 255,
                });
            }),
        )?;
        params.add(
            Params::OpacityInner,
            "Opacity Inner (%)",
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
            Params::OpacityOuter,
            "Opacity Outer (%)",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_default(80.0);
                f.set_precision(1);
            }),
        )?;

        params.add(
            Params::Amount,
            "Amount (%)",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(200.0);
                f.set_slider_min(0.0);
                f.set_slider_max(200.0);
                f.set_default(100.0);
                f.set_precision(1);
            }),
        )?;
        params.add(
            Params::GlobalBlend,
            "Global Blend",
            ae::PopupDef::setup(|f| {
                f.set_options(&["Add", "Screen", "Normal"]);
                f.set_default(1);
            }),
        )?;
        params.add(
            Params::PreserveAlpha,
            "Preserve Alpha",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(false);
                f.set_label("On");
            }),
        )?;
        params.add(
            Params::Copies,
            "Copies",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(1.0);
                f.set_valid_max(16.0);
                f.set_slider_min(1.0);
                f.set_slider_max(16.0);
                f.set_default(1.0);
                f.set_precision(0);
            }),
        )?;
        params.add(
            Params::Distribution,
            "Distribution",
            ae::PopupDef::setup(|f| {
                f.set_options(&["Radial", "Ring", "Linear"]);
                f.set_default(1);
            }),
        )?;
        params.add(
            Params::RotationStep,
            "Rotation Step (deg)",
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
            Params::ScaleStep,
            "Scale Step (%)",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(1.0);
                f.set_valid_max(400.0);
                f.set_slider_min(25.0);
                f.set_slider_max(200.0);
                f.set_default(100.0);
                f.set_precision(1);
            }),
        )?;
        params.add(
            Params::RingRadius,
            "Ring Radius (%)",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(200.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_default(20.0);
                f.set_precision(1);
            }),
        )?;
        params.add(
            Params::LineX,
            "Line X (%)",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(-200.0);
                f.set_valid_max(200.0);
                f.set_slider_min(-100.0);
                f.set_slider_max(100.0);
                f.set_default(25.0);
                f.set_precision(1);
            }),
        )?;
        params.add(
            Params::LineY,
            "Line Y (%)",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(-200.0);
                f.set_valid_max(200.0);
                f.set_slider_min(-100.0);
                f.set_slider_max(100.0);
                f.set_default(0.0);
                f.set_precision(1);
            }),
        )?;
        params.add(
            Params::CopyOpacityStep,
            "Copy Opacity Step (%)",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(-100.0);
                f.set_valid_max(100.0);
                f.set_slider_min(-50.0);
                f.set_slider_max(50.0);
                f.set_default(0.0);
                f.set_precision(1);
            }),
        )?;

        params.add(
            Params::View,
            "View",
            ae::PopupDef::setup(|f| {
                f.set_options(&["Result", "Glow only", "Source only", "Distance", "Mask"]);
                f.set_default(1);
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
                    "CelGlow 0.2 (Rust)\rStepped glow via CPU EDT + three bands.\rhttps://github.com/onmk/CelGlow",
                );
                Ok(())
            }

            ae::Command::GlobalSetup => {
                out_data.set_out_flag(ae::OutFlags::DeepColorAware, true);
                out_data.set_out_flag(ae::OutFlags::PixIndependent, true);
                out_data.set_out_flag(ae::OutFlags::UseOutputExtent, true);
                out_data.set_out_flag2(ae::OutFlags2::SupportsSmartRender, true);
                out_data.set_out_flag2(ae::OutFlags2::FloatColorAware, true);
                out_data.set_out_flag2(ae::OutFlags2::SupportsThreadedRendering, true);
                out_data.set_out_flag2(ae::OutFlags2::SupportsGetFlattenedSequenceData, true);
                Ok(())
            }

            ae::Command::SmartPreRender { mut extra } => {
                let sref = in_data.width().min(in_data.height()).max(1);
                let pad = max_render_pad_px(params, sref as f64)?;

                let mut req = extra.output_request();
                let r: ae::Rect = req.rect.into();
                let inflated = inflate_rect(r, pad);
                req.rect = inflated.into();

                let in_result = extra.callbacks().checkout_layer(
                    0,
                    0,
                    &req,
                    in_data.current_time(),
                    in_data.time_step(),
                    in_data.time_scale(),
                )?;

                extra.set_returns_extra_pixels(true);
                extra.set_result_rect(in_result.result_rect.into());
                extra.set_max_result_rect(in_result.max_result_rect.into());
                extra.set_pre_render_data(CelGlowPlane {
                    input_plane: in_result.result_rect.into(),
                });
                Ok(())
            }

            ae::Command::SmartRender { extra } => {
                smart_render(&extra, &in_data, params)?;
                Ok(())
            }

            ae::Command::SmartRenderGpu { extra } => {
                smart_render(&extra, &in_data, params)?;
                Ok(())
            }

            _ => Ok(()),
        }
    }
}

fn inflate_rect(mut r: ae::Rect, pad: i32) -> ae::Rect {
    r.left -= pad;
    r.top -= pad;
    r.right += pad;
    r.bottom += pad;
    r
}

fn max_outer_pct(params: &ae::Parameters<Params>) -> Result<f64, ae::Error> {
    let start = params.get(Params::GlowStart)?.as_float_slider()?.value();
    let total = params.get(Params::TotalSize)?.as_float_slider()?.value();
    Ok(start + total)
}

fn max_render_pad_px(params: &ae::Parameters<Params>, sref: f64) -> Result<i32, ae::Error> {
    let outer = max_outer_pct(params)? / 100.0 * sref;
    let source_blur = params.get(Params::SourceBlur)?.as_float_slider()?.value();
    let copies = (params
        .get(Params::Copies)?
        .as_float_slider()?
        .value()
        .round() as usize)
        .clamp(1, 16);
    let scale_step = (params.get(Params::ScaleStep)?.as_float_slider()?.value() / 100.0).max(0.01);
    let max_scale = if copies <= 1 {
        1.0
    } else {
        scale_step.powf((copies - 1) as f64).max(1.0)
    };
    let distribution = params.get(Params::Distribution)?.as_popup()?.value() as i32;
    let offset = match distribution {
        2 => params.get(Params::RingRadius)?.as_float_slider()?.value() / 100.0 * sref,
        3 => {
            let lx = params.get(Params::LineX)?.as_float_slider()?.value() / 100.0 * sref;
            let ly = params.get(Params::LineY)?.as_float_slider()?.value() / 100.0 * sref;
            (lx * lx + ly * ly).sqrt() * (copies.saturating_sub(1) as f64)
        }
        _ => 0.0,
    };
    Ok((outer * max_scale + offset + source_blur).ceil() as i32)
}

#[derive(Clone, Copy)]
struct Band {
    inner_px: f32,
    outer_px: f32,
    r: f32,
    g: f32,
    b: f32,
    opacity: f32,
    edge_soft_px: f32,
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    if edge1 <= edge0 {
        return if x >= edge1 { 1.0 } else { 0.0 };
    }
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn band_weight(d_px: f32, b: &Band, glow_inner_px: f32, glow_outer_px: f32) -> f32 {
    let inner = b.inner_px;
    let outer = b.outer_px;
    if outer <= inner + 1e-4 {
        return 0.0;
    }
    if d_px < inner || d_px > outer {
        return 0.0;
    }
    let es = b.edge_soft_px.max(0.0);
    let aa_in = if es > 1e-4 && (inner - glow_inner_px).abs() <= 1e-3 {
        smoothstep(glow_inner_px, glow_inner_px + es, d_px)
    } else {
        1.0
    };
    let aa_out = if es > 1e-4 && (outer - glow_outer_px).abs() <= 1e-3 {
        1.0 - smoothstep(glow_outer_px - es, glow_outer_px, d_px)
    } else {
        1.0
    };
    b.opacity * aa_in * aa_out
}

fn sample_1ch_nearest(buf: &[f32], w: usize, h: usize, x: f32, y: f32, fallback: f32) -> f32 {
    let ix = x.round() as i32;
    let iy = y.round() as i32;
    if ix < 0 || iy < 0 || ix as usize >= w || iy as usize >= h {
        return fallback;
    }
    buf[iy as usize * w + ix as usize]
}

fn combine_rgb(
    sr: f32,
    sg: f32,
    sb: f32,
    gr: f32,
    gg: f32,
    gb: f32,
    ga: f32,
    blend: i32,
) -> (f32, f32, f32) {
    match blend {
        2 => (
            1.0 - (1.0 - sr).max(0.0) * (1.0 - gr.clamp(0.0, 1.0)),
            1.0 - (1.0 - sg).max(0.0) * (1.0 - gg.clamp(0.0, 1.0)),
            1.0 - (1.0 - sb).max(0.0) * (1.0 - gb.clamp(0.0, 1.0)),
        ),
        3 => {
            let a = ga.clamp(0.0, 1.0);
            (
                sr * (1.0 - a) + gr * a,
                sg * (1.0 - a) + gg * a,
                sb * (1.0 - a) + gb * a,
            )
        }
        _ => (sr + gr, sg + gg, sb + gb),
    }
}

fn copy_transform(
    x: f32,
    y: f32,
    idx: usize,
    copies: usize,
    distribution: i32,
    rotation_step: f32,
    scale_step: f32,
    ring_radius: f32,
    line_x: f32,
    line_y: f32,
    cx: f32,
    cy: f32,
) -> (f32, f32) {
    let i = idx as f32;
    let scale = scale_step.max(0.01).powf(i);
    let mut ox = 0.0;
    let mut oy = 0.0;
    let mut theta = rotation_step * i;

    match distribution {
        2 => {
            let angle = if copies > 0 {
                std::f32::consts::TAU * i / copies as f32
            } else {
                0.0
            };
            ox = angle.cos() * ring_radius;
            oy = angle.sin() * ring_radius;
            theta += angle;
        }
        3 => {
            ox = line_x * i;
            oy = line_y * i;
        }
        _ => {}
    }

    let px = x - (cx + ox);
    let py = y - (cy + oy);
    let c = theta.cos();
    let s = theta.sin();
    let qx = (px * c + py * s) / scale + cx;
    let qy = (-px * s + py * c) / scale + cy;
    (qx, qy)
}

fn read_params_bands(params: &ae::Parameters<Params>, s: f32) -> Result<Vec<Band>, ae::Error> {
    let count = (params
        .get(Params::BandCount)?
        .as_float_slider()?
        .value()
        .round() as usize)
        .clamp(1, MAX_BANDS);
    let start_px = params.get(Params::GlowStart)?.as_float_slider()?.value() as f32 / 100.0 * s;
    let total_px = params.get(Params::TotalSize)?.as_float_slider()?.value() as f32 / 100.0 * s;
    let size_mode = params.get(Params::SizeMode)?.as_popup()?.value() as i32;
    let edge_soft_px = params.get(Params::EdgeSoftness)?.as_float_slider()?.value() as f32;
    let palette_mode = params.get(Params::PaletteMode)?.as_popup()?.value() as i32;
    let c_inner = params.get(Params::ColorInner)?.as_color()?.float_value()?;
    let c_mid = params.get(Params::ColorMiddle)?.as_color()?.float_value()?;
    let c_outer = params.get(Params::ColorOuter)?.as_color()?.float_value()?;
    let op_inner = params.get(Params::OpacityInner)?.as_float_slider()?.value() as f32 / 100.0;
    let op_outer = params.get(Params::OpacityOuter)?.as_float_slider()?.value() as f32 / 100.0;

    let mut weights = Vec::with_capacity(count);
    for i in 0..count {
        let t = if count <= 1 {
            0.0
        } else {
            i as f32 / (count - 1) as f32
        };
        let w = match size_mode {
            2 => 1.0 - 0.65 * t,
            3 => 0.35 + 0.65 * t,
            _ => 1.0,
        };
        weights.push(w.max(0.05));
    }
    let sum_w: f32 = weights.iter().sum();

    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let color_at = |t: f32| -> (f32, f32, f32) {
        match palette_mode {
            1 => (c_inner.red, c_inner.green, c_inner.blue),
            2 => (
                lerp(c_inner.red, c_outer.red, t),
                lerp(c_inner.green, c_outer.green, t),
                lerp(c_inner.blue, c_outer.blue, t),
            ),
            _ => {
                if t <= 0.5 {
                    let u = t * 2.0;
                    (
                        lerp(c_inner.red, c_mid.red, u),
                        lerp(c_inner.green, c_mid.green, u),
                        lerp(c_inner.blue, c_mid.blue, u),
                    )
                } else {
                    let u = (t - 0.5) * 2.0;
                    (
                        lerp(c_mid.red, c_outer.red, u),
                        lerp(c_mid.green, c_outer.green, u),
                        lerp(c_mid.blue, c_outer.blue, u),
                    )
                }
            }
        }
    };

    let mut bands = Vec::with_capacity(count);
    let mut inner = start_px;
    for (i, w) in weights.iter().enumerate() {
        let width = total_px * *w / sum_w;
        let outer = inner + width;
        let t = if count <= 1 {
            0.0
        } else {
            i as f32 / (count - 1) as f32
        };
        let (r, g, b) = color_at(t);
        bands.push(Band {
            inner_px: inner,
            outer_px: outer,
            r,
            g,
            b,
            opacity: lerp(op_inner, op_outer, t),
            edge_soft_px,
        });
        inner = outer;
    }
    Ok(bands)
}

fn bands_to_gpu(bands: &[Band]) -> ([gpu::GpuBand; MAX_BANDS], usize) {
    let mut out = [gpu::GpuBand::default(); MAX_BANDS];
    for (dst, b) in out.iter_mut().zip(bands.iter()) {
        *dst = gpu::GpuBand {
            inner_px: b.inner_px,
            outer_px: b.outer_px,
            opacity: b.opacity,
            edge_soft_px: b.edge_soft_px,
            color: [b.r, b.g, b.b, 0.0],
        };
    }
    (out, bands.len().min(MAX_BANDS))
}

/// Layer → row-major RGBA f32 0..1 (alpha first in slice order A,R,G,B per pixel).
fn float_plane(w:usize,h:usize,channels:usize)->Result<Vec<f32>,ae::Error> {
    let count=w.checked_mul(h).and_then(|n|n.checked_mul(channels)).ok_or(ae::Error::OutOfMemory)?;
    let mut values=Vec::new();
    values.try_reserve_exact(count).map_err(|_|ae::Error::OutOfMemory)?;
    values.resize(count,0.0);
    Ok(values)
}

fn box_blur_1ch(src: &[f32], w: usize, h: usize, radius: usize) -> Result<Vec<f32>,ae::Error> {
    if Some(src.len())!=w.checked_mul(h) {return Err(ae::Error::BadCallbackParameter);}
    if radius == 0 || w == 0 || h == 0 {
        let mut out=float_plane(w,h,1)?;
        out.copy_from_slice(src);
        return Ok(out);
    }

    let mut tmp = float_plane(w,h,1)?;
    let mut out = float_plane(w,h,1)?;
    let mut prefix=float_plane(w.max(h).checked_add(1).ok_or(ae::Error::OutOfMemory)?,1,1)?;

    for y in 0..h {
        prefix[0]=0.0;
        for x in 0..w {
            prefix[x + 1] = prefix[x] + src[y * w + x];
        }
        for x in 0..w {
            let left = x.saturating_sub(radius);
            let right = x.saturating_add(radius).min(w - 1);
            tmp[y * w + x] = (prefix[right + 1] - prefix[left]) / (right - left + 1) as f32;
        }
    }

    for x in 0..w {
        prefix[0]=0.0;
        for y in 0..h {
            prefix[y + 1] = prefix[y] + tmp[y * w + x];
        }
        for y in 0..h {
            let top = y.saturating_sub(radius);
            let bottom = y.saturating_add(radius).min(h - 1);
            out[y * w + x] = (prefix[bottom + 1] - prefix[top]) / (bottom - top + 1) as f32;
        }
    }

    Ok(out)
}

#[test]
fn blur_workspace_matches_direct_average_and_rejects_bad_sizes() {
    assert!(float_plane(usize::MAX,2,4).is_err());
    assert!(float_plane(usize::MAX,1,1).is_err());
    assert!(box_blur_1ch(&[0.0],2,2,0).is_err());
    assert!(box_blur_1ch(&[],0,7,2).unwrap().is_empty());
    for (w,h) in [(1,1),(1,7),(9,1),(7,9)] {for radius in [0,1,3,64,usize::MAX] {
        let src:Vec<f32>=(0..w*h).map(|i|(i%11) as f32/10.0).collect();
        let out=box_blur_1ch(&src,w,h,radius).unwrap();
        for y in 0..h {for x in 0..w {
            let mut sum=0.0f64; let mut count=0;
            for sy in y.saturating_sub(radius)..=y.saturating_add(radius).min(h-1) {
                for sx in x.saturating_sub(radius)..=x.saturating_add(radius).min(w-1) {sum+=src[sy*w+sx] as f64;count+=1;}
            }
            assert!((out[y*w+x] as f64-sum/count as f64).abs()<1e-6);
        }}
    }}
}

fn depth_from_format(format: ae::PixelFormat) -> Result<i16, ae::Error> {
    match format {
        ae::PixelFormat::Argb32 => Ok(8),
        ae::PixelFormat::Argb64 => Ok(16),
        ae::PixelFormat::Argb128 => Ok(32),
        _ => Err(ae::Error::BadCallbackParameter),
    }
}

#[test]
fn cpu_world_formats_are_explicit() {
    assert_eq!(depth_from_format(ae::PixelFormat::Argb32).unwrap(), 8);
    assert_eq!(depth_from_format(ae::PixelFormat::Argb64).unwrap(), 16);
    assert_eq!(depth_from_format(ae::PixelFormat::Argb128).unwrap(), 32);
    assert!(depth_from_format(ae::PixelFormat::GpuBgra128).is_err());
}

fn checked_layout(w:usize,h:usize,row_bytes:isize,depth:i16)->Result<usize,ae::Error> {
    if ![8,16,32].contains(&depth) || w>i32::MAX as usize || h>i32::MAX as usize || row_bytes==i32::MIN as isize {
        return Err(ae::Error::BadCallbackParameter);
    }
    let stride=row_bytes.unsigned_abs();
    let row=w.checked_mul(depth as usize/2).ok_or(ae::Error::OutOfMemory)?;
    let length=stride.checked_mul(h).ok_or(ae::Error::OutOfMemory)?;
    if w!=0 && h!=0 && (stride<row || length>isize::MAX as usize) {return Err(ae::Error::BadCallbackParameter);}
    Ok(stride)
}

fn world_stride(layer:&ae::Layer,depth:i16)->Result<usize,ae::Error> {
    let stride=checked_layout(layer.width(),layer.height(),layer.row_bytes(),depth)?;
    if depth_from_format(layer.pixel_format()?)?!=depth {return Err(ae::Error::BadCallbackParameter);}
    if layer.width()!=0 && layer.height()!=0 && unsafe{layer.data_ptr()}.is_null() {return Err(ae::Error::BadCallbackParameter);}
    Ok(stride)
}

fn physical_row(y:usize,h:usize,negative:bool)->usize {if negative {h-1-y}else{y}}

#[test]
fn legacy_world_layout_checks_depth_stride_and_orientation() {
    for depth in [8,16,32] {
        let stride=17*depth as usize/2+5;
        assert_eq!(checked_layout(17,31,stride as isize,depth),Ok(stride));
        assert_eq!(checked_layout(17,31,-(stride as isize),depth),Ok(stride));
        assert!(checked_layout(17,31,1,depth).is_err());
    }
    assert!(checked_layout(1,1,4,12).is_err());
    assert!(checked_layout(1,1,i32::MIN as isize,8).is_err());
    assert!(checked_layout(i32::MAX as usize+1,1,4,8).is_err());
    assert!(checked_layout(1,2,isize::MAX,8).is_err());
    assert_eq!((0..3).map(|y|physical_row(y,3,true)).collect::<Vec<_>>(),[2,1,0]);
    assert_eq!((0..3).map(|y|physical_row(y,3,false)).collect::<Vec<_>>(),[0,1,2]);
}

fn layer_to_rgba_f32(layer: &ae::Layer, depth: i16) -> Result<(Vec<f32>, usize, usize),ae::Error> {
    let w = layer.width() as usize;
    let h = layer.height() as usize;
    if w == 0 || h == 0 {
        return Ok((Vec::new(), 0, 0));
    }
    let stride = world_stride(layer,depth)?;
    let negative = layer.row_bytes()<0;
    let buf = layer.buffer();
    let count=w.checked_mul(h).and_then(|n|n.checked_mul(4)).ok_or(ae::Error::OutOfMemory)?;
    let mut out=Vec::new();
    out.try_reserve_exact(count).map_err(|_|ae::Error::OutOfMemory)?;
    out.resize(count,0.0f32);
    match depth {
        32 => {
            for y in 0..h {
                for x in 0..w {
                    let si = physical_row(y,h,negative) * stride + x * 16;
                    let di = (y * w + x) * 4;
                    if si + 15 < buf.len() && di + 3 < out.len() {
                        for ch in 0..4 {
                            let v = f32::from_ne_bytes([
                                buf[si + ch * 4],
                                buf[si + ch * 4 + 1],
                                buf[si + ch * 4 + 2],
                                buf[si + ch * 4 + 3],
                            ]);
                            out[di + ch] = v;
                        }
                    }
                }
            }
        }
        16 => {
            for y in 0..h {
                for x in 0..w {
                    let si = physical_row(y,h,negative) * stride + x * 8;
                    let di = (y * w + x) * 4;
                    if si + 7 < buf.len() && di + 3 < out.len() {
                        for ch in 0..4 {
                            let v16 = u16::from_ne_bytes([buf[si + ch * 2], buf[si + ch * 2 + 1]]);
                            out[di + ch] = v16 as f32 / 32768.0;
                        }
                    }
                }
            }
        }
        _ => {
            for y in 0..h {
                for x in 0..w {
                    let si = physical_row(y,h,negative) * stride + x * 4;
                    let di = (y * w + x) * 4;
                    if si + 3 < buf.len() && di + 3 < out.len() {
                        for ch in 0..4 {
                            out[di + ch] = buf[si + ch] as f32 / 255.0;
                        }
                    }
                }
            }
        }
    }
    Ok((out, w, h))
}

fn write_rgba_f32_to_layer(flat: &[f32], layer: &mut ae::Layer, w: usize, h: usize, depth: i16) -> Result<(),ae::Error> {
    if (w,h)!=(layer.width(),layer.height()) || Some(flat.len())!=w.checked_mul(h).and_then(|n|n.checked_mul(4)) {return Err(ae::Error::BadCallbackParameter);}
    if w==0 || h==0 {return Ok(());}
    let stride = world_stride(layer,depth)?;
    let negative = layer.row_bytes()<0;
    let buf = layer.buffer_mut();
    match depth {
        32 => {
            for y in 0..h {
                for x in 0..w {
                    let si = (y * w + x) * 4;
                    let di = physical_row(y,h,negative) * stride + x * 16;
                    if si + 3 < flat.len() && di + 15 < buf.len() {
                        for ch in 0..4 {
                            let bytes = flat[si + ch].to_ne_bytes();
                            buf[di + ch * 4] = bytes[0];
                            buf[di + ch * 4 + 1] = bytes[1];
                            buf[di + ch * 4 + 2] = bytes[2];
                            buf[di + ch * 4 + 3] = bytes[3];
                        }
                    }
                }
            }
        }
        16 => {
            for y in 0..h {
                for x in 0..w {
                    let si = (y * w + x) * 4;
                    let di = physical_row(y,h,negative) * stride + x * 8;
                    if si + 3 < flat.len() && di + 7 < buf.len() {
                        for ch in 0..4 {
                            let v = ((flat[si + ch].clamp(0.0, 1.0)) * 32768.0).round() as u16;
                            let b = v.to_ne_bytes();
                            buf[di + ch * 2] = b[0];
                            buf[di + ch * 2 + 1] = b[1];
                        }
                    }
                }
            }
        }
        _ => {
            for y in 0..h {
                for x in 0..w {
                    let si = (y * w + x) * 4;
                    let di = physical_row(y,h,negative) * stride + x * 4;
                    if si + 3 < flat.len() && di + 3 < buf.len() {
                        for ch in 0..4 {
                            buf[di + ch] = (flat[si + ch] * 255.0).clamp(0.0, 255.0) as u8;
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn with_input_checkin(
    work: impl FnOnce()->Result<(),ae::Error>,
    checkin: impl FnOnce()->Result<(),ae::Error>,
)->Result<(),ae::Error> {
    let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(work));
    let cleanup=checkin();
    match result {Ok(result)=>result.and(cleanup),Err(payload)=>std::panic::resume_unwind(payload)}
}

#[test]
fn input_checkin_covers_early_errors_and_unwind() {
    for mode in 0..3 {for release_fails in [false,true] {
        let calls=std::cell::Cell::new(0);
        let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            with_input_checkin(||match mode {
                0=>Ok(()),1=>Err(ae::Error::BadCallbackParameter),_=>panic!("work failure"),
            },||{calls.set(calls.get()+1);if release_fails{Err(ae::Error::Generic)}else{Ok(())}})
        }));
        assert_eq!(calls.get(),1);
        match mode {
            0=>assert_eq!(result.unwrap(),if release_fails{Err(ae::Error::Generic)}else{Ok(())}),
            1=>assert_eq!(result.unwrap(),Err(ae::Error::BadCallbackParameter)),
            _=>assert!(result.is_err()),
        }
    }}
}

// Callers validate world dimensions before iterating offsets. Widen before
// either operation: valid i32 origins can differ by more than i32::MAX.
fn plane_coordinate(origin: i32, plane_start: i32, offset: usize) -> i64 {
    i64::from(origin) - i64::from(plane_start) + offset as i64
}

#[test]
fn plane_coordinates_do_not_wrap_into_the_input() {
    assert_eq!(plane_coordinate(i32::MIN, i32::MAX, 0), -4_294_967_295);
    assert_eq!(plane_coordinate(i32::MAX, i32::MIN, 1), 4_294_967_296);
    assert_eq!(plane_coordinate(i32::MAX, i32::MAX, 31), 31);
    for origin in [-127, 0, 91] {
        for plane in [-63, 0, 75] {
            for offset in 0..65 {
                assert_eq!(plane_coordinate(origin, plane, offset),
                    i64::from(origin + offset as i32 - plane));
            }
        }
    }
}

fn smart_render(
    extra: &ae::pf::SmartRenderExtra,
    in_data: &ae::InData,
    params: &ae::Parameters<Params>,
) -> Result<(), ae::Error> {
    let cb = extra.callbacks();
    let Some(input_world) = cb.checkout_layer_pixels(0)? else {
        return Ok(());
    };
    with_input_checkin(|| {
    let Some(mut output_world) = cb.checkout_output()? else {
        return Ok(());
    };

    let formats = (|| -> Result<_, ae::Error> {
        Ok((depth_from_format(input_world.pixel_format()?)?,
            depth_from_format(output_world.pixel_format()?)?))
    })();
    let (input_depth, output_depth) = match formats {
        Ok(depths) => depths,
        Err(err) => return Err(err),
    };
    let out_w = output_world.width() as usize;
    let out_h = output_world.height() as usize;
    if out_w == 0 || out_h == 0 {
        return Ok(());
    }
    world_stride(&output_world, output_depth)?;

    let plane = match extra.pre_render_data::<CelGlowPlane>() {
        Some(p) => p.input_plane,
        None => {
            let (inf, iw, ih) = layer_to_rgba_f32(&input_world, input_depth)?;
            let mut flat = float_plane(out_w,out_h,4)?;
            for y in 0..out_h.min(ih) {
                for x in 0..out_w.min(iw) {
                    let s = (y * iw + x) * 4;
                    let d = (y * out_w + x) * 4;
                    if s + 3 < inf.len() && d + 3 < flat.len() {
                        flat[d..d + 4].copy_from_slice(&inf[s..s + 4]);
                    }
                }
            }
            write_rgba_f32_to_layer(&flat, &mut output_world, out_w, out_h, output_depth)?;
            return Ok(());
        }
    };

    let view = params.get(Params::View)?.as_popup()?.value() as i32;

    let (rgba_in, iw, ih) = layer_to_rgba_f32(&input_world, input_depth)?;
    if iw == 0 || ih == 0 {
        return Ok(());
    }
    let ox0 = in_data.output_origin().h;
    let oy0 = in_data.output_origin().v;
    let mut out_flat = float_plane(out_w,out_h,4)?;

    if view == 3 {
        for y in 0..out_h {
            for x in 0..out_w {
                let ix = plane_coordinate(ox0, plane.left, x);
                let iy = plane_coordinate(oy0, plane.top, y);
                if ix >= 0 && iy >= 0 && (ix as usize) < iw && (iy as usize) < ih {
                    let si = ((iy as usize) * iw + ix as usize) * 4;
                    let di = (y * out_w + x) * 4;
                    out_flat[di..di + 4].copy_from_slice(&rgba_in[si..si + 4]);
                }
            }
        }
        write_rgba_f32_to_layer(&out_flat, &mut output_world, out_w, out_h, output_depth)?;
        return Ok(());
    }

    let source_mode = params.get(Params::SourceMode)?.as_popup()?.value() as i32;
    let source_color = params.get(Params::SourceColor)?.as_color()?.float_value()?;
    let color_tolerance = params
        .get(Params::ColorTolerance)?
        .as_float_slider()?
        .value() as f32
        / 100.0;
    let luma_t = params
        .get(Params::LumaThreshold)?
        .as_float_slider()?
        .value() as f32
        / 255.0;
    let luma_s = params.get(Params::LumaSoftness)?.as_float_slider()?.value() as f32 / 255.0;
    let source_blur = params
        .get(Params::SourceBlur)?
        .as_float_slider()?
        .value()
        .round()
        .clamp(0.0, 64.0) as usize;
    let t0 = luma_t - luma_s;
    let t1 = luma_t + luma_s;
    let amount = (params.get(Params::Amount)?.as_float_slider()?.value() as f32 / 100.0).max(0.0);
    let global_blend = params.get(Params::GlobalBlend)?.as_popup()?.value() as i32;
    let preserve_alpha = params.get(Params::PreserveAlpha)?.as_checkbox()?.value();
    let copies = (params
        .get(Params::Copies)?
        .as_float_slider()?
        .value()
        .round() as usize)
        .clamp(1, 16);
    let distribution = params.get(Params::Distribution)?.as_popup()?.value() as i32;
    let rotation_step =
        (params.get(Params::RotationStep)?.as_float_slider()?.value() as f32).to_radians();
    let scale_step = params.get(Params::ScaleStep)?.as_float_slider()?.value() as f32 / 100.0;
    let copy_opacity_step = params
        .get(Params::CopyOpacityStep)?
        .as_float_slider()?
        .value() as f32
        / 100.0;
    let s = (iw.min(ih).max(1)) as f32;
    let bands = read_params_bands(params, s)?;
    let ring_radius = params.get(Params::RingRadius)?.as_float_slider()?.value() as f32 / 100.0 * s;
    let line_x = params.get(Params::LineX)?.as_float_slider()?.value() as f32 / 100.0 * s;
    let line_y = params.get(Params::LineY)?.as_float_slider()?.value() as f32 / 100.0 * s;
    let cx = (iw as f32 - 1.0) * 0.5;
    let cy = (ih as f32 - 1.0) * 0.5;
    let (gpu_bands, gpu_band_count) = bands_to_gpu(&bands);
    let glow_inner_px = bands.first().map(|b| b.inner_px).unwrap_or(0.0);
    let glow_outer_px = bands.last().map(|b| b.outer_px).unwrap_or(0.0);

    if source_blur == 0 {
        if let Some(gpu) = get_gpu() {
            let input = gpu::RenderInput {
                rgba_in: &rgba_in,
                dist: &[],
                soft: &[],
                out_w,
                out_h,
                in_w: iw,
                in_h: ih,
                ox0,
                oy0,
                plane_left: plane.left,
                plane_top: plane.top,
                copies,
                band_count: gpu_band_count,
                distribution,
                global_blend,
                preserve_alpha,
                view,
                source_mode,
                source_color: [
                    source_color.red,
                    source_color.green,
                    source_color.blue,
                    source_color.alpha,
                ],
                color_tolerance,
                luma_t,
                luma_s,
                amount,
                max_d: 1.0,
                rotation_step,
                scale_step,
                ring_radius,
                line_x,
                line_y,
                copy_opacity_step,
                center_x: cx,
                center_y: cy,
                bands: gpu_bands,
            };
            if let Some(flat) = gpu.process_with_gpu_edt(&input) {
                write_rgba_f32_to_layer(&flat, &mut output_world, out_w, out_h, output_depth)?;
                return Ok(());
            }
        }
    }

    let mut soft = float_plane(iw,ih,1)?;
    for y in 0..ih {
        for x in 0..iw {
            let i = (y * iw + x) * 4;
            let r = rgba_in[i + 1];
            let g = rgba_in[i + 2];
            let b = rgba_in[i + 3];
            let a = rgba_in[i];
            let y709 = 0.2126 * r + 0.7152 * g + 0.0722 * b;
            let luma_m = if luma_s <= 0.0 {
                if y709 >= luma_t {
                    1.0
                } else {
                    0.0
                }
            } else if y709 <= t0 {
                0.0
            } else if y709 >= t1 {
                1.0
            } else {
                (y709 - t0) / (t1 - t0)
            };
            let dr = r - source_color.red;
            let dg = g - source_color.green;
            let db = b - source_color.blue;
            let color_dist = (dr * dr + dg * dg + db * db).sqrt() / 3.0f32.sqrt();
            let color_soft = 0.10f32.max(color_tolerance * 0.25);
            let color_m =
                1.0 - smoothstep(color_tolerance, color_tolerance + color_soft, color_dist);
            let mut m = match source_mode {
                2 => color_m,
                3 => luma_m.max(color_m),
                4 => luma_m.min(color_m),
                _ => luma_m,
            };
            m *= a;
            soft[y * iw + x] = m;
        }
    }
    if source_blur > 0 {
        soft = box_blur_1ch(&soft, iw, ih, source_blur)?;
    }

    let mut bin = float_plane(iw,ih,1)?;
    for (dst, &m) in bin.iter_mut().zip(&soft) {
        *dst = if m > 0.001 { 1.0 } else { 0.0 };
    }

    let mut dist = float_plane(iw,ih,1)?;
    edt::euclidean_dt(&bin, iw, ih, &mut dist)?;
    let mut max_d = 1.0f32;
    for &d in &dist {
        if d < 1e18 {
            max_d = max_d.max(d);
        }
    }

    if let Some(gpu) = get_gpu() {
        let input = gpu::RenderInput {
            rgba_in: &rgba_in,
            dist: &dist,
            soft: &soft,
            out_w,
            out_h,
            in_w: iw,
            in_h: ih,
            ox0,
            oy0,
            plane_left: plane.left,
            plane_top: plane.top,
            copies,
            band_count: gpu_band_count,
            distribution,
            global_blend,
            preserve_alpha,
            view,
            source_mode,
            source_color: [
                source_color.red,
                source_color.green,
                source_color.blue,
                source_color.alpha,
            ],
            color_tolerance,
            luma_t,
            luma_s,
            amount,
            max_d,
            rotation_step,
            scale_step,
            ring_radius,
            line_x,
            line_y,
            copy_opacity_step,
            center_x: cx,
            center_y: cy,
            bands: gpu_bands,
        };
        if let Some(flat) = gpu.process(&input) {
            write_rgba_f32_to_layer(&flat, &mut output_world, out_w, out_h, output_depth)?;
            return Ok(());
        }
    }

    for y in 0..out_h {
        for x in 0..out_w {
            let ix = plane_coordinate(ox0, plane.left, x);
            let iy = plane_coordinate(oy0, plane.top, y);

            let (mut in_r, mut in_g, mut in_b, mut in_a) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
            let mut d_px = max_d;
            let mut sm = 0.0f32;
            if ix >= 0 && iy >= 0 && (ix as usize) < iw && (iy as usize) < ih {
                let ii = (iy as usize) * iw + (ix as usize);
                let i4 = ii * 4;
                in_a = rgba_in[i4];
                in_r = rgba_in[i4 + 1];
                in_g = rgba_in[i4 + 2];
                in_b = rgba_in[i4 + 3];
                d_px = dist[ii];
                sm = soft[ii];
            }

            let mut gr = 0.0f32;
            let mut gg = 0.0f32;
            let mut gb = 0.0f32;
            let mut ga = 0.0f32;
            for copy_idx in 0..copies {
                let (sample_x, sample_y) = copy_transform(
                    ix as f32,
                    iy as f32,
                    copy_idx,
                    copies,
                    distribution,
                    rotation_step,
                    scale_step,
                    ring_radius,
                    line_x,
                    line_y,
                    cx,
                    cy,
                );
                let cd = sample_1ch_nearest(&dist, iw, ih, sample_x, sample_y, max_d);
                let copy_opacity = (1.0 + copy_opacity_step * copy_idx as f32).clamp(0.0, 2.0);
                for b in &bands {
                    let wgt = band_weight(cd, b, glow_inner_px, glow_outer_px) * copy_opacity;
                    if wgt > 0.0 {
                        gr += b.r * wgt;
                        gg += b.g * wgt;
                        gb += b.b * wgt;
                        ga += wgt;
                    }
                }
            }
            gr *= amount;
            gg *= amount;
            gb *= amount;
            ga *= amount;

            let (o_r, o_g, o_b, o_a) = match view {
                2 => (gr, gg, gb, ga),
                4 => {
                    let u = (d_px / max_d).clamp(0.0, 1.0);
                    (u, u, u, in_a)
                }
                5 => (sm, sm, sm, in_a),
                _ => {
                    let (o_r, o_g, o_b) =
                        combine_rgb(in_r, in_g, in_b, gr, gg, gb, ga, global_blend);
                    let o_a = if preserve_alpha {
                        in_a
                    } else {
                        in_a + (1.0 - in_a) * ga.clamp(0.0, 1.0)
                    };
                    (o_r, o_g, o_b, o_a)
                }
            };

            let di = (y * out_w + x) * 4;
            out_flat[di] = o_a;
            out_flat[di + 1] = o_r;
            out_flat[di + 2] = o_g;
            out_flat[di + 3] = o_b;
        }
    }

    write_rgba_f32_to_layer(&out_flat, &mut output_world, out_w, out_h, output_depth)?;
    Ok(())
    },||cb.checkin_layer_pixels(0))
}
