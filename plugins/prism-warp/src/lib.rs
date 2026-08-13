use after_effects as ae;

mod distort;

use distort::ImageF32;

// ---- Parameter IDs ----
//
// The discriminants double as the parameter indices After Effects hands us in
// SmartPreRender: index 0 is the effect's input layer, and each parameter added in
// `params_setup` takes the next index. Keep the `params.add` calls below in the same
// order as this enum, or the checkout indices will point at the wrong parameter.
#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Strength = 1,
    Dispersion,
    BlurLens,
    Threshold,
    ThresholdSmooth,
    RotateWarpDir,
    Quality,
    LensLayer,
    MatteLayer,
    InvertMatte,
    Mix,
}

impl Params {
    /// Parameter index as seen by the AE callbacks.
    const fn index(self) -> i32 {
        self as i32
    }
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
        params.add(
            Params::Strength,
            "Strength (px)",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(-500.0);
                f.set_valid_max(500.0);
                f.set_slider_min(-100.0);
                f.set_slider_max(100.0);
                f.set_default(10.0);
                f.set_precision(1);
            }),
        )?;

        params.add(
            Params::Dispersion,
            "Dispersion (px)",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(250.0);
                f.set_slider_min(0.0);
                f.set_slider_max(50.0);
                f.set_default(5.0);
                f.set_precision(1);
            }),
        )?;

        params.add(
            Params::BlurLens,
            "Lens Blur",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(500.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_default(20.0);
                f.set_precision(1);
            }),
        )?;

        params.add(
            Params::Threshold,
            "Edge Threshold",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(50.0);
                f.set_default(0.0);
                f.set_precision(2);
            }),
        )?;

        params.add(
            Params::ThresholdSmooth,
            "Edge Softness",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(50.0);
                f.set_default(5.0);
                f.set_precision(2);
            }),
        )?;

        params.add(
            Params::RotateWarpDir,
            "Direction",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(-360.0);
                f.set_valid_max(360.0);
                f.set_slider_min(-180.0);
                f.set_slider_max(180.0);
                f.set_default(0.0);
                f.set_precision(1);
            }),
        )?;

        params.add(
            Params::Quality,
            "Quality",
            ae::PopupDef::setup(|f| {
                f.set_options(&["Draft", "Normal", "High"]);
                f.set_default(2);
            }),
        )?;

        params.add(Params::LensLayer, "Lens", ae::LayerDef::setup(|_f| {}))?;

        params.add(Params::MatteLayer, "Matte", ae::LayerDef::setup(|_f| {}))?;

        params.add(
            Params::InvertMatte,
            "Invert Matte",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(false);
                f.set_label("Invert");
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

        Ok(())
    }

    /// Panic firewall.
    ///
    /// `handle_command` is called from inside an `extern "C"` boundary. A panic trying
    /// to cross that boundary hits `panic_cannot_unwind` and aborts the process, which
    /// means any bug in this plug-in takes After Effects down with it. Catching here
    /// turns it into an error After Effects can report while staying alive.
    ///
    /// This requires `panic = "unwind"` (the default); `panic = "abort"` would make
    /// `catch_unwind` useless.
    ///
    /// Note this only covers `handle_command`. `build.rs` additionally emits
    /// `--cfg catch_panics`, which enables the same protection around the whole
    /// `EffectMain` entry point inside the `after-effects` crate, covering setup and
    /// teardown commands too. That outer handler returns `PF_Err_NONE`, so catching
    /// here as well is what lets AE see an actual error code for render-time panics.
    fn handle_command(
        &mut self,
        cmd: ae::Command,
        in_data: ae::InData,
        out_data: ae::OutData,
        params: &mut ae::Parameters<Params>,
    ) -> Result<(), ae::Error> {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.handle_command_inner(cmd, in_data, out_data, params)
        }));
        match result {
            Ok(inner) => inner,
            Err(_) => Err(ae::Error::InternalStructDamaged),
        }
    }
}

