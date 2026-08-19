use after_effects as ae;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Shape,
    Center,
    SizeX,
    SizeY,
    ForceCircle,
    Roundness,
    Edge,
    StrokeWidth,
    Expansion,
    Feather,
    Invert,
    Color,
    Opacity,
    Composite,
    MaskOperation,
}

#[derive(Clone, Copy)]
struct ShapeParams {
    shape: i32,
    center: (f32, f32),
    size: (f32, f32),
    force_circle: bool,
    roundness: f32,
    stroke: bool,
    stroke_width: f32,
    expansion: f32,
    feather: f32,
    invert: bool,
    color: [f32; 3],
    opacity: f32,
    composite: i32,
    mask_operation: i32,
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
            Params::Shape,
            "Shape",
            ae::PopupDef::setup(|f| {
                f.set_options(&["Circle", "Rectangle", "Rounded Rectangle"]);
                f.set_default(1);
            }),
        )?;
        params.add(
            Params::Center,
            "Center",
            ae::PointDef::setup(|f| {
                f.set_default((50.0, 50.0));
                f.set_restrict_bounds(false);
            }),
        )?;
        params.add(
            Params::SizeX,
            "Size X",
            float_slider(0.0, 30000.0, 100.0, 1),
        )?;
        params.add(
            Params::SizeY,
            "Size Y",
            float_slider(0.0, 30000.0, 100.0, 1),
        )?;
        params.add(
            Params::ForceCircle,
            "Force Circle / Square",
            checkbox(false, "Constrain"),
        )?;
        params.add(
            Params::Roundness,
            "Roundness",
            float_slider(0.0, 15000.0, 20.0, 1),
        )?;
        params.add(
            Params::Edge,
            "Edge",
            ae::PopupDef::setup(|f| {
                f.set_options(&["Fill", "Stroke"]);
                f.set_default(1);
            }),
        )?;
        params.add(
            Params::StrokeWidth,
            "Stroke Width",
            float_slider(0.0, 30000.0, 10.0, 1),
        )?;
        params.add(
            Params::Expansion,
            "Mask Expansion",
            float_slider(-1000.0, 1000.0, 0.0, 1),
        )?;
        params.add(
            Params::Feather,
            "Feather",
            float_slider(0.0, 1000.0, 1.0, 1),
        )?;
        params.add(Params::Invert, "Invert", checkbox(false, "Invert Shape"))?;
        params.add(
            Params::Color,
            "Color",
            ae::ColorDef::setup(|f| {
                f.set_default(ae::Pixel8 {
                    alpha: 255,
                    red: 255,
                    green: 255,
                    blue: 255,
                });
            }),
        )?;
        params.add(
            Params::Opacity,
            "Opacity",
            float_slider(0.0, 100.0, 100.0, 1),
        )?;
        params.add(
            Params::Composite,
            "Composite",
            ae::PopupDef::setup(|f| {
                f.set_options(&[
                    "Over",
                    "Shape Only",
                    "Mask Source",
                    "Alpha Matte",
                    "Luma Matte",
                ]);
                f.set_default(1);
            }),
        )?;
        params.add(
            Params::MaskOperation,
            "Source Alpha Operation",
            ae::PopupDef::setup(|f| {
                f.set_options(&["Replace", "Intersect", "Add", "Subtract", "Difference"]);
                f.set_default(2);
            }),
        )?;
        Ok(())
    }

    fn handle_command(
        &self,
        cmd: ae::Command,
        in_data: ae::InData,
        mut out_data: ae::OutData,
        params: &mut ae::Parameters<Params>,
    ) -> Result<(), ae::Error> {
        match cmd {
            ae::Command::About => out_data.set_return_msg(
                "Shape Plus v0.1\rCircle-compatible shape generator.\rWritten in Rust.",
            ),
            ae::Command::Render {
                in_layer,
                mut out_layer,
            } => render(&read_params(params)?, &in_data, &in_layer, &mut out_layer)?,
            ae::Command::SmartPreRender { mut extra } => pre_render(&in_data, &mut extra)?,
            ae::Command::SmartRender { extra } => smart_render(&extra, &in_data, params)?,
            ae::Command::SmartRenderGpu { .. } => return Err(ae::Error::BadCallbackParameter),
            _ => {}
        }
        Ok(())
    }
}

fn float_slider(min: f32, max: f32, default: f32, precision: i16) -> ae::FloatSliderDef<'static> {
    ae::FloatSliderDef::setup(move |f| {
        f.set_valid_min(min);
        f.set_valid_max(max);
        f.set_slider_min(min);
        f.set_slider_max(max);
        f.set_default(default as f64);
        f.set_precision(precision);
    })
}

