//! High-performance gradient-based chromatic distortion.
//!
//! Pixel format: flat f32, 4 values per pixel, ordered [alpha, red, green, blue]
//! to match After Effects' `PF_Pixel` layout. Channel values are nominally 0.0..1.0;
//! 32bpc input may exceed that range and is carried through unclamped. Conversion
//! to and from the host's 8/16/32bpc worlds happens at the plug-in boundary.
//!
//! Optimizations:
//! - Box blur (3-pass) instead of Gaussian: O(1) per pixel regardless of radius
//! - Gradient computed in-place during the render loop (no gx/gy buffers)
//! - Pre-computed spectrum colors and warp magnitudes
//! - Output written row by row through a callback, so no full-size output buffer
//!
//! Every heap allocation goes through `try_vec_f32`, which reports failure as
//! `Error::OutOfMemory` instead of aborting the host process.

use crate::DistortParams;
use after_effects as ae;

/// Allocate a zeroed `f32` buffer, returning `Err` instead of aborting on failure.
///
/// `vec![0.0; n]` calls `handle_alloc_error` on failure, which aborts the process and
/// is not catchable by `catch_unwind`. In After Effects that takes the whole app down.
pub fn try_vec_f32(n: usize) -> Result<Vec<f32>, ae::Error> {
    let mut v: Vec<f32> = Vec::new();
    v.try_reserve_exact(n)?;
    v.resize(n, 0.0);
    Ok(v)
}

/// `w * h`, or `None` if the pixel count (or its 4-channel byte span) would overflow.
pub fn checked_pixel_count(w: usize, h: usize) -> Option<usize> {
    let npx = w.checked_mul(h)?;
    npx.checked_mul(4)?;
    Some(npx)
}

/// Flat f32 RGBA image, 4 values per pixel in [alpha, red, green, blue] order.
pub struct ImageF32 {
    pub data: Vec<f32>,
    pub w: usize,
    pub h: usize,
}

impl ImageF32 {
    pub fn new_zeroed(w: usize, h: usize) -> Result<Self, ae::Error> {
        let npx = checked_pixel_count(w, h).ok_or(ae::Error::OutOfMemory)?;
        Ok(Self {
            data: try_vec_f32(npx * 4)?,
            w,
            h,
        })
    }

    /// True when the dimensions are non-degenerate and `data` actually holds them.
    pub fn is_valid(&self) -> bool {
        match checked_pixel_count(self.w, self.h) {
            Some(npx) => npx > 0 && self.data.len() >= npx * 4,
            None => false,
        }
    }

    /// Nearest-neighbour resample. Used when a lens/matte layer differs in size
    /// from the layer being rendered.
    pub fn resampled_to(&self, dw: usize, dh: usize) -> Result<Self, ae::Error> {
        let mut dst = Self::new_zeroed(dw, dh)?;
        if !self.is_valid() || dw == 0 || dh == 0 {
            // Degenerate source: a fully transparent destination is the best we can do.
            return Ok(dst);
        }
        for y in 0..dh {
            let sy = (y * self.h / dh).min(self.h - 1);
            for x in 0..dw {
                let sx = (x * self.w / dw).min(self.w - 1);
                let si = (sy * self.w + sx) * 4;
                let di = (y * dw + x) * 4;
                dst.data[di..di + 4].copy_from_slice(&self.data[si..si + 4]);
            }
        }
        Ok(dst)
    }
}

// ---- Fast box blur (3-pass approximation of Gaussian) ----
// Each pass is O(1) per pixel using running sums.

/// Consumes `buf_a` so the caller's brightness buffer is reused as the first
/// ping-pong buffer instead of being copied.
fn box_blur_3pass(
    mut buf_a: Vec<f32>,
    w: usize,
    h: usize,
    sigma: f64,
) -> Result<Vec<f32>, ae::Error> {
    if sigma < 0.5 {
        return Ok(buf_a);
    }
    let npx = match checked_pixel_count(w, h) {
        Some(n) if n > 0 && buf_a.len() >= n => n,
        _ => return Ok(buf_a),
    };

    let boxes = boxes_for_gauss(sigma);
    let mut buf_b = try_vec_f32(npx)?;
    // One scratch buffer for all three passes rather than one per pass.
    let mut tmp = try_vec_f32(npx)?;

    for &box_r in &boxes {
        box_blur_pass(&buf_a, &mut buf_b, &mut tmp, w, h, box_r);
        std::mem::swap(&mut buf_a, &mut buf_b);
    }
    Ok(buf_a)
}

