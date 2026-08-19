use after_effects as ae;

pub mod core;
mod gpu;
use core::EdgeParams;

const INPUT_ID: u32 = 1;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Border,
    Roughness,
    Scale,
    Detail,
    Balance,
    Sharpness,
    Stretch,
    Angle,
    Evolution,
    Seed,
    Preserve,
    Mix,
    UseGpu,
}

#[derive(Clone, Copy)]
struct PreRenderState {
    rect: ae::Rect,
}

#[derive(Default)]
struct Plugin;
ae::define_effect!(Plugin, (), Params);

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        p: &mut ae::Parameters<Params>,
        _i: ae::InData,
        _o: ae::OutData,
    ) -> Result<(), ae::Error> {
        p.add(
            Params::Border,
            "Border",
            fs(-512.0, 512.0, -100.0, 100.0, 0.0, 1),
        )?;
        p.add(
            Params::Roughness,
            "Roughness",
            fs(0.0, 512.0, 0.0, 100.0, 12.0, 1),
        )?;
        p.add(Params::Scale, "Scale", fs(1.0, 2048.0, 2.0, 500.0, 48.0, 1))?;
        p.add(
            Params::Detail,
            "Detail",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(1);
                f.set_valid_max(8);
                f.set_slider_min(1);
                f.set_slider_max(8);
                f.set_default(4);
            }),
        )?;
        p.add(
            Params::Balance,
            "Edge Balance",
            fs(-100.0, 100.0, -100.0, 100.0, 0.0, 1),
        )?;
        p.add(
            Params::Sharpness,
            "Sharpness",
            fs(0.0, 100.0, 0.0, 100.0, 70.0, 1),
        )?;
        p.add(
            Params::Stretch,
            "Stretch",
            fs(-99.0, 99.0, -99.0, 99.0, 0.0, 1),
        )?;
        p.add(
            Params::Angle,
            "Direction",
            ae::AngleDef::setup(|f| {
                f.set_default(0.0);
            }),
        )?;
        p.add(
            Params::Evolution,
            "Evolution",
            ae::AngleDef::setup(|f| {
                f.set_default(0.0);
            }),
        )?;
        p.add(
            Params::Seed,
            "Random Seed",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(100000);
                f.set_slider_min(0);
                f.set_slider_max(10000);
                f.set_default(0);
            }),
        )?;
        p.add(
            Params::Preserve,
            "Preserve Detail",
            fs(0.0, 100.0, 0.0, 100.0, 50.0, 1),
        )?;
        p.add(Params::Mix, "Mix", fs(0.0, 100.0, 0.0, 100.0, 100.0, 1))?;
        p.add(
            Params::UseGpu,
            "GPU Acceleration",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(true);
                f.set_label("Use GPU (Metal / Vulkan / DX12)");
            }),
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
            ae::Command::About=>o.set_return_msg("EdgeSmith v0.1\rDistance-field rough edge designer with deterministic GPU acceleration.\rWritten in Rust."),
            ae::Command::Render{in_layer,mut out_layer}=>render_layer(&in_layer,&mut out_layer,&read_params(p)?,false)?,
            ae::Command::SmartPreRender{mut extra}=>pre_render(&i,p,&mut extra)?,
            ae::Command::SmartRender{extra}=>smart_render(&extra,&i,p,false)?,
            ae::Command::SmartRenderGpu{extra}=>smart_render(&extra,&i,p,true)?,
            _=>{}
        }
        Ok(())
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
fn read_params(p: &ae::Parameters<Params>) -> Result<EdgeParams, ae::Error> {
    let v = |id| -> Result<f32, ae::Error> { Ok(p.get(id)?.as_float_slider()?.value() as f32) };
    Ok(EdgeParams {
        border: v(Params::Border)?,
        roughness: v(Params::Roughness)?,
        scale: v(Params::Scale)?,
        detail: p.get(Params::Detail)?.as_slider()?.value().clamp(1, 8) as u32,
        balance: v(Params::Balance)?,
        sharpness: v(Params::Sharpness)?,
        stretch: v(Params::Stretch)?,
        angle: p.get(Params::Angle)?.as_angle()?.value() as f32,
        evolution: p.get(Params::Evolution)?.as_angle()?.value() as f32,
        seed: p.get(Params::Seed)?.as_slider()?.value().max(0) as u32,
        preserve: v(Params::Preserve)?,
        mix: v(Params::Mix)? / 100.0,
    })
}

fn pre_render(
    i: &ae::InData,
    p: &ae::Parameters<Params>,
    extra: &mut ae::pf::PreRenderExtra,
) -> Result<(), ae::Error> {
    let pad = (p.get(Params::Border)?.as_float_slider()?.value().abs()
        + p.get(Params::Roughness)?.as_float_slider()?.value()
        + 8.0)
        .ceil()
        .min(1024.0) as i32;
    let mut req = extra.output_request();
    let mut r: ae::Rect = req.rect.into();
    r.left -= pad;
    r.top -= pad;
    r.right += pad;
    r.bottom += pad;
    req.rect = r.into();
    let got = extra.callbacks().checkout_layer(
        0,
        INPUT_ID as i32,
        &req,
        i.current_time(),
        i.time_step(),
        i.time_scale(),
    )?;
    extra.set_gpu_render_possible(p.get(Params::UseGpu)?.as_checkbox()?.value());
    extra.set_returns_extra_pixels(true);
    extra.set_result_rect(got.result_rect.into());
    extra.set_max_result_rect(got.max_result_rect.into());
    extra.set_pre_render_data(PreRenderState {
        rect: got.result_rect.into(),
    });
    Ok(())
}