fn checkbox(default: bool, label: &'static str) -> ae::CheckBoxDef<'static> {
    ae::CheckBoxDef::setup(move |f| {
        f.set_default(default);
        f.set_label(label);
    })
}

fn read_params(params: &ae::Parameters<Params>) -> Result<ShapeParams, ae::Error> {
    let point = params.get(Params::Center)?.as_point()?.float_value()?;
    let color = params.get(Params::Color)?.as_color()?.float_value()?;
    Ok(ShapeParams {
        shape: params.get(Params::Shape)?.as_popup()?.value() as i32,
        center: (point.x as f32, point.y as f32),
        size: (
            params
                .get(Params::SizeX)?
                .as_float_slider()?
                .value()
                .max(0.0) as f32,
            params
                .get(Params::SizeY)?
                .as_float_slider()?
                .value()
                .max(0.0) as f32,
        ),
        force_circle: params.get(Params::ForceCircle)?.as_checkbox()?.value(),
        roundness: params
            .get(Params::Roundness)?
            .as_float_slider()?
            .value()
            .max(0.0) as f32,
        stroke: params.get(Params::Edge)?.as_popup()?.value() == 2,
        stroke_width: params
            .get(Params::StrokeWidth)?
            .as_float_slider()?
            .value()
            .max(0.0) as f32,
        expansion: params.get(Params::Expansion)?.as_float_slider()?.value() as f32,
        feather: params
            .get(Params::Feather)?
            .as_float_slider()?
            .value()
            .max(0.0) as f32,
        invert: params.get(Params::Invert)?.as_checkbox()?.value(),
        color: [color.red, color.green, color.blue],
        opacity: (params.get(Params::Opacity)?.as_float_slider()?.value() as f32 / 100.0)
            .clamp(0.0, 1.0),
        composite: params.get(Params::Composite)?.as_popup()?.value() as i32,
        mask_operation: params.get(Params::MaskOperation)?.as_popup()?.value() as i32,
    })
}

fn signed_distance(p: &ShapeParams, x: f32, y: f32) -> f32 {
    let mut sx = p.size.0;
    let mut sy = p.size.1;
    if p.force_circle {
        let side = sx.min(sy);
        sx = side;
        sy = side;
    }
    let hx = sx * 0.5;
    let hy = sy * 0.5;
    let dx = (x - p.center.0).abs();
    let dy = (y - p.center.1).abs();
    if p.shape == 1 {
        let rx = hx.max(0.0001);
        let ry = hy.max(0.0001);
        let k = ((dx / rx).powi(2) + (dy / ry).powi(2)).sqrt();
        (k - 1.0) * rx.min(ry)
    } else {
        let radius = if p.shape == 3 {
            p.roundness.min(hx.min(hy))
        } else {
            0.0
        };
        let qx = dx - (hx - radius);
        let qy = dy - (hy - radius);
        qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - radius
    }
}

fn coverage(p: &ShapeParams, x: f32, y: f32) -> f32 {
    let d = signed_distance(p, x, y) - p.expansion;
    let signed = if p.stroke {
        d.abs() - p.stroke_width * 0.5
    } else {
        d
    };
    let aa = p.feather.max(0.5);
    let mut c = (0.5 - signed / aa).clamp(0.0, 1.0);
    if p.invert {
        c = 1.0 - c;
    }
    c
}

fn render(
    p: &ShapeParams,
    in_data: &ae::InData,
    input: &ae::Layer,
    output: &mut ae::Layer,
) -> Result<(), ae::Error> {
    let sx = f64::from(in_data.downsample_x()) as f32;
    let sy = f64::from(in_data.downsample_y()) as f32;
    let origin = output.origin();
    input.iterate_with(
        output,
        0,
        output.height() as i32,
        None,
        |x, y, src, mut dst| {
            let px = (x + origin.h) as f32 / sx.max(0.0001) + 0.5;
            let py = (y + origin.v) as f32 / sy.max(0.0001) + 0.5;
            let a = coverage(p, px, py) * p.opacity;
            match (&src, &mut dst) {
                (ae::GenericPixel::Pixel8(s), ae::GenericPixelMut::Pixel8(d)) => {
                    composite_u8(s, d, p, a)
                }
                (ae::GenericPixel::Pixel16(s), ae::GenericPixelMut::Pixel16(d)) => {
                    composite_u16(s, d, p, a)
                }
                (ae::GenericPixel::PixelF32(s), ae::GenericPixelMut::PixelF32(d)) => {
                    composite_f32(s, d, p, a)
                }
                _ => return Err(ae::Error::BadCallbackParameter),
            }
            Ok(())
        },
    )
}

fn composite_values(src: [f32; 4], p: &ShapeParams, a: f32) -> [f32; 4] {
    if p.composite == 3 {
        let out_a = combine_alpha(src[0], a, p.mask_operation);
        let scale = if src[0] > 0.00001 {
            out_a / src[0]
        } else {
            0.0
        };
        return [out_a, src[1] * scale, src[2] * scale, src[3] * scale];
    }
    let matte = combine_alpha(src[0], a, p.mask_operation);
    if p.composite == 4 {
        return [matte, matte, matte, matte];
    }
    if p.composite == 5 {
        return [1.0, matte, matte, matte];
    }
    let shape = [a, p.color[0] * a, p.color[1] * a, p.color[2] * a];
    if p.composite == 2 {
        return shape;
    }
    let inv = 1.0 - a;
    [
        shape[0] + src[0] * inv,
        shape[1] + src[1] * inv,
        shape[2] + src[2] * inv,
        shape[3] + src[3] * inv,
    ]
}

fn combine_alpha(source: f32, mask: f32, operation: i32) -> f32 {
    match operation {
        1 => mask,
        2 => source * mask,
        3 => source + mask - source * mask,
        4 => source * (1.0 - mask),
        5 => (source - mask).abs(),
        _ => source * mask,
    }
}

fn composite_u8(s: &ae::Pixel8, d: &mut ae::Pixel8, p: &ShapeParams, a: f32) {
    let o = composite_values(
        [
            s.alpha as f32 / 255.0,
            s.red as f32 / 255.0,
            s.green as f32 / 255.0,
            s.blue as f32 / 255.0,
        ],
        p,
        a,
    );
    d.alpha = (o[0].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    d.red = (o[1].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    d.green = (o[2].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    d.blue = (o[3].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
}

fn composite_u16(s: &ae::Pixel16, d: &mut ae::Pixel16, p: &ShapeParams, a: f32) {
    const MAX: f32 = 32768.0;
    let o = composite_values(
        [
            s.alpha as f32 / MAX,
            s.red as f32 / MAX,
            s.green as f32 / MAX,
            s.blue as f32 / MAX,
        ],
        p,
        a,
    );
    d.alpha = (o[0].clamp(0.0, 1.0) * MAX + 0.5) as u16;
    d.red = (o[1].clamp(0.0, 1.0) * MAX + 0.5) as u16;
    d.green = (o[2].clamp(0.0, 1.0) * MAX + 0.5) as u16;
    d.blue = (o[3].clamp(0.0, 1.0) * MAX + 0.5) as u16;
}

fn composite_f32(s: &ae::PixelF32, d: &mut ae::PixelF32, p: &ShapeParams, a: f32) {
    let o = composite_values([s.alpha, s.red, s.green, s.blue], p, a);
    d.alpha = o[0];
    d.red = o[1];
    d.green = o[2];
    d.blue = o[3];
}

fn pre_render(in_data: &ae::InData, extra: &mut ae::pf::PreRenderExtra) -> Result<(), ae::Error> {
    let req = extra.output_request();
    let r = extra.callbacks().checkout_layer(
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
    in_data: &ae::InData,
    params: &ae::Parameters<Params>,
) -> Result<(), ae::Error> {
    let cb = extra.callbacks();
    let input = cb.checkout_layer_pixels(0)?.ok_or(ae::Error::Generic)?;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut output = cb.checkout_output()?.ok_or(ae::Error::Generic)?;
        render(&read_params(params)?, in_data, &input, &mut output)
    }));
    cb.checkin_layer_pixels(0)?;
    result.unwrap_or(Err(ae::Error::InternalStructDamaged))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(shape: i32) -> ShapeParams {
        ShapeParams {
            shape,
            center: (50.0, 50.0),
            size: (100.0, 100.0),
            force_circle: false,
            roundness: 20.0,
            stroke: false,
            stroke_width: 10.0,
            expansion: 0.0,
            feather: 1.0,
            invert: false,
            color: [1.0; 3],
            opacity: 1.0,
            composite: 1,
            mask_operation: 2,
        }
    }

    #[test]
    fn circle_center_is_inside_and_edge_is_zero() {
        let p = params(1);
        assert!(signed_distance(&p, 50.0, 50.0) < 0.0);
        assert!(signed_distance(&p, 100.0, 50.0).abs() < 0.001);
    }

    #[test]
    fn rounded_square_becomes_circle_at_half_size() {
        let mut p = params(3);
        p.roundness = 50.0;
        assert!(signed_distance(&p, 100.0, 50.0).abs() < 0.001);
        assert!(signed_distance(&p, 100.0, 100.0) > 0.0);
    }

    #[test]
    fn force_circle_uses_shorter_dimension() {
        let mut p = params(1);
        p.size = (200.0, 100.0);
        p.force_circle = true;
        assert!(signed_distance(&p, 100.0, 50.0).abs() < 0.001);
    }

    #[test]
    fn expansion_grows_fill() {
        let mut p = params(2);
        assert!(signed_distance(&p, 105.0, 50.0) > 0.0);
        p.expansion = 10.0;
        assert!(coverage(&p, 105.0, 50.0) > 0.99);
    }

    #[test]
    fn mask_operations_match_alpha_math() {
        assert!((combine_alpha(0.5, 0.25, 1) - 0.25).abs() < 0.0001);
        assert!((combine_alpha(0.5, 0.25, 2) - 0.125).abs() < 0.0001);
        assert!((combine_alpha(0.5, 0.25, 3) - 0.625).abs() < 0.0001);
        assert!((combine_alpha(0.5, 0.25, 4) - 0.375).abs() < 0.0001);
        assert!((combine_alpha(0.5, 0.25, 5) - 0.25).abs() < 0.0001);
    }
}
