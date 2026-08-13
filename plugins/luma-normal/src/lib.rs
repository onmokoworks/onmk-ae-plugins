use after_effects as ae;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Strength,
    Blur,
    DetailScale,
    InvertX,
    InvertY,
    AlphaMode,
}

#[derive(Clone, Copy)]
struct NormalParams {
    strength: f32,
    blur: usize,
    detail_scale: usize,
    invert_x: bool,
    invert_y: bool,
    alpha_source: bool,
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
        params.add(Params::Strength, "Strength", slider(0.0, 20.0, 4.0, 2))?;
        params.add(
            Params::Blur,
            "Pre-Blur",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(100);
                f.set_slider_min(0);
                f.set_slider_max(30);
                f.set_default(1);
            }),
        )?;
        params.add(
            Params::DetailScale,
            "Detail Scale",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(1);
                f.set_valid_max(16);
                f.set_slider_min(1);
                f.set_slider_max(8);
                f.set_default(1);
            }),
        )?;
        params.add(Params::InvertX, "Invert X", checkbox(false, "Invert X"))?;
        params.add(Params::InvertY, "Invert Y", checkbox(true, "DirectX / AE"))?;
        params.add(
            Params::AlphaMode,
            "Alpha",
            ae::PopupDef::setup(|f| {
                f.set_options(&["Opaque", "Source Alpha"]);
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
            ae::Command::About => out_data
                .set_return_msg("LumaNormal v1.0\rNormal maps from luminance.\rWritten in Rust."),
            ae::Command::Render {
                in_layer,
                mut out_layer,
            } => render(&read_params(params)?, &in_layer, &mut out_layer)?,
            ae::Command::SmartPreRender { mut extra } => pre_render(&in_data, &mut extra)?,
            ae::Command::SmartRender { extra } => smart_render(&extra, params)?,
            ae::Command::SmartRenderGpu { .. } => return Err(ae::Error::BadCallbackParameter),
            _ => {}
        }
        Ok(())
    }
}

