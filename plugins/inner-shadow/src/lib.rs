//! onmk Inner Shadow — After Effects glue. Math: `spec`; backends: `cpu`,
//! `gpu`; host-independent entry: `engine`; world adapters: `world`.
//! Specification: docs/design.md.

use after_effects as ae;

pub mod cpu;
pub mod engine;
pub mod gpu;
pub mod spec;
pub mod world;

use spec::{Blend, GpuMode, Plan, Rect64, Scale, ShadowParams};

// Parameter disk IDs are hashes of these variant names and UI indices follow
// `add` order: never rename or reorder, append only.
#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    BlendMode,
    Color,
    Opacity,
    Angle,
    Distance,
    Choke,
    Size,
    GpuMode,
    KeepSource,
}

#[derive(Clone)]
struct PreState {
    plan: Plan,
    mode: GpuMode,
    /// Rect of the checked-out input world in layer coordinates.
    input_rect: Rect64,
}

#[derive(Default)]
struct Plugin;

ae::define_effect!(Plugin, (), Params);

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

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _in_data: ae::InData,
        _out_data: ae::OutData,
    ) -> Result<(), ae::Error> {
        params.add(
            Params::BlendMode,
            "Blend Mode",
            ae::PopupDef::setup(|f| {
                f.set_options(&["Normal", "Multiply"]);
                f.set_default(2);
            }),
        )?;
        params.add(
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
        params.add(
            Params::Opacity,
            "Opacity (%)",
            fs(0.0, 100.0, 0.0, 100.0, 75.0, 1),
        )?;
        params.add(
            Params::Angle,
            "Angle",
            ae::AngleDef::setup(|f| {
                f.set_default(120.0);
            }),
        )?;
        params.add(
            Params::Distance,
            "Distance (px)",
            fs(0.0, 1000.0, 0.0, 100.0, 5.0, 1),
        )?;
        params.add(
            Params::Choke,
            "Choke (%)",
            fs(0.0, 100.0, 0.0, 100.0, 0.0, 1),
        )?;
        params.add(
            Params::Size,
            "Size (px)",
            fs(0.0, 250.0, 0.0, 100.0, 5.0, 1),
        )?;
        params.add(
            Params::GpuMode,
            "Render Device",
            ae::PopupDef::setup(|f| {
                f.set_options(&["Auto", "GPU", "CPU"]);
                f.set_default(1);
            }),
        )?;
        params.add(
            Params::KeepSource,
            "Keep Source",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(true);
                f.set_label("Keep Source");
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
                    "onmk Inner Shadow v0.2\rInner shadow with optional source.\rCPU or wgpu compute (Render Device).",
                );
            }
            ae::Command::Render {
                in_layer,
                mut out_layer,
            } => {
                legacy_render(&in_data, params, &in_layer, &mut out_layer)?;
            }
            ae::Command::SmartPreRender { mut extra } => {
                smart_pre_render(&in_data, params, &mut extra)?;
            }
            ae::Command::SmartRender { extra } => {
                smart_render(&in_data, &extra)?;
            }
            // AE-native GPU rendering is not advertised in the PiPL.
            ae::Command::SmartRenderGpu { .. } => return Err(ae::Error::BadCallbackParameter),
            _ => {}
        }
        Ok(())
    }
}

fn blend_from_popup(v: i32) -> Blend {
    if v == 1 {
        Blend::Normal
    } else {
        Blend::Multiply
    }
}

fn mode_from_popup(v: i32) -> GpuMode {
    match v {
        2 => GpuMode::Gpu,
        3 => GpuMode::Cpu,
        _ => GpuMode::Auto,
    }
}

fn read_params(p: &ae::Parameters<Params>) -> Result<(ShadowParams, GpuMode), ae::Error> {
    let f = |id: Params| -> Result<f64, ae::Error> { Ok(p.get(id)?.as_float_slider()?.value()) };
    let color = match p.get(Params::Color)?.as_color()?.float_value() {
        Ok(c) => [c.red, c.green, c.blue],
        Err(_) => {
            let c = p.get(Params::Color)?.as_color()?.value();
            [
                c.red as f32 / 255.0,
                c.green as f32 / 255.0,
                c.blue as f32 / 255.0,
            ]
        }
    };
    let shadow = ShadowParams {
        blend: blend_from_popup(p.get(Params::BlendMode)?.as_popup()?.value() as i32),
        color,
        opacity: f(Params::Opacity)? / 100.0,
        angle_deg: p.get(Params::Angle)?.as_angle()?.value() as f64,
        distance: f(Params::Distance)?,
        choke: f(Params::Choke)? / 100.0,
        size: f(Params::Size)?,
    };
    let mode = mode_from_popup(p.get(Params::GpuMode)?.as_popup()?.value() as i32);
    Ok((shadow, mode))
}

