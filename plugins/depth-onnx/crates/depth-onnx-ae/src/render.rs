use after_effects as ae;

pub fn world_to_float_rgba(input: &ae::Layer) -> Option<Vec<f32>> {
    let width = input.width() as i32;
    let height = input.height() as i32;
    if width <= 0 || height <= 0 {
        return None;
    }

    let mut rgba = vec![0.0f32; (width * height * 4) as usize];
    let w = width as usize;
    let h = height as usize;

    match input.bit_depth() {
        8 => {
            const INV: f32 = 1.0 / 255.0;
            for y in 0..h {
                for x in 0..w {
                    let p = input.as_pixel8(x, y);
                    let i = (y * w + x) * 4;
                    rgba[i] = p.alpha as f32 * INV;
                    rgba[i + 1] = p.red as f32 * INV;
                    rgba[i + 2] = p.green as f32 * INV;
                    rgba[i + 3] = p.blue as f32 * INV;
                }
            }
        }
        16 => {
            const INV: f32 = 1.0 / 32768.0;
            for y in 0..h {
                for x in 0..w {
                    let p = input.as_pixel16(x, y);
                    let i = (y * w + x) * 4;
                    rgba[i] = p.alpha as f32 * INV;
                    rgba[i + 1] = p.red as f32 * INV;
                    rgba[i + 2] = p.green as f32 * INV;
                    rgba[i + 3] = p.blue as f32 * INV;
                }
            }
        }
        32 => {
            for y in 0..h {
                for x in 0..w {
                    let p = input.as_pixel32(x, y);
                    let i = (y * w + x) * 4;
                    rgba[i] = p.alpha;
                    rgba[i + 1] = p.red;
                    rgba[i + 2] = p.green;
                    rgba[i + 3] = p.blue;
                }
            }
        }
        _ => return None,
    }

    Some(rgba)
}

pub fn write_depth_to_world(
    output: &mut ae::Layer,
    depth: &[f32],
    size: i32,
    invert: bool,
) -> Result<(), ae::Error> {
    let width = output.width() as i32;
    let height = output.height() as i32;
    if width <= 0 || height <= 0 {
        return Ok(());
    }

    let w = width as usize;
    let h = height as usize;

    match output.bit_depth() {
        8 => {
            for y in 0..h {
                for x in 0..w {
                    let d = sample(depth, size, width, height, x as i32, y as i32, invert);
                    let g = (d * 255.0) as u8;
                    let out = output.as_pixel8_mut(x, y);
                    out.alpha = 255;
                    out.red = g;
                    out.green = g;
                    out.blue = g;
                }
            }
        }
        16 => {
            for y in 0..h {
                for x in 0..w {
                    let d = sample(depth, size, width, height, x as i32, y as i32, invert);
                    let g = (d * 32768.0) as u16;
                    let out = output.as_pixel16_mut(x, y);
                    out.alpha = 32768;
                    out.red = g;
                    out.green = g;
                    out.blue = g;
                }
            }
        }
        32 => {
            for y in 0..h {
                for x in 0..w {
                    let d = sample(depth, size, width, height, x as i32, y as i32, invert);
                    let out = output.as_pixel32_mut(x, y);
                    out.alpha = 1.0;
                    out.red = d;
                    out.green = d;
                    out.blue = d;
                }
            }
        }
        _ => return Err(ae::Error::BadCallbackParameter),
    }

    Ok(())
}

fn sample(depth: &[f32], size: i32, width: i32, height: i32, x: i32, y: i32, invert: bool) -> f32 {
    let u = if width > 1 {
        x as f32 / (width - 1) as f32
    } else {
        0.0
    };
    let v = if height > 1 {
        y as f32 / (height - 1) as f32
    } else {
        0.0
    };
    let mut d = sample_bilinear(depth, size, u, v);
    if invert {
        d = 1.0 - d;
    }
    d.clamp(0.0, 1.0)
}

fn sample_bilinear(depth: &[f32], size: i32, u: f32, v: f32) -> f32 {
    let size = size.max(1);
    let fx = u.clamp(0.0, 1.0) * (size - 1) as f32;
    let fy = v.clamp(0.0, 1.0) * (size - 1) as f32;
    let x0 = fx.floor() as i32;
    let y0 = fy.floor() as i32;
    let x1 = (x0 + 1).min(size - 1);
    let y1 = (y0 + 1).min(size - 1);
    let wx = fx - x0 as f32;
    let wy = fy - y0 as f32;

    let v00 = depth[(y0 * size + x0) as usize];
    let v01 = depth[(y0 * size + x1) as usize];
    let v10 = depth[(y1 * size + x0) as usize];
    let v11 = depth[(y1 * size + x1) as usize];
    let v0 = v00 * (1.0 - wx) + v01 * wx;
    let v1 = v10 * (1.0 - wx) + v11 * wx;
    v0 * (1.0 - wy) + v1 * wy
}
