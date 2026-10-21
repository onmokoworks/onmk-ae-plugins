//! Core UV-map texture projection.
//!
//! Pixels are stored as normalized, premultiplied `[A, R, G, B]` floats. Keeping
//! UV coordinates as floats avoids the 8-bit coordinate quantization that made
//! Blender UV passes look stepped or noisy.

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WrapMode {
    Clamp,
    Repeat,
    Mirror,
}

impl WrapMode {
    pub fn from_popup(v: i32) -> WrapMode {
        match v {
            2 => WrapMode::Repeat,
            3 => WrapMode::Mirror,
            _ => WrapMode::Clamp,
        }
    }
}

pub struct UvParams {
    pub v_origin_bottom: bool,
    pub wrap: WrapMode,
    pub bilinear: bool,
    pub use_uv_alpha: bool,
    pub opacity: f32,
}

pub struct Image {
    pub pixels: Vec<[f32; 4]>,
    pub width: usize,
    pub height: usize,
}

impl Image {
    fn count(width: usize, height: usize) -> Result<usize, after_effects::Error> {
        if width > i32::MAX as usize || height > i32::MAX as usize {
            return Err(after_effects::Error::BadCallbackParameter);
        }
        let count = width.checked_mul(height).ok_or(after_effects::Error::OutOfMemory)?;
        if count > isize::MAX as usize / std::mem::size_of::<[f32; 4]>() {
            return Err(after_effects::Error::OutOfMemory);
        }
        Ok(count)
    }

    pub fn validate(&self) -> Result<(), after_effects::Error> {
        if self.pixels.len() != Self::count(self.width, self.height)? {
            return Err(after_effects::Error::BadCallbackParameter);
        }
        Ok(())
    }

    pub fn transparent(width: usize, height: usize) -> Result<Self, after_effects::Error> {
        let count = Self::count(width, height)?;
        let mut pixels = Vec::new();
        pixels.try_reserve_exact(count).map_err(|_| after_effects::Error::OutOfMemory)?;
        pixels.resize(count, [0.0; 4]);
        Ok(Self {
            pixels,
            width,
            height,
        })
    }
}

/// Generates an identity UV map for a flat image plane.
///
/// Coordinates point at pixel centers so projecting an equally sized texture
/// through the generated map reproduces the texture without a half-pixel shift.
pub fn generate_planar(width: usize, height: usize, v_origin_bottom: bool) -> Result<Image, after_effects::Error> {
    let mut out = Image::transparent(width, height)?;
    if width == 0 || height == 0 {
        return Ok(out);
    }

    for y in 0..height {
        let top_origin_v = (y as f32 + 0.5) / height as f32;
        let v = if v_origin_bottom {
            1.0 - top_origin_v
        } else {
            top_origin_v
        };
        for x in 0..width {
            let u = (x as f32 + 0.5) / width as f32;
            out.pixels[y * width + x] = [1.0, u, v, 0.0];
        }
    }

    Ok(out)
}

#[inline]
fn wrap_index(i: i64, n: i64, mode: WrapMode) -> i64 {
    if n <= 1 {
        return 0;
    }
    match mode {
        WrapMode::Clamp => i.clamp(0, n - 1),
        WrapMode::Repeat => i.rem_euclid(n),
        WrapMode::Mirror => {
            let period = 2 * n;
            let m = i.rem_euclid(period);
            if m < n {
                m
            } else {
                period - 1 - m
            }
        }
    }
}

#[inline]
fn texel(image: &Image, x: i64, y: i64, wrap: WrapMode) -> [f32; 4] {
    let xx = wrap_index(x, image.width as i64, wrap) as usize;
    let yy = wrap_index(y, image.height as i64, wrap) as usize;
    image.pixels[yy * image.width + xx]
}

#[inline]
fn bilinear_sample(image: &Image, fx: f32, fy: f32, wrap: WrapMode) -> [f32; 4] {
    // Finite decoded UVs can span 65536 texture widths. Pixel indices must
    // remain wide until wrapping, including the adjacent bilinear sample.
    let x0 = fx.floor() as i64;
    let y0 = fy.floor() as i64;
    let dx = fx - x0 as f32;
    let dy = fy - y0 as f32;
    let c00 = texel(image, x0, y0, wrap);
    let c10 = texel(image, x0 + 1, y0, wrap);
    let c01 = texel(image, x0, y0 + 1, wrap);
    let c11 = texel(image, x0 + 1, y0 + 1, wrap);
    let mut result = [0.0; 4];
    for k in 0..4 {
        let top = c00[k] * (1.0 - dx) + c10[k] * dx;
        let bottom = c01[k] * (1.0 - dx) + c11[k] * dx;
        result[k] = top * (1.0 - dy) + bottom * dy;
    }
    result
}