/// Downsample and pixel aspect ratio from the host (in_data getters).
fn host_scale(i: &ae::InData) -> Result<Scale, ae::Error> {
    let (dx, dy, par) = (i.downsample_x(), i.downsample_y(), i.pixel_aspect_ratio());
    Scale::new(
        Scale::ratio(dx.num as i64, dx.den as u64),
        Scale::ratio(dy.num as i64, dy.den as u64),
        Scale::ratio(par.num as i64, par.den as u64),
    )
    .map_err(|_| ae::Error::BadCallbackParameter)
}

fn plan_for(i: &ae::InData, p: &ae::Parameters<Params>) -> Result<(Plan, GpuMode), ae::Error> {
    let (shadow, mode) = read_params(p)?;
    let mut plan =
        Plan::new(&shadow, &host_scale(i)?).map_err(|_| ae::Error::BadCallbackParameter)?;
    plan.keep_source = p.get(Params::KeepSource)?.as_checkbox()?.value();
    Ok((plan, mode))
}

fn rect64(r: ae::Rect) -> Rect64 {
    Rect64::new(r.left as i64, r.top as i64, r.right as i64, r.bottom as i64)
}

fn ae_rect(r: Rect64) -> Result<ae::Rect, ae::Error> {
    let c = |v: i64| i32::try_from(v).map_err(|_| ae::Error::BadCallbackParameter);
    Ok(ae::Rect {
        left: c(r.left)?,
        top: c(r.top)?,
        right: c(r.right)?,
        bottom: c(r.bottom)?,
    })
}

fn to_ae_error(e: engine::RenderError) -> ae::Error {
    match e {
        engine::RenderError::Invalid(_) => ae::Error::BadCallbackParameter,
        engine::RenderError::OutOfMemory => ae::Error::OutOfMemory,
        // Forced GPU failed: report it instead of silently rendering on CPU.
        engine::RenderError::GpuRequired(_) => ae::Error::Generic,
    }
}

/// Runs render work with panics converted to an error (the crate's own
/// catch_panics path would report PF_Err_NONE for a panic).
fn guard(work: impl FnOnce() -> Result<(), ae::Error>) -> Result<(), ae::Error> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(work))
        .unwrap_or(Err(ae::Error::InternalStructDamaged))
}

/// Check the input in exactly once, even on error or panic, without hiding
/// the primary render failure behind a secondary cleanup error.
fn with_input_checkin(
    work: impl FnOnce() -> Result<(), ae::Error>,
    checkin: impl FnOnce() -> Result<(), ae::Error>,
) -> Result<(), ae::Error> {
    let result = guard(work);
    let cleanup = checkin();
    result.and(cleanup)
}

fn render_into(
    img: &engine::Image,
    out: &mut ae::Layer,
    origin: (i64, i64),
    plan: &Plan,
    mode: GpuMode,
) -> Result<(), ae::Error> {
    let (w, h) = (out.width() as i64, out.height() as i64);
    if w == 0 || h == 0 {
        return Ok(());
    }
    let rect = Rect64::new(origin.0, origin.1, origin.0 + w, origin.1 + h);
    let (pixels, _route) = engine::render(img, &rect, plan, mode).map_err(to_ae_error)?;
    world::write_image(out, &pixels)
}

fn legacy_render(
    i: &ae::InData,
    p: &ae::Parameters<Params>,
    input: &ae::Layer,
    output: &mut ae::Layer,
) -> Result<(), ae::Error> {
    guard(|| {
        let (plan, mode) = plan_for(i, p)?;
        // Legacy input is the whole layer at the origin; the output buffer's
        // origin within it is in_data.output_origin.
        let img = world::read_image(input, 0, 0)?;
        let o = i.output_origin();
        render_into(&img, output, (o.h as i64, o.v as i64), &plan, mode)
    })
}

