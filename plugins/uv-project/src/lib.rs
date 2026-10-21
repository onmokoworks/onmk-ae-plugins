//! UVProject — Adobe After Effects effect plug-in.
//!
//! The applied layer can be either the UV map or the texture. The Other Layer
//! parameter supplies the opposite role. Supports 8, 16 and 32 bpc on CPU.

use after_effects as ae;

mod uv;
use uv::{Image, UvParams, WrapMode};

// ---- Parameter IDs ----

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Texture,
    VOrigin,
    Wrap,
    Interpolation,
    UseUvAlpha,
    Opacity,
    InputMode,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum InputMode {
    UvMap,
    Texture,
    GeneratePlanar,
}

// Added-parameter indices (AE index 0 is the implicit input layer = UV map).
const PARAM_TEXTURE: i32 = 1;
const TEXTURE_CHECKOUT_ID: i32 = 1;

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
        // Kept at index 1 for compatibility with existing projects.
        params.add(
            Params::Texture,
            "Other Layer (Texture / UV Map)",
            ae::LayerDef::setup(|_f| {}),
        )?;

        // index 2 — V axis origin
        params.add(
            Params::VOrigin,
            "V Origin",
            ae::PopupDef::setup(|f| {
                f.set_options(&["Top (After Effects)", "Bottom (Nuke / 3D)"]);
                f.set_default(2); // Bottom — most UV / ST passes
            }),
        )?;

        // index 3 — wrap mode for texture sampling
        params.add(
            Params::Wrap,
            "Wrap",
            ae::PopupDef::setup(|f| {
                f.set_options(&["Clamp", "Repeat", "Mirror"]);
                f.set_default(1); // Clamp
            }),
        )?;

        // index 4 — sampling filter
        params.add(
            Params::Interpolation,
            "Sampling",
            ae::PopupDef::setup(|f| {
                f.set_options(&["Bilinear", "Nearest"]);
                f.set_default(1); // Bilinear
            }),
        )?;

        // index 5 — use UV map alpha as coverage mask
        params.add(
            Params::UseUvAlpha,
            "Use UV Alpha as Mask",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(true);
                f.set_label("Mask");
            }),
        )?;

        // index 6 — overall opacity
        params.add(
            Params::Opacity,
            "Opacity",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_default(100.0);
                f.set_precision(1);
            }),
        )?;

        // Appended to preserve all existing parameter indices.
        params.add(
            Params::InputMode,
            "Input Is",
            ae::PopupDef::setup(|f| {
                f.set_options(&[
                    "UV Map (Other Layer = Texture)",
                    "Texture (Other Layer = UV Map)",
                    "Generate Planar UV Map",
                ]);
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
                    "UVProject v1.2\rUV projection and planar UV-map generation, 8/16/32 bpc.\rWritten in Rust.",
                );
            }
            ae::Command::Render {
                in_layer,
                mut out_layer,
            } => {
                render_cpu(params, &in_data, &in_layer, &mut out_layer)?;
            }
            ae::Command::SmartPreRender { mut extra } => {
                smart_pre_render(&in_data, &mut extra)?;
            }
            ae::Command::SmartRender { extra } => {
                smart_render_cpu(&extra, params)?;
            }
            ae::Command::SmartRenderGpu { extra } => {
                smart_render_cpu(&extra, params)?;
            }
            _ => {}
        }
        Ok(())
    }
}

// ---- Extract parameters ----

fn get_params(params: &ae::Parameters<Params>) -> Result<UvParams, ae::Error> {
    Ok(UvParams {
        v_origin_bottom: params.get(Params::VOrigin)?.as_popup()?.value() == 2,
        wrap: WrapMode::from_popup(params.get(Params::Wrap)?.as_popup()?.value()),
        bilinear: params.get(Params::Interpolation)?.as_popup()?.value() == 1,
        use_uv_alpha: params.get(Params::UseUvAlpha)?.as_checkbox()?.value(),
        opacity: params
            .get(Params::Opacity)?
            .as_float_slider()?
            .value()
            .clamp(0.0, 100.0) as f32
            / 100.0,
    })
}

fn input_mode(params: &ae::Parameters<Params>) -> Result<InputMode, ae::Error> {
    Ok(match params.get(Params::InputMode)?.as_popup()?.value() {
        2 => InputMode::Texture,
        3 => InputMode::GeneratePlanar,
        _ => InputMode::UvMap,
    })
}

// ---- Layer <-> normalized float image helpers ----

