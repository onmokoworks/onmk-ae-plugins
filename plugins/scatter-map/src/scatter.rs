/// Scatter effect: randomly displaces pixels with per-pixel map modulation.
///
/// Each output pixel samples from a random offset within the scatter amount.
/// Uses a deterministic hash (seed-based) so the pattern is stable per frame
/// but varies with the Randomness Seed parameter.
///
/// Pixel format: flat native-depth ARGB channels.

use crate::ScatterParams;
use crate::pixels::Channel;

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

pub fn scatter<T: Channel>(
    sp: &ScatterParams,
    src: &[T],
    w: usize, h: usize,
    luma_map: Option<&[f64]>,
) -> Result<Vec<T>,after_effects::Error> {
    use after_effects::Error;
    let npx=w.checked_mul(h).ok_or(Error::OutOfMemory)?;
    let n=npx.checked_mul(4).ok_or(Error::OutOfMemory)?;
    if src.len()!=n || w>i32::MAX as usize || h>i32::MAX as usize
        || luma_map.is_some_and(|map|map.len()!=npx) {return Err(Error::BadCallbackParameter);}
    let mut out=Vec::new();
    out.try_reserve_exact(n).map_err(|_|Error::OutOfMemory)?;
    if sp.amount<1 || npx==0 {out.extend_from_slice(src);return Ok(out);}
    out.resize(n,T::default());

    let amount = sp.amount as f32;
    let seed = sp.random_seed;
    let do_h = sp.direction == DIR_HORIZONTAL || sp.direction == DIR_BOTH;
    let do_v = sp.direction == DIR_VERTICAL || sp.direction == DIR_BOTH;


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
            let raw_sx = x as i64 + dx as i64;
            let raw_sy = y as i64 + dy as i64;
            let dst_idx = idx * 4;

            let oob = raw_sx < 0 || raw_sx >= w as i64 || raw_sy < 0 || raw_sy >= h as i64;

            if oob && !sp.repeat_edge {
                // Out of bounds, no repeat: transparent black
                out[dst_idx..dst_idx + 4].fill(T::default());
            } else {
                // Clamp to edge (repeat edge pixels)
                let sx = raw_sx.clamp(0, w as i64 - 1) as usize;
                let sy = raw_sy.clamp(0, h as i64 - 1) as usize;
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
                out[off + c] = T::blend(src[off + c], out[off + c], inv, mix);
            }
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn params() -> ScatterParams {
        ScatterParams { amount: 5, direction: 3, random_seed: 9, repeat_edge: true, mix: 1.0, invert_map: false }
    }
    #[test]
    fn scatter_preserves_native_precision_and_sampling() {
        let bytes: Vec<u8> = (0..64).collect();
        let words: Vec<u16> = bytes.iter().map(|v| *v as u16 * 257 + 1).collect();
        let floats: Vec<f32> = bytes.iter().map(|v| *v as f32 * 0.12345 - 2.0).collect();
        let p = params();
        let a = scatter(&p, &bytes, 4, 4, None).unwrap();
        let b = scatter(&p, &words, 4, 4, None).unwrap();
        let c = scatter(&p, &floats, 4, 4, None).unwrap();
        for i in 0..a.len() {
            assert_eq!(b[i], words[a[i] as usize]);
            assert_eq!(c[i].to_bits(), floats[a[i] as usize].to_bits());
        }
    }
    #[test]
    fn native16_scatter_mix_matches_integer_reference() {
        let (w, h) = (17, 31);
        let src: Vec<u16> = (0..w * h).flat_map(|i| {
            let alpha = ((i * 67 + 1) % 32769) as u16;
            [alpha, alpha / 2, alpha.saturating_sub(1), alpha / 3]
        }).collect();
        // Unique nonzero labels expose the selected source pixel; zero marks
        // transparent out-of-bounds samples. Sampling is tested separately.
        let labels: Vec<u16> = (1..=w * h).flat_map(|i| [i as u16; 4]).collect();
        let map: Vec<f64> = (0..w * h).map(|i| (i % 5) as f64 / 4.0).collect();
        for direction in 1..=3 {
            for repeat_edge in [false, true] {
                let mut p = params();
                p.direction = direction;
                p.repeat_edge = repeat_edge;
                let selected = scatter(&p, &labels, w, h, Some(&map)).unwrap();
                for numerator in 0..=4u32 {
                    p.mix = numerator as f32 / 4.0;
                    let actual = scatter(&p, &src, w, h, Some(&map)).unwrap();
                    for i in 0..actual.len() {
                        let label = selected[i] as usize;
                        let sample = if label == 0 { 0 } else { src[(label - 1) * 4 + i % 4] };
                        let expected = (src[i] as u32 * (4 - numerator)
                            + sample as u32 * numerator) / 4;
                        assert_eq!(actual[i], expected as u16,
                            "direction={direction}, repeat={repeat_edge}, mix={numerator}/4, channel={i}");
                    }
                }
            }
        }
    }
    #[test]
    fn zero_map_is_exact_identity_at_native_depth() {
        let p = params();
        let words = vec![32768u16, 12345, 23456, 1];
        assert_eq!(scatter(&p, &words, 1, 1, Some(&[0.0])).unwrap(), words);
        let floats = vec![1.0f32, -0.25, 5.75, 0.1234567];
        assert_eq!(scatter(&p, &floats, 1, 1, Some(&[0.0])).unwrap(), floats);
    }
    #[test]
    fn core_rejects_bad_geometry_and_handles_extreme_offsets() {
        let mut p=params();
        assert!(scatter::<u8>(&p,&[],usize::MAX,2,None).is_err());
        assert!(scatter::<u8>(&p,&[0;3],1,1,None).is_err());
        assert!(scatter::<u8>(&p,&[0;4],1,1,Some(&[])).is_err());
        assert_eq!(scatter::<u8>(&p,&[],0,3,None).unwrap(),Vec::<u8>::new());
        p.amount=usize::MAX;
        let input=[255u8,10,20,30].repeat(6);
        assert_eq!(scatter(&p,&input,3,2,None).unwrap(),input);
        p.repeat_edge=false;
        assert_eq!(scatter(&p,&input,3,2,None).unwrap(),vec![0u8;24]);
        p.amount=0;
        assert!(scatter::<u8>(&p,&[0;3],1,1,None).is_err());
    }
}
