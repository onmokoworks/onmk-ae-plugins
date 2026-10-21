// SPDX-License-Identifier: Apache-2.0
// onmk implementation (2026); upstream attribution and changes: see package NOTICE.
use after_effects as ae;
pub mod core;
pub mod world;
use core::{Image, Settings};

// Stable disk identities. Append only.
#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Invert,
    Side,
    Inside,
    Outside,
    RenderMode,
    UseBg,
    Color,
    Bg,
    Interpolation,
    Power,
    Blur,
    BlurSize,
    Antialias,
    Composite,
    Opacity,
}
#[derive(Default)]
struct Plugin;
ae::define_effect!(Plugin, (), Params);
#[derive(Clone)]
struct PreState {
    settings: Settings,
    rect: ae::Rect,
    width: usize,
    height: usize,
}
fn slider(max: f32, default: f64) -> ae::FloatSliderDef<'static> {
    ae::FloatSliderDef::setup(|p| {
        p.set_valid_min(0.0);
        p.set_valid_max(max);
        p.set_slider_min(0.0);
        p.set_slider_max(max.min(512.0));
        p.set_default(default);
        p.set_precision(2);
    })
}
impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        p: &mut ae::Parameters<Params>,
        _: ae::InData,
        _: ae::OutData,
    ) -> Result<(), ae::Error> {
        p.add(
            Params::Invert,
            "Invert",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(false);
            }),
        )?;
        p.add(
            Params::Side,
            "In/Out",
            ae::PopupDef::setup(|f| {
                f.set_options(&["Inside", "Outside", "Both"]);
                f.set_default(1);
            }),
        )?;
        p.add(Params::Inside, "Inside Threshold", slider(1000.0, 128.0))?;
        p.add(Params::Outside, "Outside Threshold", slider(1000.0, 128.0))?;
        p.add(
            Params::RenderMode,
            "Render Mode",
            ae::PopupDef::setup(|f| {
                f.set_options(&["RGB", "Layer"]);
                f.set_default(1);
            }),
        )?;
        p.add(
            Params::UseBg,
            "Use Background Color",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(false);
            }),
        )?;
        p.add(
            Params::Color,
            "Gradation Color",
            ae::ColorDef::setup(|f| {
                f.set_default(ae::Pixel8 {
                    alpha: 255,
                    red: 255,
                    green: 0,
                    blue: 0,
                });
            }),
        )?;
        p.add(
            Params::Bg,
            "BG Color",
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
            Params::Interpolation,
            "Interpolation Mode",
            ae::PopupDef::setup(|f| {
                f.set_options(&["Constant", "Linear", "Sphere", "Power"]);
                f.set_default(2);
            }),
        )?;
        p.add(
            Params::Power,
            "Power",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.01);
                f.set_valid_max(5.0);
                f.set_slider_min(0.01);
                f.set_slider_max(5.0);
                f.set_default(1.0);
                f.set_precision(2);
            }),
        )?;
        p.add(
            Params::Blur,
            "Blur Mode",
            ae::PopupDef::setup(|f| {
                f.set_options(&["No Blur", "Box", "Gaussian"]);
                f.set_default(1);
            }),
        )?;
        p.add(Params::BlurSize, "Blur Size", slider(500.0, 0.0))?;
        p.add(
            Params::Antialias,
            "Anti-aliasing",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(true);
            }),
        )?;
        p.add(
            Params::Composite,
            "Composite Original",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(true);
            }),
        )?;
        p.add(
            Params::Opacity,
            "Gradation Opacity (%)",
            slider(100.0, 100.0),
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
            ae::Command::About=>o.set_return_msg("onmk DistanceGradation v0.1.0\rAntialiased distance gradients over the original layer."),
            ae::Command::Render{in_layer,mut out_layer}=>{
                let settings=read_params(p,&i)?;
                let img=world::read_image(&in_layer,0,0)?;
                let origin=i.output_origin();
                render_into(&img,&settings,&mut out_layer,origin.h as i64,origin.v as i64)?;
            }
            ae::Command::SmartPreRender{mut extra}=>{
                let settings=read_params(p,&i)?;
                let (w,h)=dimensions(&i)?;
                // Min/max normalization depends on the whole image, not an ROI.
                let full=ae::Rect{left:0,top:0,right:w as i32,bottom:h as i32};
                let mut req=extra.output_request();
                let requested:ae::Rect=req.rect.into();
                req.rect=full.into();req.preserve_rgb_of_zero_alpha=1;
                let got=extra.callbacks().checkout_layer(0,0,&req,i.current_time(),i.time_step(),i.time_scale())?;
                extra.set_result_rect(ae::Rect{
                    left:requested.left.max(0).min(full.right),top:requested.top.max(0).min(full.bottom),
                    right:requested.right.max(0).min(full.right),bottom:requested.bottom.max(0).min(full.bottom)});
                extra.set_max_result_rect(full);
                extra.set_pre_render_data(PreState{settings,rect:got.result_rect.into(),width:w,height:h});
            }
            ae::Command::SmartRender{extra}=>{
                let state=extra.pre_render_data::<PreState>().cloned().ok_or(ae::Error::BadCallbackParameter)?;
                let cb=extra.callbacks();
                let input=cb.checkout_layer_pixels(0)?;
                let work=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
                    let Some(mut output)=cb.checkout_output()? else {return Ok(());};
                    let Some(ref input)=input else {return world::write_transparent(&mut output);};
                    // Reconstruct the complete layer, including transparent space
                    // outside a trimmed input world, before the global EDT.
                    let r=state.rect;
                    let input_origin=input.origin();
                    let tile=world::read_image(input,input_origin.h as i64,input_origin.v as i64)?;
                    if tile.width != (r.right-r.left).max(0) as usize || tile.height != (r.bottom-r.top).max(0) as usize {return Err(ae::Error::BadCallbackParameter);}
                    let full=canvas(&tile,state.width,state.height)?;
                    let origin=output.origin();
                    render_into(&full,&state.settings,&mut output,origin.h as i64,origin.v as i64)
                })).unwrap_or(Err(ae::Error::InternalStructDamaged));
                let checkin=if input.is_some(){cb.checkin_layer_pixels(0)}else{Ok(())};
                work.and(checkin)?;
            }
            _=>{}
        }
        Ok(())
    }
}
fn dimensions(i: &ae::InData) -> Result<(usize, usize), ae::Error> {
    let x: f64 = i.downsample_x().into();
    let y: f64 = i.downsample_y().into();
    if !(x > 0.0 && x <= 1.0 && y > 0.0 && y <= 1.0) {
        return Err(ae::Error::BadCallbackParameter);
    }
    Ok((
        (i.width() as f64 * x).ceil() as usize,
        (i.height() as f64 * y).ceil() as usize,
    ))
}
fn read_params(p: &ae::Parameters<Params>, i: &ae::InData) -> Result<Settings, ae::Error> {
    let f = |id| -> Result<f32, ae::Error> { Ok(p.get(id)?.as_float_slider()?.value() as f32) };
    let b = |id| -> Result<bool, ae::Error> { Ok(p.get(id)?.as_checkbox()?.value()) };
    let u = |id| -> Result<u8, ae::Error> { Ok(p.get(id)?.as_popup()?.value() as u8) };
    let c = |id| -> Result<[f32; 3], ae::Error> {
        let param = p.get(id)?;
        let col = param.as_color()?;
        if let Ok(v) = col.float_value() {
            Ok([v.red, v.green, v.blue])
        } else {
            let v = col.value();
            Ok([
                v.red as f32 / 255.0,
                v.green as f32 / 255.0,
                v.blue as f32 / 255.0,
            ])
        }
    };
    Ok(Settings {
        invert: b(Params::Invert)?,
        side: u(Params::Side)?,
        inside: f(Params::Inside)?,
        outside: f(Params::Outside)?,
        layer_color: u(Params::RenderMode)? == 2,
        use_bg: b(Params::UseBg)?,
        color: c(Params::Color)?,
        bg: c(Params::Bg)?,
        interpolation: u(Params::Interpolation)?,
        power: f(Params::Power)?,
        blur: u(Params::Blur)?,
        blur_size: f(Params::BlurSize)?,
        antialias: b(Params::Antialias)?,
        composite: b(Params::Composite)?,
        opacity: f(Params::Opacity)? / 100.0,
        scale: [
            f64::from(i.downsample_x()) as f32,
            f64::from(i.downsample_y()) as f32,
        ],
    })
}
fn empty_pixels(w: usize, h: usize) -> Result<Vec<[f32; 4]>, ae::Error> {
    let n = w.checked_mul(h).ok_or(ae::Error::OutOfMemory)?;
    let mut pixels = Vec::new();
    pixels
        .try_reserve_exact(n)
        .map_err(|_| ae::Error::OutOfMemory)?;
    pixels.resize(n, [0.0; 4]);
    Ok(pixels)
}
fn canvas(tile: &Image, w: usize, h: usize) -> Result<Image, ae::Error> {
    let mut pixels = empty_pixels(w, h)?;
    for y in 0..tile.height {
        for x in 0..tile.width {
            let xx = tile.left + x as i64;
            let yy = tile.top + y as i64;
            if xx >= 0 && yy >= 0 && xx < w as i64 && yy < h as i64 {
                pixels[yy as usize * w + xx as usize] = tile.pixels[y * tile.width + x];
            }
        }
    }
    Image::new(w, h, 0, 0, pixels).map_err(|_| ae::Error::BadCallbackParameter)
}
fn render_into(
    img: &Image,
    s: &Settings,
    out: &mut ae::Layer,
    left: i64,
    top: i64,
) -> Result<(), ae::Error> {
    let pixels = core::render(img, s).map_err(|e| match e {
        core::RenderError::OutOfMemory => ae::Error::OutOfMemory,
        _ => ae::Error::BadCallbackParameter,
    })?;
    let (w, h) = (out.width(), out.height());
    let mut cropped = empty_pixels(w, h)?;
    for y in 0..h {
        for x in 0..w {
            let xx = left + x as i64 - img.left;
            let yy = top + y as i64 - img.top;
            if xx >= 0 && yy >= 0 && xx < img.width as i64 && yy < img.height as i64 {
                cropped[y * w + x] = pixels[yy as usize * img.width + xx as usize];
            }
        }
    }
    world::write_image(out, &cropped)
}
