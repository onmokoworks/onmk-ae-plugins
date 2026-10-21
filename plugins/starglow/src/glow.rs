use crate::{EffectParams, blend, colormap};

/// 8 directions: up, down, left, right, up-left, up-right, down-left, down-right
/// (dx, dy, is_diagonal)
const DIRECTIONS: [(i32, i32, bool); 8] = [
    (0, -1, false),  // up
    (0, 1, false),   // down
    (-1, 0, false),  // left
    (1, 0, false),   // right
    (-1, -1, true),  // up-left
    (1, -1, true),   // up-right
    (-1, 1, true),   // down-left
    (1, 1, true),    // down-right
];

/// Shape presets: weight multipliers for each of the 8 directions
/// [up, down, left, right, up-left, up-right, down-left, down-right]
fn shape_weights(shape: i32) -> [f64; 8] {
    match shape {
        1 => [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],       // Star (all 8)
        2 => [1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0],       // Cross (+)
        3 => [0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0],       // X
        4 => [0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0],       // H (horizontal)
        5 => [1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],       // V (vertical)
        6 => [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0],       // Tri (up + down-left + down-right)
        7 => [0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0],       // Y (down + up-left + up-right)
        8 => [1.0, 1.0, 1.0, 1.0, 0.5, 0.5, 0.5, 0.5],       // 6-Point (cardinal full + diag half)
        9 => [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],       // Custom (use individual sliders)
        _ => [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
    }
}

pub trait Channel: Copy + Default {
    const PACKED_BYTE: bool = false;
    fn normalized(self) -> f64;
    fn normalized_f32(self) -> f32;
    fn rgb(v: f32) -> Self;
    fn alpha(v: f32) -> Self;
}
impl Channel for u8 {
    const PACKED_BYTE: bool = true;
    fn normalized(self) -> f64 { self as f64 / 255.0 }
    fn normalized_f32(self) -> f32 { self as f32 / 255.0 }
    fn rgb(v: f32) -> Self { (v * 255.0).clamp(0.0, 255.0) as u8 }
    fn alpha(v: f32) -> Self { (v * 255.0 + 0.5).clamp(0.0, 255.0) as u8 }
}
impl Channel for u16 {
    fn normalized(self) -> f64 { self as f64 / 32768.0 }
    fn normalized_f32(self) -> f32 { self as f32 / 32768.0 }
    fn rgb(v: f32) -> Self { (v * 32768.0).round().clamp(0.0, 32768.0) as u16 }
    fn alpha(v: f32) -> Self { Self::rgb(v) }
}
impl Channel for f32 {
    fn normalized(self) -> f64 { self as f64 }
    fn normalized_f32(self) -> f32 { self }
    fn rgb(v: f32) -> Self { v }
    fn alpha(v: f32) -> Self { v.clamp(0.0, 1.0) }
}

type RenderResult<T> = Result<T, after_effects::Error>;

pub(crate) fn filled<T: Copy + Default>(len: usize) -> RenderResult<Vec<T>> {
    let mut values = Vec::new();
    values.try_reserve_exact(len).map_err(|_| after_effects::Error::OutOfMemory)?;
    values.resize(len, T::default());
    Ok(values)
}

pub fn process(ep: &EffectParams, src: &[u8], w: usize, h: usize, map: Option<&[f64]>) -> RenderResult<Vec<u8>> {
    process_channels(ep, src, w, h, map)
}

/// Native-channel processing; existing byte rounding remains unchanged.
pub fn process_channels<T: Channel>(
    ep: &EffectParams, src: &[T], w: usize, h: usize,
    luma_map: Option<&[f64]>,
) -> RenderResult<Vec<T>> {
    if w == 0 || h == 0 || w > i32::MAX as usize || h > i32::MAX as usize {
        return Err(after_effects::Error::BadCallbackParameter);
    }
    let n = w.checked_mul(h).ok_or(after_effects::Error::BadCallbackParameter)?;
    let channels = n.checked_mul(4).ok_or(after_effects::Error::BadCallbackParameter)?;
    if src.len() != channels || luma_map.is_some_and(|map| map.len() != n) {
        return Err(after_effects::Error::BadCallbackParameter);
    }

    // Step 1: Extract brightness and apply threshold
    let brightness = extract_channel(src, w, h, ep.input_channel)?;
    let thresholded = apply_threshold(&brightness, ep.threshold, ep.threshold_soft)?;

    // Step 2: Build bright pixel buffer, modulated by map
    let mut bright_r = filled::<f32>(n)?;
    let mut bright_g = filled::<f32>(n)?;
    let mut bright_b = filled::<f32>(n)?;
    for i in 0..n {
        let mut t = thresholded[i] as f32 * ep.boost_light as f32;

        // Apply luminance map modulation
        if let Some(map) = luma_map {
            t *= map[i] as f32;
        }

        let off = i * 4; // ARGB
        bright_r[i] = src[off + 1].normalized_f32() * t;
        bright_g[i] = src[off + 2].normalized_f32() * t;
        bright_b[i] = src[off + 3].normalized_f32() * t;
    }

    // Step 3: Get shape weights and accumulate streaks
    let weights = shape_weights(ep.glow_shape);
    let mut glow_r = filled::<f32>(n)?;
    let mut glow_g = filled::<f32>(n)?;
    let mut glow_b = filled::<f32>(n)?;

    let is_spectrum = ep.colormap_preset == 13;
    let cmap = colormap::get_colormap(ep);

    for (dir_idx, &(dx, dy, is_diag)) in DIRECTIONS.iter().enumerate() {
        let _ = dir_idx;
        // Effective weight: shape weight × individual length multiplier
        let shape_w = if ep.glow_shape == 9 { 1.0 } else { weights[dir_idx] };
        let indiv = ep.lengths[dir_idx];
        let total_weight = shape_w * indiv;
        if total_weight <= 0.0 {
            continue;
        }

        let length = (ep.streak_length * total_weight).max(0.0) as usize;
        if length == 0 {
            continue;
        }

        let (sr, sg, sb) = generate_streak(
            &bright_r, &bright_g, &bright_b,
            w, h, dx, dy, is_diag, length,
            ep.decay_rate,
            &cmap,
            ep.shimmer_amount as f32,
            ep.shimmer_detail as f32,
            ep.shimmer_phase as f32,
            is_spectrum,
            ep.spectrum_offset as f32,
            ep.spectrum_density as f32,
            ep.spectrum_random as f32,
        )?;

        for i in 0..n {
            glow_r[i] += sr[i];
            glow_g[i] += sg[i];
            glow_b[i] += sb[i];
        }
    }

    // Step 4: Composite
    let mut output = filled::<T>(channels)?;
    let src_a = ep.source_opacity as f32;
    let glow_a = ep.starglow_opacity as f32;

    for i in 0..n {
        let off = i * 4;
        let sr = src[off + 1].normalized_f32();
        let sg = src[off + 2].normalized_f32();
        let sb = src[off + 3].normalized_f32();

        let gr = glow_r[i] * glow_a;
        let gg = glow_g[i] * glow_a;
        let gb = glow_b[i] * glow_a;

        let (cr, cg, cb) = blend::composite(sr, sg, sb, gr, gg, gb, src_a, ep.transfer_mode);

        let source_alpha = src[off].normalized_f32();
        let glow_alpha = if ep.affect_alpha {
            gr.max(gg).max(gb).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let output_alpha = source_alpha + glow_alpha * (1.0 - source_alpha);
        output[off] = T::alpha(output_alpha);
        output[off + 1] = T::rgb(cr);
        output[off + 2] = T::rgb(cg);
        output[off + 3] = T::rgb(cb);
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> EffectParams {
        EffectParams { source_opacity: 1.0, transfer_mode: 4,
            input_channel: 4, colors: [[1.0; 3]; 5], ..Default::default() }
    }

    #[test]
    fn invalid_geometry_and_capacity_are_errors() {
        for (w,h) in [(0,1),(1,0),(usize::MAX,2),(i32::MAX as usize+1,1),(2,1)] {
            assert_eq!(process_channels(&params(), &[0u8;4], w,h,None), Err(after_effects::Error::BadCallbackParameter));
        }
        for map in [&[][..], &[0.0,1.0][..]] {
            assert_eq!(process_channels(&params(), &[0u8;4],1,1,Some(map)), Err(after_effects::Error::BadCallbackParameter));
        }
        assert_eq!(filled::<f64>(usize::MAX), Err(after_effects::Error::OutOfMemory));
    }

    #[test]
    fn native_cpu_identity_preserves_every_16bit_level() {
        let mut src = Vec::new();
        for value in 0u16..=32768 { src.extend([32768, value, value, value]); }
        assert_eq!(process_channels(&params(), &src, 32769, 1, None).unwrap(), src);
    }

    #[test]
    fn float_cpu_identity_retains_hdr_and_negative_rgb() {
        let src = [1.0f32, -0.25, 2.5, 0.5000305];
        assert_eq!(process_channels(&params(), &src, 1, 1, None).unwrap(), src);
    }

    #[test]
    fn threshold_distinguishes_sub_byte_input_levels() {
        let src = [32768u16,16384,0,0,32768,16385,0,0];
        let brightness = extract_channel(&src,2,1,4).unwrap();
        let mask = apply_threshold(&brightness,0.5,0.001).unwrap();
        assert!(mask[1] > mask[0]);
    }

    #[test]
    fn deep_streak_generates_sub_byte_glow() {
        let ep = EffectParams { source_opacity: 0.0, starglow_opacity: 1.0,
            boost_light: 0.001, threshold: 0.0, threshold_soft: 0.0,
            glow_shape: 4, lengths: [1.0;8], streak_length: 5.0,
            decay_rate: 0.5, ..params() };
        let src = [32768u16,0,0,0,32768,32768,32768,32768,32768,0,0,0];
        let out = process_channels(&ep,&src,3,1,None).unwrap();
        assert!(out[1..4].iter().any(|&v| v>0 && v<128), "real low-level glow: {out:?}");
    }

    #[test]
    fn rendered_glow_alpha_obeys_switch_and_source_coverage() {
        let mut ep = EffectParams { source_opacity: 0.0, starglow_opacity: 1.0,
            boost_light: 0.1, threshold: 0.0, threshold_soft: 0.0,
            glow_shape: 4, lengths: [1.0;8], streak_length: 5.0,
            decay_rate: 0.5, ..params() };
        for alpha in [0.0f32, 0.25, 1.0] {
            let src = [alpha,0.0,0.0,0.0, 1.0,1.0,1.0,1.0, alpha,0.0,0.0,0.0];
            ep.affect_alpha = false;
            let preserved = process_channels(&ep,&src,3,1,None).unwrap();
            ep.affect_alpha = true;
            let extended = process_channels(&ep,&src,3,1,None).unwrap();
            for offset in [0,8] {
                // Add with source opacity zero exposes the actual generated glow RGB.
                let glow = extended[offset+1..offset+4].iter().copied().fold(0.0f32,f32::max);
                assert!(glow>0.0 && glow<1.0,"expected a non-saturated neighboring streak");
                assert_eq!(preserved[offset],alpha);
                assert_eq!(&preserved[offset+1..offset+4],&extended[offset+1..offset+4]);
                let expected = alpha+(1.0-alpha)*glow;
                assert!((extended[offset]-expected).abs()<1e-6);
            }
        }
    }
}

fn extract_channel<T: Channel>(src: &[T], w: usize, h: usize, channel: i32) -> RenderResult<Vec<f64>> {
    let n = w * h;
    let mut out = filled::<f64>(n)?;
    for i in 0..n {
        let off = i * 4;
        let a = src[off].normalized();
        let r = src[off + 1].normalized();
        let g = src[off + 2].normalized();
        let b = src[off + 3].normalized();
        out[i] = match channel {
            1 => { let mx = r.max(g).max(b); let mn = r.min(g).min(b); (mx + mn) * 0.5 }
            2 => 0.2126 * r + 0.7152 * g + 0.0722 * b,
            3 => a,
            4 => r,
            5 => g,
            6 => b,
            _ => 0.2126 * r + 0.7152 * g + 0.0722 * b,
        };
    }
    Ok(out)
}

fn apply_threshold(brightness: &[f64], threshold: f64, soft: f64) -> RenderResult<Vec<f64>> {
    let mut out = filled::<f64>(brightness.len())?;
    let soft_range = soft.max(0.001);
    let low = threshold - soft_range * 0.5;
    let high = threshold + soft_range * 0.5;
    for (i, &v) in brightness.iter().enumerate() {
        out[i] = if v >= high {
            1.0
        } else if v <= low {
            0.0
        } else {
            let t = (v - low) / (high - low);
            t * t * (3.0 - 2.0 * t)
        };
    }
    Ok(out)
}

/// Additive IIR streak generation with diagonal correction and decay rate control.
fn generate_streak(
    bright_r: &[f32], bright_g: &[f32], bright_b: &[f32],
    w: usize, h: usize,
    dx: i32, dy: i32,
    is_diagonal: bool,
    length: usize,
    decay_rate: f64,
    cmap: &[[f32; 3]; 5],
    shimmer_amount: f32,
    shimmer_detail: f32,
    shimmer_phase: f32,
    is_spectrum: bool,
    spectrum_offset: f32,
    spectrum_density: f32,
    spectrum_random: f32,
) -> RenderResult<(Vec<f32>, Vec<f32>, Vec<f32>)> {
    let n = w * h;
    let mut streak_r = filled::<f32>(n)?;
    let mut streak_g = filled::<f32>(n)?;
    let mut streak_b = filled::<f32>(n)?;

    // Diagonal correction: each pixel step covers √2 distance
    let step_distance = if is_diagonal { std::f64::consts::SQRT_2 } else { 1.0 };

    // Decay: reaches target_floor at `length` visual pixels.
    // decay_rate controls how aggressive the falloff is:
    //   rate=1.0 → normal (reaches ~1% at length)
    //   rate>1.0 → faster decay (shorter visible streak)
    //   rate<1.0 → slower decay (longer visible streak)
    let target_floor = 0.01f64;
    let effective_length = length as f64 / step_distance; // pixel steps to cover `length` visual pixels
    let decay = if effective_length > 1.0 {
        target_floor.powf(decay_rate / effective_length) as f32
    } else {
        0.5f32
    };

    // Iteration order: (x - dx, y - dy) must be already processed
    // Pass 1: IIR propagation
    for row in 0..h {
        let y = if dy > 0 { row } else { h - 1 - row };
        for col in 0..w {
            let x = if dx > 0 { col } else { w - 1 - col };
            let i = y * w + x;
            let sx = x as i32 - dx;
            let sy = y as i32 - dy;

            let mut prev_r = 0.0f32;
            let mut prev_g = 0.0f32;
            let mut prev_b = 0.0f32;

            if sx >= 0 && sx < w as i32 && sy >= 0 && sy < h as i32 {
                let si = sy as usize * w + sx as usize;
                let shimmer = if shimmer_amount > 0.0 {
                    let noise = value_noise(
                        x as f32 * shimmer_detail * 0.02,
                        y as f32 * shimmer_detail * 0.02,
                        shimmer_phase,
                    );
                    1.0 - shimmer_amount * (1.0 - noise)
                } else {
                    1.0
                };
                let d = decay * shimmer;
                prev_r = streak_r[si] * d;
                prev_g = streak_g[si] * d;
                prev_b = streak_b[si] * d;
            }

            streak_r[i] = bright_r[i] + prev_r;
            streak_g[i] = bright_g[i] + prev_g;
            streak_b[i] = bright_b[i] + prev_b;
        }
    }

    // Pass 2: Extract glow (subtract source bright), apply colormap
    let mut out_r = filled::<f32>(n)?;
    let mut out_g = filled::<f32>(n)?;
    let mut out_b = filled::<f32>(n)?;

    for i in 0..n {
        let sr = streak_r[i];
        let sg = streak_g[i];
        let sb = streak_b[i];
        let streak_lum = sr * 0.2126 + sg * 0.7152 + sb * 0.0722;

        if streak_lum <= 0.0001 {
            continue;
        }

        // Pure glow = streak - bright (removes source contribution at the center)
        let glow_r = (sr - bright_r[i]).max(0.0);
        let glow_g = (sg - bright_g[i]).max(0.0);
        let glow_b = (sb - bright_b[i]).max(0.0);
        let glow_lum = glow_r * 0.2126 + glow_g * 0.7152 + glow_b * 0.0722;

        // Depth for colormap: 0 near source, 1 at streak tip
        let depth = if streak_lum > 0.0001 {
            (glow_lum / streak_lum).clamp(0.0, 1.0)
        } else {
            0.0
        };

        let [cr, cg, cb] = if is_spectrum {
            let x = (i % w) as f32;
            let y = (i / w) as f32;
            let rand_off = if spectrum_random > 0.0 {
                hash_f32(x as i32, y as i32) * 360.0 * spectrum_random
            } else {
                0.0
            };
            colormap::sample_spectrum(depth, spectrum_offset, spectrum_density, rand_off)
        } else {
            colormap::sample_gradient(cmap, depth)
        };
        out_r[i] = glow_lum * cr;
        out_g[i] = glow_lum * cg;
        out_b[i] = glow_lum * cb;
    }

    Ok((out_r, out_g, out_b))
}

fn value_noise(x: f32, y: f32, phase: f32) -> f32 {
    let x = x + phase * 0.137;
    let ix = x.floor() as i32;
    let iy = y.floor() as i32;
    let fx = x - x.floor();
    let fy = y - y.floor();
    let fx = fx * fx * fx * (fx * (fx * 6.0 - 15.0) + 10.0);
    let fy = fy * fy * fy * (fy * (fy * 6.0 - 15.0) + 10.0);
    let n00 = hash_f32(ix, iy);
    let n10 = hash_f32(ix + 1, iy);
    let n01 = hash_f32(ix, iy + 1);
    let n11 = hash_f32(ix + 1, iy + 1);
    let n0 = n00 + (n10 - n00) * fx;
    let n1 = n01 + (n11 - n01) * fx;
    n0 + (n1 - n0) * fy
}

fn hash_f32(x: i32, y: i32) -> f32 {
    let mut n = x.wrapping_mul(374761393).wrapping_add(y.wrapping_mul(668265263));
    n = (n ^ (n >> 13)).wrapping_mul(1274126177);
    n = n ^ (n >> 16);
    (n & 0x7FFFFFFF) as f32 / 0x7FFFFFFF as f32
}
