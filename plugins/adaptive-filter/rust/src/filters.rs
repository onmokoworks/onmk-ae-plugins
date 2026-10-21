/// CPU filter implementations for AdaptiveFilter.
/// Filters retain the native channel type. Color-distance math uses the original
/// 0..255 scale so edge parameters keep the same meaning at every depth.
/// When a luminance map is provided, the effective radius is scaled per-pixel:
///   effective_radius = max(1, round(radius * luma))
/// Bright map areas → full filter strength, dark areas → minimal filtering.

/// Conversion uses f64 without reducing deep/float channels to eight bits.
pub trait Channel: Copy + Default {
    fn value(self) -> f64;
    fn from_value(v: f64) -> Self;
}
impl Channel for u8 {
    fn value(self) -> f64 { self as f64 }
    fn from_value(v: f64) -> Self { (v + 0.5).clamp(0.0, 255.0) as u8 }
}
impl Channel for u16 {
    fn value(self) -> f64 { self as f64 * (255.0 / 32768.0) }
    fn from_value(v: f64) -> Self { (v * (32768.0 / 255.0)).round().clamp(0.0, 32768.0) as u16 }
}
impl Channel for f32 {
    fn value(self) -> f64 { self as f64 * 255.0 }
    fn from_value(v: f64) -> Self { (v / 255.0) as f32 }
}

#[inline]
fn lerp<T: Channel>(a: T, b: T, t: f64) -> T {
    T::from_value(a.value() * (1.0 - t) + b.value() * t)
}

/// Read pixel ARGB from a flat buffer with clamped coordinates.
/// Returns (A, R, G, B).
#[inline]
fn read_pixel<T: Channel>(buf: &[T], w: usize, h: usize, x: i32, y: i32) -> (T, T, T, T) {
    let cx = x.clamp(0, w as i32 - 1) as usize;
    let cy = y.clamp(0, h as i32 - 1) as usize;
    let off = (cy * w + cx) * 4;
    (buf[off], buf[off + 1], buf[off + 2], buf[off + 3])
}

#[inline]
fn write_pixel<T: Channel>(buf: &mut [T], w: usize, x: usize, y: usize, a: T, r: T, g: T, b: T) {
    let off = (y * w + x) * 4;
    buf[off] = a;
    buf[off + 1] = r;
    buf[off + 2] = g;
    buf[off + 3] = b;
}

/// Compute the per-pixel effective radius from the luminance map.
#[inline]
fn effective_radius(base_radius: usize, luma_map: Option<&[f64]>, px_index: usize) -> usize {
    match luma_map {
        None => base_radius,
        Some(map) => {
            let luma = map.get(px_index).copied().unwrap_or(1.0);
            (base_radius as f64 * luma).round().max(1.0) as usize
        }
    }
}

// ---- 1. Kuwahara (4 quadrants) ----

pub fn filter_kuwahara<T: Channel>(
    src: &[T],
    dst: &mut [T],
    w: usize,
    h: usize,
    radius: usize,
    luma_map: Option<&[f64]>,
) {
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let px_idx = y as usize * w + x as usize;
            let er = effective_radius(radius, luma_map, px_idx) as i32;

            let quads: [(i32, i32, i32, i32); 4] = [
                (-er, 0, -er, 0),
                (0, er, -er, 0),
                (-er, 0, 0, er),
                (0, er, 0, er),
            ];

            let mut best_var = f64::MAX;
            let mut best_r = 0.0f64;
            let mut best_g = 0.0f64;
            let mut best_b = 0.0f64;

            for &(x0, x1, y0, y1) in &quads {
                let mut sum_r = 0.0f64;
                let mut sum_g = 0.0f64;
                let mut sum_b = 0.0f64;
                let mut sum_r2 = 0.0f64;
                let mut sum_g2 = 0.0f64;
                let mut sum_b2 = 0.0f64;
                let mut count = 0;

                for ky in y0..=y1 {
                    for kx in x0..=x1 {
                        let (_, pr, pg, pb) = read_pixel(src, w, h, x + kx, y + ky);
                        let (rf, gf, bf) = (pr.value(), pg.value(), pb.value());
                        sum_r += rf;
                        sum_g += gf;
                        sum_b += bf;
                        sum_r2 += rf * rf;
                        sum_g2 += gf * gf;
                        sum_b2 += bf * bf;
                        count += 1;
                    }
                }

                let inv_n = 1.0 / count as f64;
                let (mr, mg, mb) = (sum_r * inv_n, sum_g * inv_n, sum_b * inv_n);
                let total_var = (sum_r2 * inv_n - mr * mr)
                    + (sum_g2 * inv_n - mg * mg)
                    + (sum_b2 * inv_n - mb * mb);

                if total_var < best_var {
                    best_var = total_var;
                    best_r = mr;
                    best_g = mg;
                    best_b = mb;
                }
            }

            let (a, _, _, _) = read_pixel(src, w, h, x, y);
            write_pixel(
                dst,
                w,
                x as usize,
                y as usize,
                a,
                T::from_value(best_r),
                T::from_value(best_g),
                T::from_value(best_b),
            );
        }
    }
}