/// Compute 3 box radii that approximate a Gaussian with given sigma.
fn boxes_for_gauss(sigma: f64) -> [usize; 3] {
    let n = 3.0f64;
    let w_ideal = ((12.0 * sigma * sigma / n) + 1.0).sqrt();
    // For sigma >= 0.5 this is always >= 1, but clamp anyway so the `-= 1` below
    // cannot underflow if the guard above ever changes.
    let mut wl = (w_ideal.floor() as usize).max(1);
    if wl.is_multiple_of(2) {
        wl -= 1;
    }
    let wl = wl.max(1);
    let wu = wl + 2;
    let m =
        ((12.0 * sigma * sigma - (n * wl as f64 * wl as f64) - (4.0 * n * wl as f64) - (3.0 * n))
            / (-4.0 * wl as f64 - 4.0))
            .round() as usize;

    let mut sizes = [0usize; 3];
    for (i, s) in sizes.iter_mut().enumerate() {
        // Convert from box width to radius as we go.
        *s = if i < m { wl } else { wu }.max(1) / 2;
    }
    sizes
}

/// Running sum over `[-r, r]` around index 0 of a row/column of length `len`,
/// with out-of-range taps clamped to the nearest edge sample.
///
/// Taps at negative indices contribute `r * first`. Taps at `0..=r` contribute the
/// samples that exist plus, when `r` reaches past the end, `(r - (len-1))` extra
/// copies of the last sample. That trailing term is what makes the blur correct
/// when the radius is larger than the layer.
#[inline]
fn initial_running_sum(len: usize, r: usize, mut sample: impl FnMut(usize) -> f32) -> f32 {
    let last = len - 1;
    let mut running = r as f32 * sample(0);
    for i in 0..=r.min(last) {
        running += sample(i);
    }
    if r > last {
        running += (r - last) as f32 * sample(last);
    }
    running
}

/// Single box blur pass (separable: horizontal into `tmp`, then vertical into `dst`).
fn box_blur_pass(src: &[f32], dst: &mut [f32], tmp: &mut [f32], w: usize, h: usize, r: usize) {
    let npx = match checked_pixel_count(w, h) {
        Some(n) => n,
        None => return,
    };
    // Guard every buffer up front: `w - 1` / `h - 1` below would wrap to usize::MAX
    // on a zero-sized layer, and release builds have overflow checks disabled.
    if npx == 0 || src.len() < npx || dst.len() < npx || tmp.len() < npx {
        return;
    }
    if r == 0 {
        dst[..npx].copy_from_slice(&src[..npx]);
        return;
    }

    let diam = (2 * r + 1) as f32;
    let inv = 1.0 / diam;

    // Horizontal pass
    for y in 0..h {
        let row = y * w;
        let mut running = initial_running_sum(w, r, |i| src[row + i]);

        for x in 0..w {
            tmp[row + x] = running * inv;
            // Add the tap entering the window, remove the one leaving it.
            let add_x = (x + r + 1).min(w - 1);
            let rem_x = x.saturating_sub(r);
            running += src[row + add_x] - src[row + rem_x];
        }
    }

    // Vertical pass
    for x in 0..w {
        let mut running = initial_running_sum(h, r, |i| tmp[i * w + x]);

        for y in 0..h {
            dst[y * w + x] = running * inv;
            let add_y = (y + r + 1).min(h - 1);
            let rem_y = y.saturating_sub(r);
            running += tmp[add_y * w + x] - tmp[rem_y * w + x];
        }
    }
}

// ---- Bilinear sampling with reflect boundary ----

