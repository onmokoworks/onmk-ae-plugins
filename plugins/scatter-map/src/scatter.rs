/// Scatter effect: randomly displaces pixels with per-pixel map modulation.
///
/// Each output pixel samples from a random offset within the scatter amount.
/// Uses a deterministic hash (seed-based) so the pattern is stable per frame
/// but varies with the Randomness Seed parameter.
///
/// Pixel format: flat ARGB u8, 4 bytes per pixel.

use crate::ScatterParams;

// Direction constants (1-indexed popup)
const DIR_HORIZONTAL: i32 = 1;
const DIR_VERTICAL: i32 = 2;
const DIR_BOTH: i32 = 3;

/// Fast deterministic hash for pixel scatter offset.
/// Returns a value in [-1.0, 1.0].
#[inline]
fn hash_pixel(x: u32, y: u32, seed: u32, channel: u32) -> f32 {
    // Based on a simple integer hash (similar to PCG/xxHash style mixing)
    let mut h = x.wrapping_mul(374761393)
        .wrapping_add(y.wrapping_mul(668265263))
        .wrapping_add(seed.wrapping_mul(2246822519))
        .wrapping_add(channel.wrapping_mul(3266489917));
    h = h ^ (h >> 13);
    h = h.wrapping_mul(274177);
    h = h ^ (h >> 16);
    h = h.wrapping_mul(1900813);
    h = h ^ (h >> 13);
    // Map to [-1, 1]
    (h as f32 / (u32::MAX as f32)) * 2.0 - 1.0
}

pub fn scatter(
    sp: &ScatterParams,
    src: &[u8],
    w: usize, h: usize,
    luma_map: Option<&[f64]>,
) -> Vec<u8> {
    let npx = w * h;
    if sp.amount < 1 || (w == 0 || h == 0) {
        return src.to_vec();
    }

    let amount = sp.amount as f32;
    let seed = sp.random_seed;
    let do_h = sp.direction == DIR_HORIZONTAL || sp.direction == DIR_BOTH;
    let do_v = sp.direction == DIR_VERTICAL || sp.direction == DIR_BOTH;

    let mut out = vec![0u8; npx * 4];

    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;

            // Per-pixel amount modulation from map
            let px_amount = match luma_map {
                Some(map) => amount * map[idx] as f32,
                None => amount,
            };

            // Compute random offset
            let dx = if do_h {
                (hash_pixel(x as u32, y as u32, seed, 0) * px_amount).round() as i32
            } else {
                0
            };
            let dy = if do_v {
                (hash_pixel(x as u32, y as u32, seed, 1) * px_amount).round() as i32
            } else {
                0
            };

            // Source coordinate
            let raw_sx = x as i32 + dx;
            let raw_sy = y as i32 + dy;
            let dst_idx = idx * 4;

            let oob = raw_sx < 0 || raw_sx >= w as i32 || raw_sy < 0 || raw_sy >= h as i32;

            if oob && !sp.repeat_edge {
                // Out of bounds, no repeat: transparent black
                out[dst_idx]     = 0;
                out[dst_idx + 1] = 0;
                out[dst_idx + 2] = 0;
                out[dst_idx + 3] = 0;
            } else {
                // Clamp to edge (repeat edge pixels)
                let sx = raw_sx.clamp(0, w as i32 - 1) as usize;
                let sy = raw_sy.clamp(0, h as i32 - 1) as usize;
                let src_idx = (sy * w + sx) * 4;
                out[dst_idx]     = src[src_idx];
                out[dst_idx + 1] = src[src_idx + 1];
                out[dst_idx + 2] = src[src_idx + 2];
                out[dst_idx + 3] = src[src_idx + 3];
            }
        }
    }

    // Mix with original
    if sp.mix < 1.0 {
        let mix = sp.mix;
        let inv = 1.0 - mix;
        for i in 0..npx {
            let off = i * 4;
            for c in 0..4 {
                let o = src[off + c] as f32;
                let s = out[off + c] as f32;
                out[off + c] = (o * inv + s * mix).clamp(0.0, 255.0) as u8;
            }
        }
    }

    out
}

