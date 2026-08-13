use after_effects as ae;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Amount,
    Size,
    Shadows,
    Midtones,
    Highlights,
    Seed,
    Monochrome,
}

#[derive(Clone, Copy)]
struct GrainParams {
    amount: f32,
    size: usize,
    shadows: f32,
    midtones: f32,
    highlights: f32,
    seed: u32,
    monochrome: bool,
}

#[derive(Clone, Copy, Default)]
struct PreRenderState {
    time: i32,
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
        params.add(Params::Amount, "Amount", float_slider(0.0, 100.0, 15.0, 1))?;
        params.add(
            Params::Size,
            "Size",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(1);
                f.set_valid_max(32);
                f.set_slider_min(1);
                f.set_slider_max(12);
                f.set_default(2);
            }),
        )?;
        params.add(
            Params::Shadows,
            "Shadows",
            float_slider(0.0, 200.0, 130.0, 0),
        )?;
        params.add(
            Params::Midtones,
            "Midtones",
            float_slider(0.0, 200.0, 100.0, 0),
        )?;
        params.add(
            Params::Highlights,
            "Highlights",
            float_slider(0.0, 200.0, 45.0, 0),
        )?;
        params.add(
            Params::Seed,
            "Seed",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(10000);
                f.set_slider_min(0);
                f.set_slider_max(10000);
                f.set_default(0);
            }),
        )?;
        params.add(
            Params::Monochrome,
            "Monochrome",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(true);
                f.set_label("Monochrome");
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
            ae::Command::About => out_data.set_return_msg(
                "LumaGrain v1.0\rLightweight luminance-weighted grain.\rWritten in Rust.",
            ),
            ae::Command::Render {
                in_layer,
                mut out_layer,
            } => {
                let p = read_params(params, in_data.current_time())?;
                render(&p, &in_layer, &mut out_layer)?;
            }
            ae::Command::SmartPreRender { mut extra } => pre_render(&in_data, &mut extra)?,
            ae::Command::SmartRender { extra } => smart_render(&extra, params)?,
            ae::Command::SmartRenderGpu { .. } => return Err(ae::Error::BadCallbackParameter),
            _ => {}
        }
        Ok(())
    }
}

fn float_slider(min: f32, max: f32, default: f32, precision: i16) -> ae::FloatSliderDef<'static> {
    ae::FloatSliderDef::setup(|f| {
        f.set_valid_min(min);
        f.set_valid_max(max);
        f.set_slider_min(min);
        f.set_slider_max(max);
        f.set_default(default as f64);
        f.set_precision(precision);
    })
}

fn read_params(params: &ae::Parameters<Params>, time: i32) -> Result<GrainParams, ae::Error> {
    let pct = |id| -> Result<f32, ae::Error> {
        Ok(params.get(id)?.as_float_slider()?.value() as f32 / 100.0)
    };
    Ok(GrainParams {
        amount: pct(Params::Amount)?,
        size: params.get(Params::Size)?.as_slider()?.value().clamp(1, 32) as usize,
        shadows: pct(Params::Shadows)?,
        midtones: pct(Params::Midtones)?,
        highlights: pct(Params::Highlights)?,
        seed: params.get(Params::Seed)?.as_slider()?.value().max(0) as u32
            ^ (time as u32).wrapping_mul(0x9e37_79b9),
        monochrome: params.get(Params::Monochrome)?.as_checkbox()?.value(),
    })
}

fn luma_weight(luma: f32, p: &GrainParams) -> f32 {
    let l = luma.clamp(0.0, 1.0);
    let shadow = (1.0 - l).powi(2);
    let highlight = l.powi(2);
    let midtone = (1.0 - (2.0 * l - 1.0).abs()).max(0.0);
    let sum = shadow + midtone + highlight;
    (shadow * p.shadows + midtone * p.midtones + highlight * p.highlights) / sum.max(1.0e-6)
}

fn hash(mut v: u32) -> f32 {
    v ^= v >> 16;
    v = v.wrapping_mul(0x7feb_352d);
    v ^= v >> 15;
    v = v.wrapping_mul(0x846c_a68b);
    v ^= v >> 16;
    (v as f32 / u32::MAX as f32) * 2.0 - 1.0
}

fn noise(x: usize, y: usize, channel: u32, p: &GrainParams) -> f32 {
    let gx = (x / p.size) as u32;
    let gy = (y / p.size) as u32;
    hash(gx.wrapping_mul(0x1f12_3bb5) ^ gy.wrapping_mul(0x5f35_6495) ^ p.seed ^ channel)
}

fn process_pixel(px: [u8; 4], x: usize, y: usize, p: &GrainParams) -> [u8; 4] {
    let luma = (0.2126 * px[1] as f32 + 0.7152 * px[2] as f32 + 0.0722 * px[3] as f32) / 255.0;
    let scale = p.amount * luma_weight(luma, p) * 64.0;
    let mono = noise(x, y, 0, p) * scale;
    let apply = |v: u8, c: u32| -> u8 {
        let n = if p.monochrome {
            mono
        } else {
            noise(x, y, c, p) * scale
        };
        (v as f32 + n).clamp(0.0, 255.0).round() as u8
    };
    [px[0], apply(px[1], 1), apply(px[2], 2), apply(px[3], 3)]
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

fn render(p: &GrainParams, input: &ae::Layer, output: &mut ae::Layer) -> Result<(), ae::Error> {
    if input.bit_depth() != 8 || output.bit_depth() != 8 {
        return Err(ae::Error::BadCallbackParameter);
    }
    let (w, h, src_stride) = geometry(input)?;
    let (ow, oh, dst_stride) = geometry(output)?;
    let w = w.min(ow);
    let h = h.min(oh);
    let src = input.buffer();
    let dst = output.buffer_mut();
    for y in 0..h {
        for x in 0..w {
            let s = y * src_stride + x * 4;
            let d = y * dst_stride + x * 4;
            let out = process_pixel([src[s], src[s + 1], src[s + 2], src[s + 3]], x, y, p);
            dst[d..d + 4].copy_from_slice(&out);
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
    extra.set_pre_render_data(PreRenderState {
        time: in_data.current_time(),
    });
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
        let time = extra
            .pre_render_data::<PreRenderState>()
            .map(|state| state.time)
            .unwrap_or(0);
        let p = read_params(params, time)?;
        render(&p, &input, &mut output)
    }));
    cb.checkin_layer_pixels(0)?;
    result.unwrap_or(Err(ae::Error::InternalStructDamaged))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn p() -> GrainParams {
        GrainParams {
            amount: 1.0,
            size: 2,
            shadows: 1.5,
            midtones: 1.0,
            highlights: 0.25,
            seed: 7,
            monochrome: true,
        }
    }
    #[test]
    fn shadows_receive_more_grain_than_highlights() {
        assert!(luma_weight(0.05, &p()) > luma_weight(0.95, &p()));
    }
    #[test]
    fn noise_is_deterministic() {
        assert_eq!(noise(4, 8, 0, &p()), noise(4, 8, 0, &p()));
    }
    #[test]
    fn zero_amount_is_passthrough() {
        let mut q = p();
        q.amount = 0.0;
        assert_eq!(
            process_pixel([255, 10, 20, 30], 1, 1, &q),
            [255, 10, 20, 30]
        );
    }
}