#[inline]
fn reflect_coord(v: f32, max: f32) -> f32 {
    if max <= 1.0 {
        return 0.0;
    }
    let limit = max - 1.0;
    let mut v = v;
    if v < 0.0 {
        v = -v;
    }
    if v > limit {
        let period = 2.0 * limit;
        v %= period;
        if v > limit {
            v = period - v;
        }
    }
    // NaN would slip through the comparisons above; clamp maps it to 0.0.
    if v.is_nan() {
        return 0.0;
    }
    v.clamp(0.0, limit)
}

#[inline]
fn sample_bilinear_rgb(buf: &[f32], w: usize, h: usize, fx: f32, fy: f32) -> (f32, f32, f32) {
    debug_assert!(w > 0 && h > 0);
    let fx = reflect_coord(fx, w as f32);
    let fy = reflect_coord(fy, h as f32);

    let x0 = (fx as usize).min(w.saturating_sub(1));
    let y0 = (fy as usize).min(h.saturating_sub(1));
    let x1 = (x0 + 1).min(w.saturating_sub(1));
    let y1 = (y0 + 1).min(h.saturating_sub(1));
    let dx = fx - x0 as f32;
    let dy = fy - y0 as f32;
    let idx = 1.0 - dx;
    let idy = 1.0 - dy;

    let w00 = idx * idy;
    let w10 = dx * idy;
    let w01 = idx * dy;
    let w11 = dx * dy;

    let o00 = (y0 * w + x0) * 4;
    let o10 = (y0 * w + x1) * 4;
    let o01 = (y1 * w + x0) * 4;
    let o11 = (y1 * w + x1) * 4;

    (
        buf[o00 + 1] * w00 + buf[o10 + 1] * w10 + buf[o01 + 1] * w01 + buf[o11 + 1] * w11,
        buf[o00 + 2] * w00 + buf[o10 + 2] * w10 + buf[o01 + 2] * w01 + buf[o11 + 2] * w11,
        buf[o00 + 3] * w00 + buf[o10 + 3] * w10 + buf[o01 + 3] * w01 + buf[o11 + 3] * w11,
    )
}

/// Brightness is kept on a 0..255 scale even though colours are 0..1, so that the
/// Threshold parameters and the displacement produced by the gradient keep the same
/// magnitude they had when the whole pipeline was 8bpc.
const BRIGHTNESS_SCALE: f32 = 255.0;
const MAX_GRADIENT_MAGNITUDE: f32 = BRIGHTNESS_SCALE / std::f32::consts::SQRT_2;

#[inline]
fn spectral_displacement(strength: f32, dispersion: f32, t: f32) -> f32 {
    strength + dispersion * (t * 2.0 - 1.0)
}

#[inline]
fn normalized_gradient(gdx: f32, gdy: f32, scale: f32) -> (f32, f32) {
    let mag = (gdx * gdx + gdy * gdy).sqrt();
    if mag > 1.0e-6 {
        (gdx * scale / mag, gdy * scale / mag)
    } else {
        (0.0, 0.0)
    }
}