/// Samples a UV map at the center of an output pixel. This also lets the
/// Texture-input mode work when the applied layer and UV-map layer differ in
/// resolution, while the effect output remains the applied layer's size.
#[inline]
fn uv_at_output_pixel(uv: &Image, x: usize, y: usize, out_w: usize, out_h: usize) -> [f32; 4] {
    let fx = (x as f32 + 0.5) * uv.width as f32 / out_w as f32 - 0.5;
    let fy = (y as f32 + 0.5) * uv.height as f32 / out_h as f32 - 0.5;
    bilinear_sample(uv, fx, fy, WrapMode::Clamp)
}

pub fn project(p: &UvParams, uv: &Image, texture: &Image, out_w: usize, out_h: usize) -> Result<Image, after_effects::Error> {
    uv.validate()?;
    texture.validate()?;
    let mut out = Image::transparent(out_w, out_h)?;
    if uv.width == 0
        || uv.height == 0
        || texture.width == 0
        || texture.height == 0
        || out_w == 0
        || out_h == 0
    {
        return Ok(out);
    }

    for y in 0..out_h {
        for x in 0..out_w {
            let uv_pixel = uv_at_output_pixel(uv, x, y, out_w, out_h);
            let a_uv = uv_pixel[0].clamp(0.0, 1.0);

            // AE effect worlds are premultiplied. Blender's UV values must be
            // unpremultiplied before decoding, otherwise antialiased edges pull
            // the coordinates toward zero and create noisy fringes.
            let (mut u, mut v) = if a_uv > 1.0e-6 {
                (uv_pixel[1] / a_uv, uv_pixel[2] / a_uv)
            } else {
                (0.0, 0.0)
            };
            u = u.clamp(-65536.0, 65536.0);
            v = v.clamp(-65536.0, 65536.0);
            if p.v_origin_bottom {
                v = 1.0 - v;
            }

            let fx = u * texture.width as f32 - 0.5;
            let fy = v * texture.height as f32 - 0.5;
            let sample = if p.bilinear {
                bilinear_sample(texture, fx, fy, p.wrap)
            } else {
                texel(texture, fx.round() as i64, fy.round() as i64, p.wrap)
            };

            let mut coverage = p.opacity;
            if p.use_uv_alpha {
                coverage *= a_uv;
            }
            let dst = &mut out.pixels[y * out_w + x];
            for k in 0..4 {
                dst[k] = sample[k] * coverage;
            }
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_images_reject_invalid_geometry_before_allocation_or_sampling() {
        assert!(Image::transparent(usize::MAX,2).is_err());
        assert!(Image::transparent(i32::MAX as usize,i32::MAX as usize).is_err());
        assert!(generate_planar(usize::MAX,0,false).is_err());
        assert!(Image::transparent(0,7).unwrap().pixels.is_empty());
        let valid = Image::transparent(1,1).unwrap();
        let bad = Image { width:2,height:2,pixels:vec![[0.0;4]] };
        let p = UvParams { v_origin_bottom:false,wrap:WrapMode::Clamp,bilinear:true,use_uv_alpha:true,opacity:1.0 };
        assert!(project(&p,&bad,&valid,1,1).is_err());
        assert!(project(&p,&valid,&bad,1,1).is_err());
        assert!(project(&p,&valid,&valid,usize::MAX,1).is_err());
        assert!(project(&p,&valid,&valid,0,1).unwrap().pixels.is_empty());
    }

    #[test]
    fn large_finite_uvs_preserve_wrap_and_signed_hdr_samples() {
        let width = 40000;
        let texture = Image { width, height: 1,
            pixels: (0..width).map(|i| [1.0, i as f32 / width as f32, -0.25, 2.0]).collect() };
        for u in [-65536.0f32, 65536.0] {
            let uv = Image { width: 1, height: 1, pixels: vec![[1.0,u,0.5,0.0]] };
            for wrap in [WrapMode::Clamp, WrapMode::Repeat, WrapMode::Mirror] {
                for bilinear in [false,true] {
                    let params = UvParams { v_origin_bottom:false, wrap, bilinear,
                        use_uv_alpha:true, opacity:0.5 };
                    let result = project(&params,&uv,&texture,1,1).unwrap();
                    // f32 represents u*40000 exactly here; the half-texel term
                    // is below its precision. The wrap period divides it exactly.
                    let index = if wrap == WrapMode::Clamp && u > 0.0 { width-1 } else { 0 };
                    let expected = texture.pixels[index].map(|value|value*0.5);
                    assert_eq!(result.pixels[0],expected,"u={u}, bilinear={bilinear}");
                }
            }
        }
    }

    #[test]
    fn wrap_indices_cover_negative_and_past_end_coordinates() {
        for (mode, expected) in [
            (WrapMode::Clamp, [0,0,0,0,0,1,2,2,2,2]),
            (WrapMode::Repeat, [2,0,1,2,0,1,2,0,1,2]),
            (WrapMode::Mirror, [2,2,1,0,0,1,2,2,1,0]),
        ] {
            for (index, want) in (-4..6).zip(expected) {
                assert_eq!(wrap_index(index, 3, mode), want);
                assert_eq!(wrap_index(index, 1, mode), 0);
            }
        }
    }

    #[test]
    fn bilinear_wrap_seams_match_hand_computed_colors() {
        let texture = Image { width:3, height:1,
            pixels:vec![[1.0,0.0,0.0,0.0],[1.0,0.5,0.0,0.0],[1.0,1.0,0.0,0.0]] };
        for (mode, expected) in [
            (WrapMode::Clamp, [0.0,0.0,1.0,1.0]),
            (WrapMode::Repeat, [0.75,0.5,0.5,0.25]),
            (WrapMode::Mirror, [0.25,0.0,1.0,0.75]),
        ] {
            for (x,want) in [-1.5,-0.5,2.5,3.5].into_iter().zip(expected) {
                assert_eq!(bilinear_sample(&texture,x,0.0,mode), [1.0,want,0.0,0.0]);
            }
        }
    }

    #[test]
    fn preserves_sub_8_bit_uv_precision() {
        let uv = Image {
            pixels: vec![[1.0, 0.5001, 0.0, 0.0]],
            width: 1,
            height: 1,
        };
        let texture = Image {
            pixels: vec![[1.0, 0.0, 0.0, 0.0], [1.0, 1.0, 0.0, 0.0]],
            width: 2,
            height: 1,
        };
        let result = project(
            &UvParams {
                v_origin_bottom: false,
                wrap: WrapMode::Clamp,
                bilinear: true,
                use_uv_alpha: true,
                opacity: 1.0,
            },
            &uv,
            &texture,
            1,
            1,
        ).unwrap();
        assert!((result.pixels[0][1] - 0.5002).abs() < 0.001);
    }

    #[test]
    fn unpremultiplies_uv_edges_before_sampling() {
        let uv = Image {
            pixels: vec![[0.5, 0.25, 0.0, 0.0]],
            width: 1,
            height: 1,
        };
        let texture = Image {
            pixels: vec![[1.0, 0.0, 0.0, 0.0], [1.0, 1.0, 0.0, 0.0]],
            width: 2,
            height: 1,
        };
        let result = project(
            &UvParams {
                v_origin_bottom: false,
                wrap: WrapMode::Clamp,
                bilinear: false,
                use_uv_alpha: true,
                opacity: 1.0,
            },
            &uv,
            &texture,
            1,
            1,
        ).unwrap();
        assert!((result.pixels[0][0] - 0.5).abs() < 1.0e-6);
        assert!((result.pixels[0][1] - 0.5).abs() < 1.0e-6);
    }

    #[test]
    fn planar_uv_round_trips_an_equal_size_texture() {
        let texture = Image {
            pixels: vec![
                [1.0, 1.0, 0.0, 0.0],
                [1.0, 0.0, 1.0, 0.0],
                [1.0, 0.0, 0.0, 1.0],
                [1.0, 1.0, 1.0, 1.0],
            ],
            width: 2,
            height: 2,
        };
        let uv = generate_planar(2, 2, true).unwrap();
        let result = project(
            &UvParams {
                v_origin_bottom: true,
                wrap: WrapMode::Clamp,
                bilinear: false,
                use_uv_alpha: true,
                opacity: 1.0,
            },
            &uv,
            &texture,
            2,
            2,
        ).unwrap();
        assert_eq!(result.pixels, texture.pixels);
    }

    #[test]
    fn planar_uv_respects_v_origin() {
        let top = generate_planar(1, 2, false).unwrap();
        let bottom = generate_planar(1, 2, true).unwrap();
        assert_eq!(top.pixels[0], [1.0, 0.5, 0.25, 0.0]);
        assert_eq!(top.pixels[1], [1.0, 0.5, 0.75, 0.0]);
        assert_eq!(bottom.pixels[0], [1.0, 0.5, 0.75, 0.0]);
        assert_eq!(bottom.pixels[1], [1.0, 0.5, 0.25, 0.0]);
    }
}