// ---- 2. Generalized Kuwahara (8 sectors, weighted blending) ----

pub fn filter_gen_kuwahara<T: Channel>(
    src: &[T],
    dst: &mut [T],
    w: usize,
    h: usize,
    radius: usize,
    edge_param: f64,
    luma_map: Option<&[f64]>,
) {
    const NS: usize = 8;
    let pi = std::f64::consts::PI;
    let sector_angle = 2.0 * pi / NS as f64;
    let sharpness = 1.0 + edge_param * 0.07;

    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let px_idx = y as usize * w + x as usize;
            let er = effective_radius(radius, luma_map, px_idx);
            let r = er as i32;
            let rf = er as f64;

            let mut s_r = [0.0f64; NS];
            let mut s_g = [0.0f64; NS];
            let mut s_b = [0.0f64; NS];
            let mut s_r2 = [0.0f64; NS];
            let mut s_g2 = [0.0f64; NS];
            let mut s_b2 = [0.0f64; NS];
            let mut s_w = [0.0f64; NS];

            for ky in -r..=r {
                for kx in -r..=r {
                    let dist = ((kx * kx + ky * ky) as f64).sqrt();
                    if dist > rf + 0.5 {
                        continue;
                    }
                    let angle = (ky as f64).atan2(kx as f64) + pi;
                    let sec = ((angle / sector_angle) as usize) % NS;
                    let sw = (-(dist * dist) / (2.0 * rf * rf * 0.25)).exp();

                    let (_, pr, pg, pb) = read_pixel(src, w, h, x + kx, y + ky);
                    let (rv, gv, bv) = (pr.value(), pg.value(), pb.value());
                    s_r[sec] += rv * sw;
                    s_g[sec] += gv * sw;
                    s_b[sec] += bv * sw;
                    s_r2[sec] += rv * rv * sw;
                    s_g2[sec] += gv * gv * sw;
                    s_b2[sec] += bv * bv * sw;
                    s_w[sec] += sw;
                }
            }

            let mut total_r = 0.0f64;
            let mut total_g = 0.0f64;
            let mut total_b = 0.0f64;
            let mut total_w = 0.0f64;

            for s in 0..NS {
                if s_w[s] < 1e-6 {
                    continue;
                }
                let inv = 1.0 / s_w[s];
                let (mr, mg, mb) = (s_r[s] * inv, s_g[s] * inv, s_b[s] * inv);
                let var = ((s_r2[s] * inv - mr * mr)
                    + (s_g2[s] * inv - mg * mg)
                    + (s_b2[s] * inv - mb * mb))
                    .max(0.0);
                let wt = (-(var * sharpness) / (256.0 * 256.0 * 0.01)).exp();
                total_r += mr * wt;
                total_g += mg * wt;
                total_b += mb * wt;
                total_w += wt;
            }

            if total_w < 1e-6 {
                total_w = 1.0;
            }
            let (a, _, _, _) = read_pixel(src, w, h, x, y);
            write_pixel(
                dst,
                w,
                x as usize,
                y as usize,
                a,
                T::from_value(total_r / total_w),
                T::from_value(total_g / total_w),
                T::from_value(total_b / total_w),
            );
        }
    }
}

// ---- 3. Bilateral Filter ----