fn pixel_depth(layer: &ae::Layer) -> Result<i16, ae::Error> {
    // Query the documented world format first. Layer::bit_depth() in the
    // pinned SDK wrapper short-circuits to 16 for DEEP worlds, even when the
    // World Suite identifies an ARGB128 float allocation.
    depth_from_format(layer.pixel_format()?)
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
fn explicit_world_formats_determine_channel_width() {
    assert_eq!(depth_from_format(ae::PixelFormat::Argb32).unwrap(), 8);
    assert_eq!(depth_from_format(ae::PixelFormat::Argb64).unwrap(), 16);
    assert_eq!(depth_from_format(ae::PixelFormat::Argb128).unwrap(), 32);
    // A CPU accessor must never dereference a GPU world as host pixels.
    assert!(depth_from_format(ae::PixelFormat::GpuBgra128).is_err());
}

fn checked_world_access(w: usize, h: usize, row_bytes: isize, depth: i16, address: usize) -> Result<(), ae::Error> {
    let component = match depth { 8 => 1, 16 => 2, 32 => 4, _ => return Err(ae::Error::BadCallbackParameter) };
    if w > i32::MAX as usize || h > i32::MAX as usize || row_bytes == i32::MIN as isize {
        return Err(ae::Error::BadCallbackParameter);
    }
    let stride = row_bytes.checked_abs().ok_or(ae::Error::BadCallbackParameter)? as usize;
    let row = w.checked_mul(4 * component).ok_or(ae::Error::OutOfMemory)?;
    let extent = stride.checked_mul(h).ok_or(ae::Error::OutOfMemory)?;
    if extent > isize::MAX as usize { return Err(ae::Error::OutOfMemory); }
    if w != 0 && h != 0 && (stride < row || address == 0 || address % component != 0 || stride % component != 0) {
        return Err(ae::Error::BadCallbackParameter);
    }
    Ok(())
}

fn validate_layer_access(layer: &ae::Layer, depth: i16) -> Result<(), ae::Error> {
    checked_world_access(layer.width(),layer.height(),layer.row_bytes(),depth,unsafe {layer.data_ptr()} as usize)
}

#[test]
fn world_access_rejects_short_unaligned_null_and_overflow_layouts() {
    for (depth, component) in [(8,1),(16,2),(32,4)] {
        for sign in [-1,1] {
            let stride = (17 * 4 * component + 8) as isize * sign;
            assert!(checked_world_access(17,31,stride,depth,16).is_ok());
            assert!(checked_world_access(17,31,(17*4*component-1) as isize * sign,depth,16).is_err());
        }
        assert!(checked_world_access(1,1,16,depth,0).is_err());
        if component > 1 {
            assert!(checked_world_access(1,1,16,depth,17).is_err());
            assert!(checked_world_access(1,1,17,depth,16).is_err());
        }
        assert!(checked_world_access(0,0,0,depth,0).is_ok());
        assert!(checked_world_access(1,2,isize::MAX,depth,16).is_err());
        assert!(checked_world_access(1,1,i32::MIN as isize,depth,16).is_err());
        assert!(checked_world_access(usize::MAX,1,16,depth,16).is_err());
    }
}

fn layer_to_image(layer: &ae::Layer) -> Result<Image, ae::Error> {
    let depth = pixel_depth(layer)?;
    validate_layer_access(layer,depth)?;
    let w = layer.width() as usize;
    let h = layer.height() as usize;
    let mut image = Image::transparent(w, h)?;
    for y in 0..h {
        for x in 0..w {
            image.pixels[y * w + x] = match depth {
                8 => {
                    let px = layer.as_pixel8(x, y);
                    [
                        px.alpha as f32 / 255.0,
                        px.red as f32 / 255.0,
                        px.green as f32 / 255.0,
                        px.blue as f32 / 255.0,
                    ]
                }
                16 => {
                    let px = layer.as_pixel16(x, y);
                    let max = ae::MAX_CHANNEL16 as f32;
                    [
                        px.alpha as f32 / max,
                        px.red as f32 / max,
                        px.green as f32 / max,
                        px.blue as f32 / max,
                    ]
                }
                32 => {
                    let px = layer.as_pixel32(x, y);
                    [px.alpha, px.red, px.green, px.blue]
                }
                _ => return Err(ae::Error::BadCallbackParameter),
            };
        }
    }
    Ok(image)
}

fn image_to_layer(image: &Image, layer: &mut ae::Layer) -> Result<(), ae::Error> {
    image.validate()?;
    let depth = pixel_depth(layer)?;
    validate_layer_access(layer,depth)?;
    if image.width != layer.width() || image.height != layer.height() {
        return Err(ae::Error::BadCallbackParameter);
    }
    let w = image.width;
    let h = image.height;
    for y in 0..h {
        for x in 0..w {
            let px = image.pixels[y * image.width + x];
            match depth {
                8 => {
                    let dst = layer.as_pixel8_mut(x, y);
                    dst.alpha = (px[0] * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
                    dst.red = (px[1] * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
                    dst.green = (px[2] * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
                    dst.blue = (px[3] * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
                }
                16 => {
                    let max = ae::MAX_CHANNEL16 as f32;
                    let dst = layer.as_pixel16_mut(x, y);
                    dst.alpha = (px[0] * max + 0.5).clamp(0.0, max) as u16;
                    dst.red = (px[1] * max + 0.5).clamp(0.0, max) as u16;
                    dst.green = (px[2] * max + 0.5).clamp(0.0, max) as u16;
                    dst.blue = (px[3] * max + 0.5).clamp(0.0, max) as u16;
                }
                32 => {
                    let dst = layer.as_pixel32_mut(x, y);
                    dst.alpha = px[0];
                    dst.red = px[1];
                    dst.green = px[2];
                    dst.blue = px[3];
                }
                _ => return Err(ae::Error::BadCallbackParameter),
            }
        }
    }
    Ok(())
}

// ---- Legacy Render ----

fn render_cpu(
    params: &ae::Parameters<Params>,
    in_data: &ae::InData,
    in_layer: &ae::Layer,
    out_layer: &mut ae::Layer,
) -> Result<(), ae::Error> {
    let p = get_params(params)?;
    if input_mode(params)? == InputMode::GeneratePlanar {
        let result = uv::generate_planar(out_layer.width(), out_layer.height(), p.v_origin_bottom)?;
        return image_to_layer(&result, out_layer);
    }
    let input = layer_to_image(in_layer)?;
    let other = get_other_layer(params, in_data)?;
    let result = match other {
        Some(other) if input_mode(params)? == InputMode::Texture => {
            uv::project(&p, &other, &input, out_layer.width(), out_layer.height())
        }
        Some(other) => uv::project(&p, &input, &other, out_layer.width(), out_layer.height()),
        None => Image::transparent(out_layer.width(), out_layer.height()),
    }?;
    image_to_layer(&result, out_layer)
}

fn get_other_layer(params: &ae::Parameters<Params>, in_data: &ae::InData) -> Result<Option<Image>, ae::Error> {
    let Ok(checkout) = params.checkout_at(Params::Texture, Some(in_data.current_time()), None, None) else { return Ok(None); };
    let layer_def = checkout.as_layer()?;
    let Some(layer) = layer_def.value() else { return Ok(None); };
    layer_to_image(&layer).map(Some)
}

// ---- SmartFX ----

fn smart_pre_render(
    in_data: &ae::InData,
    extra: &mut ae::pf::PreRenderExtra,
) -> Result<(), ae::Error> {
    let req = extra.output_request();
    let cb = extra.callbacks();

    // Input layer 0 = the UV map; it defines the output geometry.
    let in_result = cb.checkout_layer(
        0,
        0,
        &req,
        in_data.current_time(),
        in_data.time_step(),
        in_data.time_scale(),
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

    // Texture layer (param index 1). Failure is non-fatal: we render transparent.
    let _ = cb.checkout_layer(
        PARAM_TEXTURE,
        TEXTURE_CHECKOUT_ID,
        &req,
        in_data.current_time(),
        in_data.time_step(),
        in_data.time_scale(),
    );

    Ok(())
}

fn smart_render_cpu(
    extra: &ae::pf::SmartRenderExtra,
    params: &ae::Parameters<Params>,
) -> Result<(), ae::Error> {
    let p = get_params(params)?;
    let cb = extra.callbacks();

    let input_world = cb.checkout_layer_pixels(0)?.ok_or(ae::Error::Generic)?;
    let other_acquired = std::cell::Cell::new(false);
    with_render_cleanup(|| {
    let mut output_world = cb.checkout_output()?.ok_or(ae::Error::Generic)?;
    let input = layer_to_image(&input_world)?;
    let out_w = output_world.width();
    let out_h = output_world.height();

    let result = if input_mode(params)? == InputMode::GeneratePlanar {
        uv::generate_planar(out_w, out_h, p.v_origin_bottom)
    } else if let Ok(Some(other_world)) = cb.checkout_layer_pixels(TEXTURE_CHECKOUT_ID as u32) {
        other_acquired.set(true);
        let other = layer_to_image(&other_world)?;
        if input_mode(params)? == InputMode::Texture {
            uv::project(&p, &other, &input, out_w, out_h)
        } else {
            uv::project(&p, &input, &other, out_w, out_h)
        }
    } else {
        Image::transparent(out_w, out_h)
    }?;
    image_to_layer(&result, &mut output_world)
    }, || {
        let other = if other_acquired.get() {
            cb.checkin_layer_pixels(TEXTURE_CHECKOUT_ID as u32)
        } else { Ok(()) };
        let input = cb.checkin_layer_pixels(0);
        other.and(input)
    })
}

fn with_render_cleanup<T>(work: impl FnOnce() -> Result<T, ae::Error>, cleanup: impl FnOnce() -> Result<(), ae::Error>) -> Result<T, ae::Error> {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work));
    let released = cleanup();
    match result {
        Ok(Ok(value)) => released.map(|()| value),
        Ok(Err(error)) => Err(error),
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

#[test]
fn render_cleanup_preserves_errors_and_runs_before_unwind() {
    for work_fails in [false,true] {
        for cleanup_fails in [false,true] {
            let calls = std::cell::Cell::new(0);
            let work = if work_fails { Err(ae::Error::OutOfMemory) } else { Ok(()) };
            let cleanup = if cleanup_fails { Err(ae::Error::BadCallbackParameter) } else { Ok(()) };
            assert_eq!(with_render_cleanup(||work,||{calls.set(calls.get()+1);cleanup}),work.and(cleanup));
            assert_eq!(calls.get(),1);
        }
    }
    let called = std::cell::Cell::new(false);
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _: Result<(),ae::Error> = with_render_cleanup(||panic!("render"),||{called.set(true);Ok(())});
    }));
    assert!(panic.is_err());
    assert!(called.get());
}
