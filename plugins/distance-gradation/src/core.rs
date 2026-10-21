// SPDX-License-Identifier: Apache-2.0
// onmk implementation (2026); upstream attribution and changes: see package NOTICE.
//! Straight RGBA at both boundaries. Only composition uses associated RGB.
//! Distance units are full-resolution layer pixels, including unequal downsampling.

#[derive(Clone, Debug)]
pub struct Image {
    pub width: usize,
    pub height: usize,
    pub left: i64,
    pub top: i64,
    pub pixels: Vec<[f32; 4]>,
}
#[derive(Debug)]
pub enum RenderError {
    Invalid,
    OutOfMemory,
}
impl Image {
    pub fn new(
        width: usize,
        height: usize,
        left: i64,
        top: i64,
        pixels: Vec<[f32; 4]>,
    ) -> Result<Self, RenderError> {
        if width.checked_mul(height) != Some(pixels.len()) {
            return Err(RenderError::Invalid);
        }
        Ok(Self {
            width,
            height,
            left,
            top,
            pixels,
        })
    }
}
#[derive(Clone, Debug)]
pub struct Settings {
    pub invert: bool,
    pub side: u8, // 1 inside, 2 outside, 3 both
    pub inside: f32,
    pub outside: f32,
    pub layer_color: bool,
    pub use_bg: bool,
    pub color: [f32; 3],
    pub bg: [f32; 3],
    pub interpolation: u8, // constant, linear, sphere, power
    pub power: f32,
    pub blur: u8, // none, box, Gaussian
    pub blur_size: f32,
    pub antialias: bool,
    pub composite: bool,
    pub opacity: f32,
    pub scale: [f32; 2],
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            invert: false,
            side: 1,
            inside: 128.0,
            outside: 128.0,
            layer_color: false,
            use_bg: false,
            color: [1.0, 0.0, 0.0],
            bg: [0.0; 3],
            interpolation: 2,
            power: 1.0,
            blur: 1,
            blur_size: 0.0,
            antialias: true,
            composite: true,
            opacity: 1.0,
            scale: [1.0; 2],
        }
    }
}
fn alloc<T: Clone>(n: usize, value: T) -> Result<Vec<T>, RenderError> {
    let mut v = Vec::new();
    v.try_reserve_exact(n)
        .map_err(|_| RenderError::OutOfMemory)?;
    v.resize(n, value);
    Ok(v)
}
pub fn alpha(a: f32) -> f32 {
    if a.is_finite() {
        a.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

// Lower envelope of squared parabolas, with arbitrary finite seed costs.
// Empty lines remain infinity; no infinity-minus-infinity intersection.
fn edt_line(f: &[f64], out: &mut [f64], spacing: f64, v: &mut [usize], z: &mut [f64]) {
    let mut count = 0usize;
    let s2 = spacing * spacing;
    for (q, &fq) in f.iter().enumerate() {
        if !fq.is_finite() {
            continue;
        }
        let mut cut = f64::NEG_INFINITY;
        while count > 0 {
            let p = v[count - 1];
            cut = ((fq - f[p]) / s2 + (q * q) as f64 - (p * p) as f64) / (2.0 * (q - p) as f64);
            if cut > z[count - 1] {
                break;
            }
            count -= 1;
        }
        if count == 0 {
            cut = f64::NEG_INFINITY;
        }
        v[count] = q;
        z[count] = cut;
        count += 1;
    }
    if count == 0 {
        out.fill(f64::INFINITY);
        return;
    }
    z[count] = f64::INFINITY;
    let mut k = 0;
    for (q, d) in out.iter_mut().enumerate() {
        while z[k + 1] < (q as f64) {
            k += 1;
        }
        let delta = q as f64 - v[k] as f64;
        *d = f[v[k]] + delta * delta * s2;
    }
}
fn edt(data: &mut [f64], w: usize, h: usize, spacing: [f64; 2]) -> Result<(), RenderError> {
    let n = w.max(h);
    let mut f = alloc(n, 0.0)?;
    let mut d = alloc(n, 0.0)?;
    let mut v = alloc(n, 0usize)?;
    let mut z = alloc(n + 1, 0.0)?;
    for y in 0..h {
        f[..w].copy_from_slice(&data[y * w..(y + 1) * w]);
        edt_line(&f[..w], &mut d[..w], spacing[0], &mut v, &mut z);
        data[y * w..(y + 1) * w].copy_from_slice(&d[..w]);
    }
    for x in 0..w {
        for y in 0..h {
            f[y] = data[y * w + x];
        }
        edt_line(&f[..h], &mut d[..h], spacing[1], &mut v, &mut z);
        for y in 0..h {
            data[y * w + x] = d[y];
        }
    }
    Ok(())
}
pub fn signed_distance(img: &Image, s: &Settings) -> Result<Vec<f32>, RenderError> {
    let n = img.pixels.len();
    let mut outside = alloc(n, f64::INFINITY)?;
    let mut inside = alloc(n, f64::INFINITY)?;
    let spacing = [1.0 / s.scale[0] as f64, 1.0 / s.scale[1] as f64];
    let edge = spacing[0].min(spacing[1]);
    for (i, p) in img.pixels.iter().enumerate() {
        let a = alpha(p[3]) as f64;
        if s.antialias && a > 0.0 && a < 1.0 {
            outside[i] = ((0.5 - a).max(0.0) * edge).powi(2);
            inside[i] = ((a - 0.5).max(0.0) * edge).powi(2);
        } else if a >= 0.5 {
            outside[i] = 0.0;
        } else {
            inside[i] = 0.0;
        }
    }
    edt(&mut outside, img.width, img.height, spacing)?;
    edt(&mut inside, img.width, img.height, spacing)?;
    let mut result = alloc(n, 0.0)?;
    for i in 0..n {
        let d = outside[i].sqrt() - inside[i].sqrt();
        // Fractional-alpha seeds encode a subpixel contour position. Binary
        // samples use center-to-center distance, as in the hard-mask EDT.
        result[i] = d as f32;
    }
    Ok(result)
}

fn blur_field(f: &mut [f32], w: usize, h: usize, s: &Settings) -> Result<(), RenderError> {
    if s.blur == 1 || s.blur_size <= 0.0 || w == 0 || h == 0 {
        return Ok(());
    }
    let mut tmp = alloc(f.len(), 0.0)?;
    for axis in 0..2 {
        let radius = (s.blur_size * s.scale[axis]).round().max(1.0) as usize;
        let length = if axis == 0 { w } else { h };
        let lines = if axis == 0 { h } else { w };
        let at = |line: usize, pos: usize| {
            if axis == 0 {
                line * w + pos
            } else {
                pos * w + line
            }
        };
        if s.blur == 2 {
            for line in 0..lines {
                let sample =
                    |pos: i64| f[at(line, pos.clamp(0, length as i64 - 1) as usize)] as f64;
                let mut sum = 0.0;
                for j in -(radius as i64)..=radius as i64 {
                    sum += sample(j);
                }
                for x in 0..length {
                    tmp[at(line, x)] = (sum / (2 * radius + 1) as f64) as f32;
                    sum += sample(x as i64 + radius as i64 + 1) - sample(x as i64 - radius as i64);
                }
            }
        } else {
            let sigma = (radius as f64 * 0.3 + 0.5).max(0.5);
            let mut kernel = alloc(2 * radius + 1, 0.0f64)?;
            for (i, k) in kernel.iter_mut().enumerate() {
                *k = (-0.5 * ((i as f64 - radius as f64) / sigma).powi(2)).exp();
            }
            let sum: f64 = kernel.iter().sum();
            for k in &mut kernel {
                *k /= sum;
            }
            for line in 0..lines {
                for x in 0..length {
                    let mut total = 0.0;
                    for (j, &k) in kernel.iter().enumerate() {
                        let p = (x as i64 + j as i64 - radius as i64).clamp(0, length as i64 - 1)
                            as usize;
                        total += f[at(line, p)] as f64 * k;
                    }
                    tmp[at(line, x)] = total as f32;
                }
            }
        }
        f.copy_from_slice(&tmp);
    }
    Ok(())
}

/// Independent foreground over original source; no opaque canvas is introduced.
pub fn over(fg: [f32; 4], src: [f32; 4]) -> [f32; 4] {
    let fa = alpha(fg[3]);
    let sa = alpha(src[3]);
    if fa == 0.0 {
        return src;
    }
    let a = fa + sa * (1.0 - fa);
    let mut out = [0.0; 4];
    out[3] = a;
    for c in 0..3 {
        out[c] = (fg[c] * fa + src[c] * sa * (1.0 - fa)) / a;
    }
    out
}

pub fn render(img: &Image, s: &Settings) -> Result<Vec<[f32; 4]>, RenderError> {
    if img.width.checked_mul(img.height) != Some(img.pixels.len())
        || !s
            .scale
            .iter()
            .all(|x| x.is_finite() && *x > 0.0 && *x <= 1.0)
        || ![s.inside, s.outside, s.power, s.blur_size, s.opacity]
            .iter()
            .all(|x| x.is_finite())
        || s.inside < 0.0
        || s.outside < 0.0
        || s.blur_size < 0.0
        || s.blur_size > 500.0
        || s.power <= 0.0
    {
        return Err(RenderError::Invalid);
    }
    if img.pixels.is_empty() {
        return Ok(Vec::new());
    }
    // An empty source has no contour, even with inversion/background enabled.
    if !img.pixels.iter().any(|p| alpha(p[3]) > 0.0) {
        return Ok(img.pixels.clone());
    }
    let distance = signed_distance(img, s)?;
    let mut field = alloc(img.pixels.len(), 0.0f32)?;
    let mut max_in = 0.0f32;
    let mut max_out = 0.0f32;
    for &d in &distance {
        max_in = max_in.max((-d).max(0.0));
        max_out = max_out.max(d.max(0.0));
    }
    let limits = [s.inside.min(max_in), s.outside.min(max_out)];
    for (i, &d) in distance.iter().enumerate() {
        let limit = if d <= 0.0 { limits[0] } else { limits[1] };
        let dist = d.abs();
        let mut t = if s.interpolation == 1 {
            if s.antialias {
                ((limit - dist) * s.scale[0].min(s.scale[1]) + 0.5).clamp(0.0, 1.0)
            } else if dist <= limit {
                1.0
            } else {
                0.0
            }
        } else if limit > 0.0 {
            (1.0 - dist / limit).clamp(0.0, 1.0)
        } else {
            0.0
        };
        if !dist.is_finite() {
            t = 0.0;
        }
        if s.invert {
            t = 1.0 - t;
        }
        field[i] = t;
    }
    blur_field(&mut field, img.width, img.height, s)?;
    let mut result = alloc(img.pixels.len(), [0.0; 4])?;
    for (i, src) in img.pixels.iter().enumerate() {
        let mut t = field[i].clamp(0.0, 1.0);
        if s.interpolation == 3 {
            t = (1.0 - (1.0 - t).powi(2)).max(0.0).sqrt();
        }
        if s.interpolation == 4 {
            t = t.powf(s.power);
        }
        let coverage = if s.antialias {
            alpha(src[3])
        } else if alpha(src[3]) >= 0.5 {
            1.0
        } else {
            0.0
        };
        let region = match s.side {
            1 => coverage,
            2 => 1.0 - coverage,
            _ => 1.0,
        };
        let color = if s.layer_color {
            [src[0], src[1], src[2]]
        } else {
            s.color
        };
        let mut fg = [color[0], color[1], color[2], region * t * alpha(s.opacity)];
        if s.use_bg {
            for c in 0..3 {
                fg[c] = s.bg[c] * (1.0 - t) + color[c] * t;
            }
            fg[3] = region * alpha(s.opacity);
        }
        result[i] = if s.composite { over(fg, *src) } else { fg };
    }
    Ok(result)
}