pub fn filter_bilateral<T: Channel>(
    src: &[T],
    dst: &mut [T],
    w: usize,
    h: usize,
    radius: usize,
    edge_param: f64,
    luma_map: Option<&[f64]>,
) {
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let px_idx = y as usize * w + x as usize;
            let er = effective_radius(radius, luma_map, px_idx);
            let r = er as i32;

            let sigma_s = (er as f64 * 0.5).max(1.0);
            let sigma_r = 5.0 + edge_param * 2.5;
            let inv_ss2 = 1.0 / (2.0 * sigma_s * sigma_s);
            let inv_sr2 = 1.0 / (2.0 * sigma_r * sigma_r);

            let (ca, cr, cg, cb) = read_pixel(src, w, h, x, y);
            let (crf, cgf, cbf) = (cr.value(), cg.value(), cb.value());
            let mut sum_r = 0.0f64;
            let mut sum_g = 0.0f64;
            let mut sum_b = 0.0f64;
            let mut sum_w = 0.0f64;

            for ky in -r..=r {
                for kx in -r..=r {
                    let (_, pr, pg, pb) = read_pixel(src, w, h, x + kx, y + ky);
                    let dr = pr.value() - crf;
                    let dg = pg.value() - cgf;
                    let db = pb.value() - cbf;
                    let wt = (-(kx * kx + ky * ky) as f64 * inv_ss2
                        - (dr * dr + dg * dg + db * db) * inv_sr2)
                        .exp();
                    sum_r += pr.value() * wt;
                    sum_g += pg.value() * wt;
                    sum_b += pb.value() * wt;
                    sum_w += wt;
                }
            }

            if sum_w < 1e-6 {
                sum_w = 1.0;
            }
            write_pixel(
                dst,
                w,
                x as usize,
                y as usize,
                ca,
                T::from_value(sum_r / sum_w),
                T::from_value(sum_g / sum_w),
                T::from_value(sum_b / sum_w),
            );
        }
    }
}

// ---- Dispatch ----

pub fn dispatch_filter<T: Channel>(
    filter_type: i32,
    src: &[T],
    dst: &mut [T],
    w: usize,
    h: usize,
    radius: usize,
    edge_p: f64,
    luma_map: Option<&[f64]>,
) {
    match filter_type {
        1 => filter_kuwahara(src, dst, w, h, radius, luma_map),
        2 => filter_gen_kuwahara(src, dst, w, h, radius, edge_p, luma_map),
        3 => filter_bilateral(src, dst, w, h, radius, edge_p, luma_map),
        _ => filter_kuwahara(src, dst, w, h, radius, luma_map),
    }
}

// ---- Mix ----

pub fn mix_buffers<T: Channel>(original: &[T], filtered: &[T], output: &mut [T], mix: f64) {
    if mix >= 1.0 - 1e-6 {
        output.copy_from_slice(filtered);
    } else {
        for i in (0..original.len()).step_by(4) {
            output[i] = original[i]; // alpha
            output[i + 1] = lerp(original[i + 1], filtered[i + 1], mix);
            output[i + 2] = lerp(original[i + 2], filtered[i + 2], mix);
            output[i + 3] = lerp(original[i + 3], filtered[i + 3], mix);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_sixteen_bit_codes_survive_conversion() {
        for value in 0u16..=32768 {
            assert_eq!(u16::from_value(value.value()), value);
        }
    }

    #[test]
    fn deep_filters_preserve_constant_sub_byte_detail() {
        for mode in 1..=3 {
            let source = [32768u16, 16385, 16387, 1];
            let mut output = [0u16; 4];
            dispatch_filter(mode, &source, &mut output, 1, 1, 2, 50.0, None);
            assert_eq!(output, source, "16-bit mode {mode}");
            let source = [0.75f32, -0.125, 2.0, 0.000030517578];
            let mut output = [0f32; 4];
            dispatch_filter(mode, &source, &mut output, 1, 1, 2, 50.0, None);
            for (actual, expected) in output.iter().zip(source) {
                assert!((actual - expected).abs() < 1e-7, "float mode {mode}");
            }
        }
    }

    #[test]
    fn deep_bilateral_produces_non_byte_aligned_output() {
        let source = [32768u16,16385,16385,16385,32768,16387,16387,16387];
        let mut output = [0u16; 8];
        dispatch_filter(3, &source, &mut output, 2, 1, 1, 100.0, None);
        // The spatial weights average both levels to 16386 after native
        // rounding; neither an 8-bit conversion nor a passthrough can pass.
        assert_eq!(output[1], 16386);
        assert_eq!(output[5], 16386);
        assert_eq!(output[0], 32768);
    }
}