fn smart_pre_render(
    i: &ae::InData,
    p: &ae::Parameters<Params>,
    extra: &mut ae::pf::PreRenderExtra,
) -> Result<(), ae::Error> {
    let (plan, mode) = plan_for(i, p)?;
    let mut req = extra.output_request();
    let out_rect = rect64(req.rect.into());
    req.rect = ae_rect(plan.input_rect(&out_rect))?.into();
    let got = extra.callbacks().checkout_layer(
        0,
        0,
        &req,
        i.current_time(),
        i.time_step(),
        i.time_scale(),
    )?;
    let input_rect = rect64(got.result_rect.into());
    // Both modes stay within the input alpha support.
    extra.set_result_rect(ae_rect(input_rect.intersect(&out_rect))?);
    extra.set_max_result_rect(got.max_result_rect.into());
    extra.set_pre_render_data(PreState {
        plan,
        mode,
        input_rect,
    });
    Ok(())
}

fn smart_render(i: &ae::InData, extra: &ae::pf::SmartRenderExtra) -> Result<(), ae::Error> {
    let state = extra
        .pre_render_data::<PreState>()
        .cloned()
        .ok_or(ae::Error::BadCallbackParameter)?;
    let cb = extra.callbacks();
    let o = i.output_origin();
    let out_origin = (o.h as i64, o.v as i64);
    match cb.checkout_layer_pixels(0)? {
        None => guard(|| match cb.checkout_output()? {
            Some(mut out) => world::write_transparent(&mut out),
            None => Ok(()),
        }),
        Some(input) => with_input_checkin(
            || {
                let Some(mut out) = cb.checkout_output()? else {
                    return Ok(());
                };
                let r = state.input_rect;
                // Never silently misalign: the world must be the checked-out rect.
                if input.width() as i64 != r.width() || input.height() as i64 != r.height() {
                    return Err(ae::Error::BadCallbackParameter);
                }
                let img = world::read_image(&input, r.left, r.top)?;
                render_into(&img, &mut out, out_origin, &state.plan, state.mode)
            },
            || cb.checkin_layer_pixels(0),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parameter_identity_is_append_only() {
        let names: Vec<String> = [
            Params::BlendMode,
            Params::Color,
            Params::Opacity,
            Params::Angle,
            Params::Distance,
            Params::Choke,
            Params::Size,
            Params::GpuMode,
            Params::KeepSource,
        ]
        .iter()
        .map(|p| format!("{p:?}"))
        .collect();
        assert_eq!(
            names,
            [
                "BlendMode",
                "Color",
                "Opacity",
                "Angle",
                "Distance",
                "Choke",
                "Size",
                "GpuMode",
                "KeepSource"
            ]
        );
    }

    #[test]
    fn popup_values_are_one_based() {
        assert_eq!(blend_from_popup(1), Blend::Normal);
        assert_eq!(blend_from_popup(2), Blend::Multiply);
        assert_eq!(mode_from_popup(1), GpuMode::Auto);
        assert_eq!(mode_from_popup(2), GpuMode::Gpu);
        assert_eq!(mode_from_popup(3), GpuMode::Cpu);
        assert_eq!(mode_from_popup(0), GpuMode::Auto);
    }

    #[test]
    fn checkin_runs_once_and_keeps_primary_failure() {
        use std::cell::Cell;
        for cleanup_fails in [false, true] {
            for kind in 0..3 {
                let calls = Cell::new(0);
                let result = with_input_checkin(
                    || match kind {
                        0 => Ok(()),
                        1 => Err(ae::Error::OutOfMemory),
                        _ => panic!("injected render panic"),
                    },
                    || {
                        calls.set(calls.get() + 1);
                        if cleanup_fails {
                            Err(ae::Error::BadCallbackParameter)
                        } else {
                            Ok(())
                        }
                    },
                );
                assert_eq!(calls.get(), 1);
                let expected = match kind {
                    0 if cleanup_fails => Err(ae::Error::BadCallbackParameter),
                    0 => Ok(()),
                    1 => Err(ae::Error::OutOfMemory),
                    _ => Err(ae::Error::InternalStructDamaged),
                };
                assert_eq!(
                    result, expected,
                    "kind={kind} cleanup_fails={cleanup_fails}"
                );
            }
        }
    }

    #[test]
    fn rect_conversion_rejects_i32_overflow() {
        assert!(ae_rect(Rect64::new(0, 0, i32::MAX as i64 + 1, 1)).is_err());
        let r = ae_rect(Rect64::new(-5, -6, 7, 8)).unwrap();
        assert_eq!((r.left, r.top, r.right, r.bottom), (-5, -6, 7, 8));
        assert_eq!(rect64(r), Rect64::new(-5, -6, 7, 8));
    }
}