impl Plugin {
    fn handle_command_inner(
        &mut self,
        cmd: ae::Command,
        in_data: ae::InData,
        mut out_data: ae::OutData,
        params: &mut ae::Parameters<Params>,
    ) -> Result<(), ae::Error> {
        match cmd {
            ae::Command::About => {
                out_data.set_return_msg(
                    "PrismWarp v1.0\rLens-driven prism and refraction warp.\rWritten in Rust.",
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

            // No GPU implementation exists. The PiPL no longer advertises
            // `SupportsGpuRenderF32`, so this should never arrive; if it does, refuse
            // rather than run the CPU path over a world whose pixels may live in VRAM
            // and are not reachable through `buffer()`.
            ae::Command::SmartRenderGpu { .. } => {
                return Err(ae::Error::BadCallbackParameter);
            }

            _ => {}
        }
        Ok(())
    }
}

// ---- Extract parameters ----

pub struct DistortParams {
    pub strength: f64,
    pub dispersion: f64,
    pub blur_lens: f64,
    pub threshold: f64,
    pub threshold_smooth: f64,
    pub rotate_warp_dir: f64,
    pub steps: usize,
    pub invert_matte: bool,
    pub mix: f64,
}

fn get_params(params: &ae::Parameters<Params>) -> Result<DistortParams, ae::Error> {
    Ok(DistortParams {
        strength: params.get(Params::Strength)?.as_float_slider()?.value(),
        dispersion: params
            .get(Params::Dispersion)?
            .as_float_slider()?
            .value()
            .max(0.0),
        blur_lens: params
            .get(Params::BlurLens)?
            .as_float_slider()?
            .value()
            .max(0.0),
        threshold: params
            .get(Params::Threshold)?
            .as_float_slider()?
            .value()
            .clamp(0.0, 100.0)
            / 100.0,
        threshold_smooth: params
            .get(Params::ThresholdSmooth)?
            .as_float_slider()?
            .value()
            .clamp(0.0, 100.0)
            / 100.0,
        rotate_warp_dir: params
            .get(Params::RotateWarpDir)?
            .as_float_slider()?
            .value(),
        steps: match params.get(Params::Quality)?.as_popup()?.value() {
            1 => 5,
            3 => 17,
            _ => 9,
        },
        invert_matte: params.get(Params::InvertMatte)?.as_checkbox()?.value(),
        mix: params
            .get(Params::Mix)?
            .as_float_slider()?
            .value()
            .clamp(0.0, 100.0)
            / 100.0,
    })
}

// ---- Layer <-> f32 image conversion ----
//
// After Effects hands us 8, 16 or 32 bit worlds depending on the project's colour
// depth. All three use the same [alpha, red, green, blue] channel order; only the
// storage type and nominal range differ. Everything above this boundary works in
// normalized f32.

/// 16bpc worlds are 0..32768, not 0..65535 (PF_MAX_CHAN16).
const MAX_CHAN16: f32 = 32768.0;
const MAX_CHAN8: f32 = 255.0;

/// Number of bytes each pixel occupies in a world of the given bit depth.
fn bytes_per_pixel(bit_depth: i16) -> Option<usize> {
    match bit_depth {
        8 => Some(4),
        16 => Some(8),
        32 => Some(16),
        _ => None,
    }
}

/// Geometry of a layer's pixel buffer, validated to the point where `Layer::buffer()`
/// is safe to call.
struct LayerGeometry {
    w: usize,
    h: usize,
    stride: usize,
    bpp: usize,
    bit_depth: i16,
    /// A negative `rowbytes` means `buffer()` returns rows bottom-up, so row `y` of
    /// the image lives at offset `(h - 1 - y) * stride`.
    bottom_up: bool,
}

/// Validate a layer before touching its pixels.
///
/// `Layer::buffer()` asserts on `rowbytes == 0` and on a null data pointer. Those
/// assertions fire inside the `extern "C"` boundary, so an empty world reaching this
/// far would otherwise abort the host.
fn layer_geometry(layer: &ae::Layer) -> Result<LayerGeometry, ae::Error> {
    let w = layer.width();
    let h = layer.height();
    let stride = layer.buffer_stride();
    let bit_depth = layer.bit_depth();
    let bpp = bytes_per_pixel(bit_depth).ok_or(ae::Error::BadCallbackParameter)?;

    if w == 0 || h == 0 || stride == 0 {
        return Err(ae::Error::InternalStructDamaged);
    }
    if unsafe { layer.data_ptr() }.is_null() {
        return Err(ae::Error::InternalStructDamaged);
    }
    // The row must actually fit in the stride the layer advertises.
    let row_bytes = w.checked_mul(bpp).ok_or(ae::Error::OutOfMemory)?;
    if stride < row_bytes {
        return Err(ae::Error::InternalStructDamaged);
    }
    distort::checked_pixel_count(w, h).ok_or(ae::Error::OutOfMemory)?;

    Ok(LayerGeometry {
        w,
        h,
        stride,
        bpp,
        bit_depth,
        bottom_up: layer.row_bytes() < 0,
    })
}

impl LayerGeometry {
    /// Byte offset of image row `y` within the slice returned by `buffer()`,
    /// or `None` if `y` is outside the layer.
    fn row_offset(&self, y: usize) -> Option<usize> {
        if y >= self.h {
            return None;
        }
        let row = if self.bottom_up { self.h - 1 - y } else { y };
        Some(row * self.stride)
    }
}

/// Copy a layer's pixels into a normalized f32 image.
fn layer_to_image(layer: &ae::Layer) -> Result<ImageF32, ae::Error> {
    let g = layer_geometry(layer)?;
    let mut img = ImageF32::new_zeroed(g.w, g.h)?;
    let buf = layer.buffer();
    let row_bytes = g.w * g.bpp;

    for y in 0..g.h {
        let src_off = match g.row_offset(y) {
            Some(off) if off + row_bytes <= buf.len() => off,
            _ => continue,
        };
        let src_row = &buf[src_off..src_off + row_bytes];
        let dst_row = &mut img.data[y * g.w * 4..(y + 1) * g.w * 4];
        decode_row(src_row, g.bit_depth, dst_row)?;
    }

    Ok(img)
}

/// Decode one row of host pixels into normalized f32 channel values.
///
/// `src` must hold at least `dst.len()` channels at the given depth.
fn decode_row(src: &[u8], bit_depth: i16, dst: &mut [f32]) -> Result<(), ae::Error> {
    match bit_depth {
        8 => {
            for (dst, src) in dst.iter_mut().zip(src.iter()) {
                *dst = *src as f32 / MAX_CHAN8;
            }
        }
        16 => {
            for (i, dst) in dst.iter_mut().enumerate() {
                let b = i * 2;
                // from_ne_bytes rather than a pointer cast: the row is not
                // guaranteed to be u16-aligned.
                let v = u16::from_ne_bytes([src[b], src[b + 1]]);
                *dst = v as f32 / MAX_CHAN16;
            }
        }
        32 => {
            for (i, dst) in dst.iter_mut().enumerate() {
                let b = i * 4;
                *dst = f32::from_ne_bytes([src[b], src[b + 1], src[b + 2], src[b + 3]]);
            }
        }
        _ => return Err(ae::Error::BadCallbackParameter),
    }
    Ok(())
}

/// Encode normalized f32 channel values into one row of host pixels.
///
/// 8 and 16 bit destinations round and clamp; 32 bit passes values through so HDR
/// content survives the round trip.
fn encode_row(dst: &mut [u8], bit_depth: i16, src: &[f32]) {
    match bit_depth {
        8 => {
            for (dst, src) in dst.iter_mut().zip(src.iter()) {
                *dst = (src * MAX_CHAN8 + 0.5).clamp(0.0, MAX_CHAN8) as u8;
            }
        }
        16 => {
            for (i, src) in src.iter().enumerate() {
                let v = (src * MAX_CHAN16 + 0.5).clamp(0.0, MAX_CHAN16) as u16;
                let b = i * 2;
                dst[b..b + 2].copy_from_slice(&v.to_ne_bytes());
            }
        }
        32 => {
            for (i, src) in src.iter().enumerate() {
                let b = i * 4;
                dst[b..b + 4].copy_from_slice(&src.to_ne_bytes());
            }
        }
        _ => {}
    }
}

/// Write one row of normalized f32 pixels back into a layer, converting to its depth.
fn write_row_to_layer(buf: &mut [u8], g: &LayerGeometry, y: usize, row: &[f32]) {
    let row_bytes = g.w * g.bpp;
    let dst_off = match g.row_offset(y) {
        Some(off) if off + row_bytes <= buf.len() => off,
        _ => return,
    };
    // A shorter row than the destination is possible when the output world is wider
    // than the input; write what we have rather than nothing.
    let vals = (g.w * 4).min(row.len());
    encode_row(
        &mut buf[dst_off..dst_off + row_bytes],
        g.bit_depth,
        &row[..vals],
    );
}

// ---- Legacy Render path ----

fn render_cpu(
    params: &ae::Parameters<Params>,
    in_data: &ae::InData,
    in_layer: &ae::Layer,
    out_layer: &mut ae::Layer,
) -> Result<(), ae::Error> {
    let dp = get_params(params)?;
    let src = layer_to_image(in_layer)?;
    let (w, h) = (src.w, src.h);

    let lens = get_layer_image(params, Params::LensLayer, in_data, w, h);
    let matte = get_layer_image(params, Params::MatteLayer, in_data, w, h);

    let out_geom = layer_geometry(out_layer)?;
    let out_buf = out_layer.buffer_mut();

    distort::prism_warp(&dp, &src, lens.as_ref(), matte.as_ref(), |y, row| {
        write_row_to_layer(out_buf, &out_geom, y, row);
    })
}

/// Check out an auxiliary layer parameter and bring it to the render size.
///
/// Failure is not fatal: the effect just runs without that layer.
fn get_layer_image(
    params: &ae::Parameters<Params>,
    param_id: Params,
    in_data: &ae::InData,
    w: usize,
    h: usize,
) -> Option<ImageF32> {
    let checkout = params
        .checkout_at(param_id, Some(in_data.current_time()), None, None)
        .ok()?;
    let layer_def = checkout.as_layer().ok()?;
    let layer = layer_def.value()?;
    let img = layer_to_image(&layer).ok()?;
    if img.w == w && img.h == h {
        Some(img)
    } else {
        img.resampled_to(w, h).ok()
    }
}

// ---- SmartFX PreRender ----

const LENS_CHECKOUT_ID: u32 = 1;
const MATTE_CHECKOUT_ID: u32 = 2;

/// Which auxiliary layers PreRender managed to check out.
///
/// Asking for pixels in SmartRender for an ID that PreRender did not check out is not
/// guaranteed to work, so the outcome has to be carried across.
#[derive(Default, Clone, Copy)]
struct PreRenderState {
    lens_checked_out: bool,
    matte_checked_out: bool,
}

fn smart_pre_render(
    in_data: &ae::InData,
    extra: &mut ae::pf::PreRenderExtra,
) -> Result<(), ae::Error> {
    let req = extra.output_request();
    let cb = extra.callbacks();

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

    let state = PreRenderState {
        lens_checked_out: cb
            .checkout_layer(
                Params::LensLayer.index(),
                LENS_CHECKOUT_ID as i32,
                &req,
                in_data.current_time(),
                in_data.time_step(),
                in_data.time_scale(),
            )
            .is_ok(),
        matte_checked_out: cb
            .checkout_layer(
                Params::MatteLayer.index(),
                MATTE_CHECKOUT_ID as i32,
                &req,
                in_data.current_time(),
                in_data.time_step(),
                in_data.time_scale(),
            )
            .is_ok(),
    };

    extra.set_pre_render_data(state);

    Ok(())
}

// ---- SmartFX CPU Render ----

fn smart_render_cpu(
    extra: &ae::pf::SmartRenderExtra,
    params: &ae::Parameters<Params>,
) -> Result<(), ae::Error> {
    let cb = extra.callbacks();
    let state = extra
        .pre_render_data::<PreRenderState>()
        .copied()
        .unwrap_or_default();

    // Track what actually got checked out so every early return still checks back in.
    // Dropping out without checking in leaks the host's buffers.
    let mut input_checked_out = false;
    let mut lens_checked_out = false;
    let mut matte_checked_out = false;

    let result = (|| -> Result<(), ae::Error> {
        let dp = get_params(params)?;

        let input_world = cb.checkout_layer_pixels(0)?.ok_or(ae::Error::Generic)?;
        input_checked_out = true;

        let src = layer_to_image(&input_world)?;
        let (w, h) = (src.w, src.h);

        let lens = if state.lens_checked_out {
            let img = checkout_layer_image(&cb, LENS_CHECKOUT_ID, w, h);
            lens_checked_out = img.is_some();
            img.flatten()
        } else {
            None
        };
        let matte = if state.matte_checked_out {
            let img = checkout_layer_image(&cb, MATTE_CHECKOUT_ID, w, h);
            matte_checked_out = img.is_some();
            img.flatten()
        } else {
            None
        };

        let mut output_world = cb.checkout_output()?.ok_or(ae::Error::Generic)?;
        let out_geom = layer_geometry(&output_world)?;
        let out_buf = output_world.buffer_mut();

        distort::prism_warp(&dp, &src, lens.as_ref(), matte.as_ref(), |y, row| {
            write_row_to_layer(out_buf, &out_geom, y, row);
        })
    })();

    if input_checked_out {
        let _ = cb.checkin_layer_pixels(0);
    }
    if lens_checked_out {
        let _ = cb.checkin_layer_pixels(LENS_CHECKOUT_ID);
    }
    if matte_checked_out {
        let _ = cb.checkin_layer_pixels(MATTE_CHECKOUT_ID);
    }

    result
}

/// Returns `Some(_)` when the ID was checked out (so the caller must check it back in),
/// with the inner `Option` holding the image if one could be built from it.
fn checkout_layer_image(
    cb: &ae::pf::SmartRenderCallbacks,
    checkout_id: u32,
    w: usize,
    h: usize,
) -> Option<Option<ImageF32>> {
    let world = cb.checkout_layer_pixels(checkout_id).ok()?;
    let world = match world {
        Some(w) => w,
        // Checked out fine but there are no pixels (e.g. an empty adjustment layer).
        None => return Some(None),
    };
    let img = match layer_to_image(&world) {
        Ok(img) => img,
        Err(_) => return Some(None),
    };
    if img.w == w && img.h == h {
        Some(Some(img))
    } else {
        Some(img.resampled_to(w, h).ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The parameter enum's discriminants are used directly as AE parameter indices,
    /// so they must start at 1 (index 0 is the input layer) and stay contiguous in the
    /// same order `params_setup` adds them.
    #[test]
    fn param_indices_match_setup_order() {
        let order = [
            Params::Strength,
            Params::Dispersion,
            Params::BlurLens,
            Params::Threshold,
            Params::ThresholdSmooth,
            Params::RotateWarpDir,
            Params::Quality,
            Params::LensLayer,
            Params::MatteLayer,
            Params::InvertMatte,
            Params::Mix,
        ];
        for (i, p) in order.iter().enumerate() {
            assert_eq!(p.index(), i as i32 + 1, "{p:?} has the wrong index");
        }
        assert_eq!(Params::LensLayer.index(), 8);
        assert_eq!(Params::MatteLayer.index(), 9);
    }

    #[test]
    fn bytes_per_pixel_covers_supported_depths() {
        assert_eq!(bytes_per_pixel(8), Some(4));
        assert_eq!(bytes_per_pixel(16), Some(8));
        assert_eq!(bytes_per_pixel(32), Some(16));
        // Premiere's 10/12 bit formats are not supported by this effect.
        assert_eq!(bytes_per_pixel(10), None);
        assert_eq!(bytes_per_pixel(0), None);
    }

    /// Decoding then encoding must return the original bytes for every depth.
    #[test]
    fn row_codec_round_trips() {
        // 8bpc: every representable value.
        let src8: Vec<u8> = (0..=255u8).collect();
        let mut mid = vec![0.0f32; src8.len()];
        decode_row(&src8, 8, &mut mid).unwrap();
        let mut back = vec![0u8; src8.len()];
        encode_row(&mut back, 8, &mid);
        assert_eq!(src8, back);

        // 16bpc: endpoints, midpoint, and a few arbitrary values, all <= 32768.
        let vals16: Vec<u16> = vec![0, 1, 1234, 16384, 32767, 32768, 300, 9999];
        let src16: Vec<u8> = vals16.iter().flat_map(|v| v.to_ne_bytes()).collect();
        let mut mid = vec![0.0f32; vals16.len()];
        decode_row(&src16, 16, &mut mid).unwrap();
        let mut back = vec![0u8; src16.len()];
        encode_row(&mut back, 16, &mid);
        assert_eq!(src16, back);

        // 32bpc: including out-of-range HDR values, which must not be clamped.
        let vals32: Vec<f32> = vec![0.0, 1.0, 0.5, -0.25, 7.75, 1e-6];
        let src32: Vec<u8> = vals32.iter().flat_map(|v| v.to_ne_bytes()).collect();
        let mut mid = vec![0.0f32; vals32.len()];
        decode_row(&src32, 32, &mut mid).unwrap();
        assert_eq!(mid, vals32);
        let mut back = vec![0u8; src32.len()];
        encode_row(&mut back, 32, &mid);
        assert_eq!(src32, back);
    }

    /// 16bpc worlds run 0..32768. Treating them as 0..65535 would halve every value.
    #[test]
    fn sixteen_bit_white_is_32768() {
        let src: Vec<u8> = 32768u16.to_ne_bytes().to_vec();
        let mut out = [0.0f32; 1];
        decode_row(&src, 16, &mut out).unwrap();
        assert!((out[0] - 1.0).abs() < 1e-6, "got {}", out[0]);
    }

    #[test]
    fn encode_clamps_integer_depths_but_not_float() {
        let over = [2.0f32, -1.0];
        let mut out8 = [0u8; 2];
        encode_row(&mut out8, 8, &over);
        assert_eq!(out8, [255, 0]);

        let mut out32 = [0u8; 8];
        encode_row(&mut out32, 32, &over);
        assert_eq!(f32::from_ne_bytes(out32[0..4].try_into().unwrap()), 2.0);
        assert_eq!(f32::from_ne_bytes(out32[4..8].try_into().unwrap()), -1.0);
    }

    fn geom(w: usize, h: usize, bit_depth: i16, bottom_up: bool) -> LayerGeometry {
        let bpp = bytes_per_pixel(bit_depth).unwrap();
        LayerGeometry {
            w,
            h,
            stride: w * bpp,
            bpp,
            bit_depth,
            bottom_up,
        }
    }

    /// A negative rowbytes means `buffer()` hands back rows bottom-up, so image row 0
    /// lives at the end of the slice.
    #[test]
    fn bottom_up_layers_write_in_reverse() {
        let g = geom(1, 3, 8, true);
        let mut buf = vec![0u8; 3 * 4];
        write_row_to_layer(&mut buf, &g, 0, &[1.0, 1.0, 1.0, 1.0]);
        assert_eq!(&buf[8..12], &[255, 255, 255, 255]);
        assert_eq!(&buf[0..8], &[0; 8]);

        let g = geom(1, 3, 8, false);
        let mut buf = vec![0u8; 3 * 4];
        write_row_to_layer(&mut buf, &g, 0, &[1.0, 1.0, 1.0, 1.0]);
        assert_eq!(&buf[0..4], &[255, 255, 255, 255]);
    }

    /// Out-of-range rows and short source rows must be dropped, not indexed.
    #[test]
    fn write_row_rejects_out_of_range() {
        let g = geom(2, 2, 16, false);
        let mut buf = vec![0u8; 2 * 2 * 8];
        write_row_to_layer(&mut buf, &g, 5, &[1.0; 8]);
        assert!(
            buf.iter().all(|b| *b == 0),
            "row past the end must be ignored"
        );

        // A row shorter than the destination writes its prefix and leaves the rest.
        write_row_to_layer(&mut buf, &g, 0, &[1.0; 4]);
        assert_eq!(&buf[0..8], &32768u16.to_ne_bytes().repeat(4)[..]);
        assert!(buf[8..16].iter().all(|b| *b == 0));
    }
}
