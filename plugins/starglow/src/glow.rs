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

/// Main processing pipeline
pub fn process(
    ep: &EffectParams, src: &[u8], w: usize, h: usize,
    luma_map: Option<&[f64]>,
) -> Vec<u8> {
    let n = w * h;

    // Step 1: Extract brightness and apply threshold
    let brightness = extract_channel(src, w, h, ep.input_channel);
    let thresholded = apply_threshold(&brightness, ep.threshold, ep.threshold_soft);

    // Step 2: Build bright pixel buffer, modulated by map
    let mut bright_r = vec![0.0f32; n];
    let mut bright_g = vec![0.0f32; n];
    let mut bright_b = vec![0.0f32; n];
    for i in 0..n {
        let mut t = thresholded[i] as f32 * ep.boost_light as f32;

        // Apply luminance map modulation
        if let Some(map) = luma_map {
            t *= map[i] as f32;
        }

        let off = i * 4; // ARGB
        bright_r[i] = src[off + 1] as f32 / 255.0 * t;
        bright_g[i] = src[off + 2] as f32 / 255.0 * t;
        bright_b[i] = src[off + 3] as f32 / 255.0 * t;
    }

    // Step 3: Get shape weights and accumulate streaks
    let weights = shape_weights(ep.glow_shape);
    let mut glow_r = vec![0.0f32; n];
    let mut glow_g = vec![0.0f32; n];
    let mut glow_b = vec![0.0f32; n];

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
        );

        for i in 0..n {
            glow_r[i] += sr[i];
            glow_g[i] += sg[i];
            glow_b[i] += sb[i];
        }
    }

    // Step 4: Composite
    let mut output = vec![0u8; n * 4];
    let src_a = ep.source_opacity as f32;
    let glow_a = ep.starglow_opacity as f32;

    for i in 0..n {
        let off = i * 4;
        let sr = src[off + 1] as f32 / 255.0;
        let sg = src[off + 2] as f32 / 255.0;
        let sb = src[off + 3] as f32 / 255.0;

        let gr = glow_r[i] * glow_a;
        let gg = glow_g[i] * glow_a;
        let gb = glow_b[i] * glow_a;

        let (cr, cg, cb) = blend::composite(sr, sg, sb, gr, gg, gb, src_a, ep.transfer_mode);

        output[off] = src[off]; // alpha passthrough
        output[off + 1] = (cr * 255.0).clamp(0.0, 255.0) as u8;
        output[off + 2] = (cg * 255.0).clamp(0.0, 255.0) as u8;
        output[off + 3] = (cb * 255.0).clamp(0.0, 255.0) as u8;
    }

    output
}

fn extract_channel(src: &[u8], w: usize, h: usize, channel: i32) -> Vec<f64> {
    let n = w * h;
    let mut out = vec![0.0f64; n];
    for i in 0..n {
        let off = i * 4;
        let a = src[off] as f64 / 255.0;
        let r = src[off + 1] as f64 / 255.0;
        let g = src[off + 2] as f64 / 255.0;
        let b = src[off + 3] as f64 / 255.0;
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
    out
}

fn apply_threshold(brightness: &[f64], threshold: f64, soft: f64) -> Vec<f64> {
    let mut out = vec![0.0f64; brightness.len()];
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
    out
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
) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
    let n = w * h;
    let mut streak_r = vec![0.0f32; n];
    let mut streak_g = vec![0.0f32; n];
    let mut streak_b = vec![0.0f32; n];

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
    let x_iter: Vec<usize> = if dx > 0 { (0..w).collect() } else { (0..w).rev().collect() };
    let y_iter: Vec<usize> = if dy > 0 { (0..h).collect() } else { (0..h).rev().collect() };

    // Pass 1: IIR propagation
    for &y in &y_iter {
        for &x in &x_iter {
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
    let mut out_r = vec![0.0f32; n];
    let mut out_g = vec![0.0f32; n];
    let mut out_b = vec![0.0f32; n];

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

    (out_r, out_g, out_b)
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