/// Main PrismWarp function.
///
/// `write_row` receives each output row as `w * 4` floats in [alpha, r, g, b] order,
/// top row first. Writing row by row avoids a second full-frame buffer.
pub fn prism_warp(
    dp: &DistortParams,
    src: &ImageF32,
    lens: Option<&ImageF32>,
    matte: Option<&ImageF32>,
    mut write_row: impl FnMut(usize, &[f32]),
) -> Result<(), ae::Error> {
    let (w, h) = (src.w, src.h);

    // Degenerate input: emit transparent rows rather than reading out of bounds.
    if !src.is_valid() {
        let row_len = w.checked_mul(4).ok_or(ae::Error::OutOfMemory)?;
        let zero = try_vec_f32(row_len)?;
        for y in 0..h {
            write_row(y, &zero);
        }
        return Ok(());
    }
    let npx = checked_pixel_count(w, h).ok_or(ae::Error::OutOfMemory)?;

    // Only accept auxiliary layers that match the render size; the caller resamples,
    // but a mismatch here would index out of bounds.
    let lens = lens.filter(|l| l.is_valid() && l.w == w && l.h == h);
    let matte = matte.filter(|m| m.is_valid() && m.w == w && m.h == h);

    // 1. Extract lens brightness
    let lens_src = lens.unwrap_or(src);
    let mut lens_bright = try_vec_f32(npx)?;
    for (dst, px) in lens_bright.iter_mut().zip(lens_src.data.chunks_exact(4)) {
        *dst = (0.2126 * px[1] + 0.7152 * px[2] + 0.0722 * px[3]) * BRIGHTNESS_SCALE;
    }

    // 2. Box blur (3-pass Gaussian approximation). Consumes lens_bright.
    let blurred = box_blur_3pass(lens_bright, w, h, dp.blur_lens)?;

    // 3. Threshold thresholds, in the same 0..255 space as the gradient magnitude
    let thresh_lo = dp.threshold as f32 * MAX_GRADIENT_MAGNITUDE;
    let smooth_range = dp.threshold_smooth as f32 * MAX_GRADIENT_MAGNITUDE;
    let thresh_hi = thresh_lo + smooth_range.max(0.01); // avoid div by zero

    // 4. Pre-compute rotation
    let rot_rad = (dp.rotate_warp_dir as f32).to_radians();
    let do_rotate = rot_rad.abs() > 1e-6;
    let cos_r = rot_rad.cos();
    let sin_r = rot_rad.sin();

    // 5. Pre-compute matte
    let matte_map: Option<Vec<f32>> = match matte {
        Some(m) => {
            let mut map = try_vec_f32(npx)?;
            for (dst, px) in map.iter_mut().zip(m.data.chunks_exact(4)) {
                let luma = 0.2126 * px[1] + 0.7152 * px[2] + 0.0722 * px[3];
                *dst = if dp.invert_matte { 1.0 - luma } else { luma };
            }
            Some(map)
        }
        None => None,
    };

    // 6. Pre-compute spectrum colors and pixel displacement.
    let steps = dp.steps.max(1);
    let mut spec_r = try_vec_f32(steps)?;
    let mut spec_g = try_vec_f32(steps)?;
    let mut spec_b = try_vec_f32(steps)?;
    let mut warp_mags = try_vec_f32(steps)?;

    let mut sum_sr = 0.0f32;
    let mut sum_sg = 0.0f32;
    let mut sum_sb = 0.0f32;

    for step in 0..steps {
        let t = if steps > 1 {
            step as f32 / (steps - 1) as f32
        } else {
            0.5
        };

        // Spectrum: R→G→B
        let (cr, cg, cb) = if t <= 0.5 {
            let s = t * 2.0;
            (1.0 - s, s, 0.0f32)
        } else {
            let s = (t - 0.5) * 2.0;
            (0.0f32, 1.0 - s, s)
        };
        spec_r[step] = cr;
        spec_g[step] = cg;
        spec_b[step] = cb;
        sum_sr += cr;
        sum_sg += cg;
        sum_sb += cb;

        // Strength is the shared refraction offset. Dispersion opens the
        // spectrum symmetrically around it, from red (-) to blue (+).
        warp_mags[step] = spectral_displacement(dp.strength as f32, dp.dispersion as f32, t);
    }

    // White balance normalization
    let inv_sr = if sum_sr > 1e-6 { 1.0 / sum_sr } else { 1.0 };
    let inv_sg = if sum_sg > 1e-6 { 1.0 / sum_sg } else { 1.0 };
    let inv_sb = if sum_sb > 1e-6 { 1.0 / sum_sb } else { 1.0 };

    for step in 0..steps {
        spec_r[step] *= inv_sr;
        spec_g[step] *= inv_sg;
        spec_b[step] *= inv_sb;
    }

    let mix = dp.mix as f32;
    let inv_mix = 1.0 - mix;

    // 7. Render, one row at a time
    let mut row_buf = try_vec_f32(w * 4)?;

    for y in 0..h {
        let row_off = y * w;
        let y0 = y.saturating_sub(1);
        let y1 = (y + 1).min(h - 1);

        for x in 0..w {
            let idx = row_off + x;
            let off = idx * 4;
            let dst = x * 4;

            let alpha = src.data[off];

            // Central-difference gradient of the blurred lens, with a smoothstep
            // threshold. Computed here rather than buffered into gx/gy arrays.
            let x0 = x.saturating_sub(1);
            let x1 = (x + 1).min(w - 1);
            let gdx = (blurred[row_off + x1] - blurred[row_off + x0]) * 0.5;
            let gdy = (blurred[y1 * w + x] - blurred[y0 * w + x]) * 0.5;
            let mag = (gdx * gdx + gdy * gdy).sqrt();
            let t = ((mag - thresh_lo) / (thresh_hi - thresh_lo)).clamp(0.0, 1.0);
            let scale = t * t * (3.0 - 2.0 * t); // smoothstep

            // Normalize the gradient before applying pixel displacement. This
            // keeps Strength stable across lens images with different contrast.
            let (mut dx, mut dy) = normalized_gradient(gdx, gdy, scale);

            if do_rotate {
                let rdx = dx * cos_r - dy * sin_r;
                let rdy = dx * sin_r + dy * cos_r;
                dx = rdx;
                dy = rdy;
            }

            // Matte modulation
            let matte_strength = match &matte_map {
                Some(m) => m[idx],
                None => 1.0,
            };

            // Skip pixels with zero displacement
            if dx.abs() < 1e-6 && dy.abs() < 1e-6 {
                row_buf[dst] = alpha;
                row_buf[dst + 1] = src.data[off + 1];
                row_buf[dst + 2] = src.data[off + 2];
                row_buf[dst + 3] = src.data[off + 3];
                continue;
            }

            let mut acc_r = 0.0f32;
            let mut acc_g = 0.0f32;
            let mut acc_b = 0.0f32;

            for step in 0..steps {
                let disp = warp_mags[step] * matte_strength;
                let sx = x as f32 + dx * disp;
                let sy = y as f32 + dy * disp;

                let (sr, sg, sb) = sample_bilinear_rgb(&src.data, w, h, sx, sy);

                acc_r += sr * spec_r[step];
                acc_g += sg * spec_g[step];
                acc_b += sb * spec_b[step];
            }

            // Mix with original. Left unclamped: 32bpc worlds legitimately carry
            // values outside 0..1, and the integer paths clamp on write-out.
            row_buf[dst] = alpha;
            row_buf[dst + 1] = src.data[off + 1] * inv_mix + acc_r * mix;
            row_buf[dst + 2] = src.data[off + 2] * inv_mix + acc_g * mix;
            row_buf[dst + 3] = src.data[off + 3] * inv_mix + acc_b * mix;
        }

        write_row(y, &row_buf);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> DistortParams {
        DistortParams {
            strength: 10.0,
            dispersion: 5.0,
            blur_lens: 4.0,
            threshold: 0.0,
            threshold_smooth: 0.05,
            rotate_warp_dir: 0.0,
            steps: 8,
            invert_matte: false,
            mix: 1.0,
        }
    }

    fn solid(w: usize, h: usize, v: f32) -> ImageF32 {
        ImageF32 {
            data: vec![v; w * h * 4],
            w,
            h,
        }
    }

    /// A box blur is an average, so a constant image must survive it unchanged --
    /// including when the radius reaches past both edges. The previous running-sum
    /// initialization dropped the right-edge clamp taps, which darkened the result
    /// whenever the radius exceeded the layer size.
    #[test]
    fn box_blur_preserves_constant_when_radius_exceeds_width() {
        for (w, h) in [(1usize, 1usize), (3, 3), (4, 2), (2, 7)] {
            for r in 0..12usize {
                let src = vec![0.75f32; w * h];
                let mut dst = vec![0.0f32; w * h];
                let mut tmp = vec![0.0f32; w * h];
                box_blur_pass(&src, &mut dst, &mut tmp, w, h, r);
                for (i, v) in dst.iter().enumerate() {
                    assert!(
                        (v - 0.75).abs() < 1e-4,
                        "w={w} h={h} r={r} idx={i}: got {v}, want 0.75"
                    );
                }
            }
        }
    }

    /// The running sum must count exactly 2r+1 taps, with out-of-range ones clamped
    /// to the nearest edge.
    #[test]
    fn initial_running_sum_counts_every_tap() {
        for len in 1..6usize {
            for r in 0..8usize {
                let data: Vec<f32> = (0..len).map(|i| (i + 1) as f32).collect();
                let got = initial_running_sum(len, r, |i| data[i]);
                let want: f32 = (-(r as isize)..=(r as isize))
                    .map(|i| data[i.clamp(0, len as isize - 1) as usize])
                    .sum();
                assert!(
                    (got - want).abs() < 1e-3,
                    "len={len} r={r}: got {got}, want {want}"
                );
            }
        }
    }

    /// Zero-sized inputs must not underflow into a usize::MAX index. Release builds
    /// have overflow checks off, so this would surface as an out-of-bounds panic far
    /// from its cause.
    #[test]
    fn degenerate_sizes_do_not_panic() {
        for (w, h) in [(0usize, 0usize), (0, 4), (4, 0)] {
            let mut dst = Vec::new();
            let mut tmp = Vec::new();
            box_blur_pass(&[], &mut dst, &mut tmp, w, h, 3);

            let src = ImageF32 {
                data: Vec::new(),
                w,
                h,
            };
            let mut rows = 0;
            prism_warp(&params(), &src, None, None, |_, _| rows += 1).unwrap();
            assert_eq!(rows, h, "expected one transparent row per line for {w}x{h}");

            assert!(solid(2, 2, 0.5).resampled_to(w, h).is_ok());
            assert!(src.resampled_to(3, 3).is_ok());
        }
    }

    /// A flat lens produces no gradient, so every pixel should come through untouched.
    #[test]
    fn flat_input_passes_through() {
        let src = solid(8, 6, 0.4);
        let mut out = Vec::new();
        prism_warp(&params(), &src, None, None, |_, row| {
            out.extend_from_slice(row)
        })
        .unwrap();
        assert_eq!(out.len(), 8 * 6 * 4);
        for v in &out {
            assert!((v - 0.4).abs() < 1e-4, "got {v}, want 0.4");
        }
    }

    /// 32bpc worlds carry values outside 0..1; the pipeline must not clamp them away.
    #[test]
    fn hdr_values_survive() {
        let src = solid(4, 4, 3.5);
        let mut out = Vec::new();
        prism_warp(&params(), &src, None, None, |_, row| {
            out.extend_from_slice(row)
        })
        .unwrap();
        for v in &out {
            assert!((v - 3.5).abs() < 1e-3, "got {v}, want 3.5");
        }
    }

    /// A mismatched lens or matte must be rejected rather than indexed out of bounds.
    #[test]
    fn mismatched_aux_layers_are_ignored() {
        let src = solid(8, 6, 0.4);
        let small = solid(2, 2, 1.0);
        let mut rows = 0;
        prism_warp(&params(), &src, Some(&small), Some(&small), |_, _| {
            rows += 1
        })
        .unwrap();
        assert_eq!(rows, 6);
    }

    #[test]
    fn checked_pixel_count_rejects_overflow() {
        assert_eq!(checked_pixel_count(4, 5), Some(20));
        assert_eq!(checked_pixel_count(usize::MAX, 2), None);
        // w*h fits but w*h*4 does not.
        assert_eq!(checked_pixel_count(usize::MAX / 2, 1), None);
    }

    #[test]
    fn dispersion_opens_symmetrically_around_strength() {
        assert_eq!(spectral_displacement(10.0, 4.0, 0.0), 6.0);
        assert_eq!(spectral_displacement(10.0, 4.0, 0.5), 10.0);
        assert_eq!(spectral_displacement(10.0, 4.0, 1.0), 14.0);
    }

    #[test]
    fn normalized_gradient_is_contrast_independent() {
        let low = normalized_gradient(3.0, 4.0, 1.0);
        let high = normalized_gradient(30.0, 40.0, 1.0);
        assert!((low.0 - 0.6).abs() < 1.0e-6);
        assert!((low.1 - 0.8).abs() < 1.0e-6);
        assert!((low.0 - high.0).abs() < 1.0e-6);
        assert!((low.1 - high.1).abs() < 1.0e-6);
    }
}