fn slider(min: f32, max: f32, default: f32, precision: i16) -> ae::FloatSliderDef<'static> {
    ae::FloatSliderDef::setup(|f| {
        f.set_valid_min(min);
        f.set_valid_max(max);
        f.set_slider_min(min);
        f.set_slider_max(max);
        f.set_default(default as f64);
        f.set_precision(precision);
    })
}
fn checkbox(default: bool, label: &'static str) -> ae::CheckBoxDef<'static> {
    ae::CheckBoxDef::setup(|f| {
        f.set_default(default);
        f.set_label(label);
    })
}
fn read_params(params: &ae::Parameters<Params>) -> Result<NormalParams, ae::Error> {
    Ok(NormalParams {
        strength: params
            .get(Params::Strength)?
            .as_float_slider()?
            .value()
            .max(0.0) as f32,
        blur: params.get(Params::Blur)?.as_slider()?.value().clamp(0, 100) as usize,
        detail_scale: params
            .get(Params::DetailScale)?
            .as_slider()?
            .value()
            .clamp(1, 16) as usize,
        invert_x: params.get(Params::InvertX)?.as_checkbox()?.value(),
        invert_y: params.get(Params::InvertY)?.as_checkbox()?.value(),
        alpha_source: params.get(Params::AlphaMode)?.as_popup()?.value() == 2,
    })
}

fn box_blur(src: &[f32], w: usize, h: usize, radius: usize) -> Result<Vec<f32>, ae::Error> {
    if radius == 0 {
        return Ok(src.to_vec());
    }
    let n = w.checked_mul(h).ok_or(ae::Error::OutOfMemory)?;
    let mut tmp = Vec::new();
    tmp.try_reserve_exact(n)?;
    tmp.resize(n, 0.0);
    let mut out = Vec::new();
    out.try_reserve_exact(n)?;
    out.resize(n, 0.0);
    for y in 0..h {
        for x in 0..w {
            let a = x.saturating_sub(radius);
            let b = (x + radius).min(w - 1);
            let mut s = 0.0;
            for xx in a..=b {
                s += src[y * w + xx];
            }
            tmp[y * w + x] = s / (b - a + 1) as f32;
        }
    }
    for y in 0..h {
        for x in 0..w {
            let a = y.saturating_sub(radius);
            let b = (y + radius).min(h - 1);
            let mut s = 0.0;
            for yy in a..=b {
                s += tmp[yy * w + x];
            }
            out[y * w + x] = s / (b - a + 1) as f32;
        }
    }
    Ok(out)
}

fn normal_from_height(
    height: &[f32],
    w: usize,
    h: usize,
    x: usize,
    y: usize,
    p: &NormalParams,
) -> [u8; 3] {
    let d = p.detail_scale;
    let x0 = x.saturating_sub(d);
    let x1 = (x + d).min(w - 1);
    let y0 = y.saturating_sub(d);
    let y1 = (y + d).min(h - 1);
    let mut nx = -(height[y * w + x1] - height[y * w + x0]) * p.strength;
    let mut ny = -(height[y1 * w + x] - height[y0 * w + x]) * p.strength;
    if p.invert_x {
        nx = -nx;
    }
    if p.invert_y {
        ny = -ny;
    }
    let inv = (nx * nx + ny * ny + 1.0).sqrt().recip();
    let encode = |v: f32| ((v * 0.5 + 0.5) * 255.0).clamp(0.0, 255.0).round() as u8;
    [encode(nx * inv), encode(ny * inv), encode(inv)]
}

fn geometry(layer: &ae::Layer) -> Result<(usize, usize, usize), ae::Error> {
    let w = layer.width();
    let h = layer.height();
    let stride = layer.buffer_stride();
    if w == 0 || h == 0 || stride < w.saturating_mul(4) {
        return Err(ae::Error::BadCallbackParameter);
    }
    let need = stride.checked_mul(h).ok_or(ae::Error::OutOfMemory)?;
    if layer.buffer().len() < need {
        return Err(ae::Error::BadCallbackParameter);
    }
    Ok((w, h, stride))
}
fn render(p: &NormalParams, input: &ae::Layer, output: &mut ae::Layer) -> Result<(), ae::Error> {
    if input.bit_depth() != 8 || output.bit_depth() != 8 {
        return Err(ae::Error::BadCallbackParameter);
    }
    let (w, h, ss) = geometry(input)?;
    let (ow, oh, ds) = geometry(output)?;
    let w = w.min(ow);
    let h = h.min(oh);
    let src = input.buffer();
    let n = w.checked_mul(h).ok_or(ae::Error::OutOfMemory)?;
    let mut lum = Vec::new();
    lum.try_reserve_exact(n)?;
    lum.resize(n, 0.0);
    for y in 0..h {
        for x in 0..w {
            let s = y * ss + x * 4;
            lum[y * w + x] = (0.2126 * src[s + 1] as f32
                + 0.7152 * src[s + 2] as f32
                + 0.0722 * src[s + 3] as f32)
                / 255.0;
        }
    }
    let lum = box_blur(&lum, w, h, p.blur)?;
    let dst = output.buffer_mut();
    for y in 0..h {
        for x in 0..w {
            let s = y * ss + x * 4;
            let d = y * ds + x * 4;
            let n = normal_from_height(&lum, w, h, x, y, p);
            dst[d] = if p.alpha_source { src[s] } else { 255 };
            dst[d + 1] = n[0];
            dst[d + 2] = n[1];
            dst[d + 3] = n[2];
        }
    }
    Ok(())
}
fn pre_render(in_data: &ae::InData, extra: &mut ae::pf::PreRenderExtra) -> Result<(), ae::Error> {
    let req = extra.output_request();
    let cb = extra.callbacks();
    let r = cb.checkout_layer(
        0,
        0,
        &req,
        in_data.current_time(),
        in_data.time_step(),
        in_data.time_scale(),
    )?;
    extra.set_result_rect(r.result_rect.into());
    extra.set_max_result_rect(r.max_result_rect.into());
    Ok(())
}
fn smart_render(
    extra: &ae::pf::SmartRenderExtra,
    params: &ae::Parameters<Params>,
) -> Result<(), ae::Error> {
    let cb = extra.callbacks();
    let input = cb.checkout_layer_pixels(0)?.ok_or(ae::Error::Generic)?;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut output = cb.checkout_output()?.ok_or(ae::Error::Generic)?;
        render(&read_params(params)?, &input, &mut output)
    }));
    cb.checkin_layer_pixels(0)?;
    result.unwrap_or(Err(ae::Error::InternalStructDamaged))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn p() -> NormalParams {
        NormalParams {
            strength: 4.0,
            blur: 0,
            detail_scale: 1,
            invert_x: false,
            invert_y: false,
            alpha_source: false,
        }
    }
    #[test]
    fn flat_height_is_flat_normal() {
        assert_eq!(
            normal_from_height(&[0.5; 9], 3, 3, 1, 1, &p()),
            [128, 128, 255]
        );
    }
    #[test]
    fn horizontal_ramp_tilts_x() {
        let h = [0.0, 0.5, 1.0, 0.0, 0.5, 1.0, 0.0, 0.5, 1.0];
        assert!(normal_from_height(&h, 3, 3, 1, 1, &p())[0] < 128);
    }
    #[test]
    fn blur_preserves_constant() {
        assert_eq!(box_blur(&[0.25; 9], 3, 3, 4).unwrap(), vec![0.25; 9]);
    }
}