fn smart_render(
    extra: &ae::pf::SmartRenderExtra,
    i: &ae::InData,
    p: &ae::Parameters<Params>,
    gpu_requested: bool,
) -> Result<(), ae::Error> {
    let cb = extra.callbacks();
    let input = cb
        .checkout_layer_pixels(INPUT_ID)?
        .ok_or(ae::Error::Generic)?;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut output = cb.checkout_output()?.ok_or(ae::Error::Generic)?;
        let ep = read_params(p)?;
        let use_gpu =
            (gpu_requested || p.get(Params::UseGpu)?.as_checkbox()?.value()) && gpu_requested;
        let (rgba, w, h) = layer_rgba(&input)?;
        let rendered = if use_gpu {
            gpu::render(&rgba, w, h, &ep).unwrap_or_else(|_| core::render_rgba(&rgba, w, h, &ep))
        } else {
            core::render_rgba(&rgba, w, h, &ep)
        };
        let ow = output.width();
        let oh = output.height();
        let mut cropped = vec![0.0; ow * oh * 4];
        let rect = extra
            .pre_render_data::<PreRenderState>()
            .map(|s| s.rect)
            .unwrap_or(ae::Rect {
                left: 0,
                top: 0,
                right: w as i32,
                bottom: h as i32,
            });
        let ox = i.output_origin().h - rect.left;
        let oy = i.output_origin().v - rect.top;
        for y in 0..oh {
            for x in 0..ow {
                let sx = ox + x as i32;
                let sy = oy + y as i32;
                if sx >= 0 && sy >= 0 && (sx as usize) < w && (sy as usize) < h {
                    let s = (sy as usize * w + sx as usize) * 4;
                    let d = (y * ow + x) * 4;
                    cropped[d..d + 4].copy_from_slice(&rendered[s..s + 4]);
                }
            }
        }
        write_rgba(&cropped, &mut output, ow, oh)
    }));
    cb.checkin_layer_pixels(INPUT_ID)?;
    result.unwrap_or(Err(ae::Error::InternalStructDamaged))
}

fn layer_rgba(layer: &ae::Layer) -> Result<(Vec<f32>, usize, usize), ae::Error> {
    let w = layer.width();
    let h = layer.height();
    if w == 0 || h == 0 {
        return Err(ae::Error::BadCallbackParameter);
    }
    let depth = layer.bit_depth();
    let bpp = match depth {
        32 => 16,
        16 => 8,
        8 => 4,
        _ => return Err(ae::Error::BadCallbackParameter),
    };
    let stride = layer.buffer_stride();
    let src = layer.buffer();
    let mut out = vec![0.0; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let s = y * stride + x * bpp;
            let d = (y * w + x) * 4;
            let read = |c: usize| match depth {
                32 => f32::from_ne_bytes(src[s + c * 4..s + c * 4 + 4].try_into().unwrap()),
                16 => {
                    u16::from_ne_bytes(src[s + c * 2..s + c * 2 + 2].try_into().unwrap()) as f32
                        / 32768.0
                }
                _ => src[s + c] as f32 / 255.0,
            };
            let a = read(0).clamp(0.0, 1.0);
            let inv = if a > 1e-6 { 1.0 / a } else { 1.0 };
            out[d] = read(1) * inv;
            out[d + 1] = read(2) * inv;
            out[d + 2] = read(3) * inv;
            out[d + 3] = a;
        }
    }
    Ok((out, w, h))
}
fn write_rgba(src: &[f32], layer: &mut ae::Layer, w: usize, h: usize) -> Result<(), ae::Error> {
    let depth = layer.bit_depth();
    let bpp = match depth {
        32 => 16,
        16 => 8,
        8 => 4,
        _ => return Err(ae::Error::BadCallbackParameter),
    };
    let stride = layer.buffer_stride();
    let dst = layer.buffer_mut();
    for y in 0..h {
        for x in 0..w {
            let s = (y * w + x) * 4;
            let d = y * stride + x * bpp;
            let a = src[s + 3].clamp(0.0, 1.0);
            for (dc, sc) in [3usize, 0, 1, 2].into_iter().enumerate() {
                let v = if sc == 3 {
                    a
                } else {
                    (src[s + sc] * a).clamp(0.0, 1.0)
                };
                match depth {
                    32 => dst[d + dc * 4..d + dc * 4 + 4].copy_from_slice(&v.to_ne_bytes()),
                    16 => dst[d + dc * 2..d + dc * 2 + 2]
                        .copy_from_slice(&((v * 32768.0).round() as u16).to_ne_bytes()),
                    _ => dst[d + dc] = (v * 255.0).round() as u8,
                }
            }
        }
    }
    Ok(())
}
fn render_layer(
    input: &ae::Layer,
    output: &mut ae::Layer,
    p: &EdgeParams,
    use_gpu: bool,
) -> Result<(), ae::Error> {
    let (rgba, w, h) = layer_rgba(input)?;
    let mut rendered = if use_gpu {
        gpu::render(&rgba, w, h, p).unwrap_or_else(|_| core::render_rgba(&rgba, w, h, p))
    } else {
        core::render_rgba(&rgba, w, h, p)
    };
    let ow = output.width();
    let oh = output.height();
    if ow != w || oh != h {
        let mut cropped = vec![0.0; ow * oh * 4];
        for y in 0..oh.min(h) {
            for x in 0..ow.min(w) {
                cropped[(y * ow + x) * 4..(y * ow + x + 1) * 4]
                    .copy_from_slice(&rendered[(y * w + x) * 4..(y * w + x + 1) * 4]);
            }
        }
        rendered = cropped;
    }
    write_rgba(&rendered, output, ow, oh)
}
