//! Density-space lattice grain.

use crate::color::{luma, stock_profile};
use crate::params::FilmParams;

#[inline]
pub fn hash_u32(mut v: u32) -> u32 {
    v ^= v >> 16;
    v = v.wrapping_mul(0x7feb_352d);
    v ^= v >> 15;
    v = v.wrapping_mul(0x846c_a68b);
    v ^= v >> 16;
    v
}

#[inline]
pub fn hash_f32(v: u32) -> f32 {
    (hash_u32(v) as f32 / u32::MAX as f32) * 2.0 - 1.0
}

#[inline]
fn fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

/// Value noise on a lattice with smooth interpolation.
pub fn lattice_noise(x: f32, y: f32, seed: u32, channel: u32) -> f32 {
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;
    let ux = fade(fx);
    let uy = fade(fy);

    let h = |ix: i32, iy: i32| -> f32 {
        let n = (ix as u32)
            .wrapping_mul(0x1f12_3bb5)
            .wrapping_add((iy as u32).wrapping_mul(0x5f35_6495))
            .wrapping_add(seed)
            .wrapping_add(channel.wrapping_mul(0x9e37_79b9));
        hash_f32(n)
    };

    let n00 = h(x0, y0);
    let n10 = h(x0 + 1, y0);
    let n01 = h(x0, y0 + 1);
    let n11 = h(x0 + 1, y0 + 1);
    let nx0 = n00 + (n10 - n00) * ux;
    let nx1 = n01 + (n11 - n01) * ux;
    nx0 + (nx1 - nx0) * uy
}

fn band_weight(y: f32, shadows: f32, mids: f32, highs: f32) -> f32 {
    let l = y.clamp(0.0, 1.0);
    let shadow = (1.0 - l).powi(2);
    let highlight = l.powi(2);
    let mid = (1.0 - (2.0 * l - 1.0).abs()).max(0.0);
    let sum = shadow + mid + highlight;
    (shadow * shadows + mid * mids + highlight * highs) / sum.max(1.0e-6)
}

/// Apply multiplicative grain in density space to an RGBA f32 buffer.
pub fn apply_grain(buf: &mut [f32], w: usize, h: usize, p: &FilmParams) {
    if !p.grain_enable || p.grain_amount <= 1.0e-5 || w == 0 || h == 0 {
        return;
    }
    let profile = stock_profile(p.stock);
    let cell = (3.5 * p.grain_size.max(0.25) / p.gauge.scale()).max(0.75);
    let clump = p.grain_clump.clamp(0.0, 1.0);
    let seed = p
        .seed
        .wrapping_add(p.frame.wrapping_mul(0x9e37_79b9))
        .wrapping_add(0x00a1_1ce5);
    let amount = p.grain_amount * profile.grain_base;

    for y in 0..h {
        for x in 0..w {
            let o = (y * w + x) * 4;
            let rgb = [buf[o], buf[o + 1], buf[o + 2]];
            let yv = luma(rgb).max(1.0e-4);
            let density = (-yv.clamp(1.0e-4, 8.0).ln()).max(0.0);
            let sigma = amount
                * (density + 0.15).sqrt()
                * band_weight(yv, p.grain_shadows, p.grain_mids, p.grain_highlights);

            let fx = x as f32 / cell;
            let fy = y as f32 / cell;
            let cl = if clump > 1.0e-4 {
                0.65 + 0.35 * lattice_noise(fx * 0.35, fy * 0.35, seed ^ 0xc1a5_5ed0, 7)
            } else {
                1.0
            };
            let cl = 1.0 + (cl - 1.0) * clump;

            for c in 0..3 {
                let ch = if profile.mono { 0 } else { c as u32 + 1 };
                let n = lattice_noise(fx, fy, seed, ch);
                let g = (1.0 + n * sigma * cl * 0.35).max(0.05);
                buf[o + c] = (buf[o + c] * g).max(0.0);
            }
        }
    }

    if p.print_grain && p.print_grain_amount > 1.0e-5 {
        let cell_p = (5.0 * p.print_grain_size.max(0.25)).max(1.0);
        let seed_p = seed ^ 0x0005_0517;
        let amt = p.print_grain_amount * 0.25;
        for y in 0..h {
            for x in 0..w {
                let o = (y * w + x) * 4;
                let n = lattice_noise(x as f32 / cell_p, y as f32 / cell_p, seed_p, 3);
                let g = (1.0 + n * amt).max(0.2);
                buf[o] = (buf[o] * g).max(0.0);
                buf[o + 1] = (buf[o + 1] * g).max(0.0);
                buf[o + 2] = (buf[o + 2] * g).max(0.0);
            }
        }
    }
}

/// Low-frequency color mottle.
pub fn apply_mottle(buf: &mut [f32], w: usize, h: usize, p: &FilmParams) {
    if !p.mottle_enable || p.mottle_amount <= 1.0e-5 {
        return;
    }
    let seed = if p.mottle_static {
        p.seed ^ 0x0a07_71e5
    } else {
        p.seed.wrapping_add(p.frame.wrapping_mul(0x85eb_ca6b))
    };
    let cell = (w.min(h) as f32 / 20.0) * p.mottle_size.max(0.25);
    let amt = p.mottle_amount * 0.08;
    for y in 0..h {
        for x in 0..w {
            let o = (y * w + x) * 4;
            let nr = lattice_noise(x as f32 / cell, y as f32 / cell, seed, 1);
            let ng = lattice_noise(x as f32 / cell, y as f32 / cell, seed, 2);
            let nb = lattice_noise(x as f32 / cell, y as f32 / cell, seed, 3);
            buf[o] = (buf[o] * (1.0 + nr * amt)).max(0.0);
            buf[o + 1] = (buf[o + 1] * (1.0 + ng * amt)).max(0.0);
            buf[o + 2] = (buf[o + 2] * (1.0 + nb * amt)).max(0.0);
        }
    }
}
