//! Full film look pipeline (CPU reference).

use crate::blur::{acutance_sharpen, add_tinted, extract_highlights, gaussian_blur_rgba, mix_add};
use crate::color::{
    apply_crossover, apply_skin, decode_input, encode_output, film_response, luma,
    saturate_around_luma, stock_profile,
};
use crate::grain::{apply_grain, apply_mottle, hash_f32};
use crate::params::FilmParams;

/// Render `src` RGBA f32 (display-encoded per input CS) into `dst`.
pub fn render_cpu(src: &[f32], dst: &mut [f32], w: usize, h: usize, p: &FilmParams) {
    render_with_backends(src, dst, w, h, p, gaussian_blur_rgba, apply_grain)
}

/// Render with an injected blur backend. GPU hosts use this entry point and
/// can fall back per pass without duplicating the film pipeline.
pub fn render_with_blur<F>(
    src: &[f32],
    dst: &mut [f32],
    w: usize,
    h: usize,
    p: &FilmParams,
    blur: F,
) where
    F: FnMut(&[f32], &mut [f32], usize, usize, f32, f32),
{
    render_with_backends(src, dst, w, h, p, blur, apply_grain)
}

pub fn render_with_backends<F, G>(
    src: &[f32],
    dst: &mut [f32],
    w: usize,
    h: usize,
    p: &FilmParams,
    mut blur: F,
    mut grain: G,
) where
    F: FnMut(&[f32], &mut [f32], usize, usize, f32, f32),
    G: FnMut(&mut [f32], usize, usize, &FilmParams),
{
    assert_eq!(src.len(), w * h * 4);
    assert_eq!(dst.len(), w * h * 4);
    if w == 0 || h == 0 {
        return;
    }
    if p.bypass {
        dst.copy_from_slice(src);
        return;
    }

    // 1–3) Decode, exposure, and negative response (per pixel).
    let exp_mul = 2.0f32.powf(p.exposure);
    let print_c = p.print_contrast.max(0.05);
    let shoulder = p.shoulder.clamp(0.0, 1.0);
    let mut work = vec![0.0f32; w * h * 4];

    // Optional gate weave sample offset (integer for CPU simplicity).
    let (wdx, wdy) = if p.weave_enable && p.weave_amount > 1.0e-5 {
        let s = p.seed.wrapping_add(p.frame.wrapping_mul(0x27d4_eb2d));
        let amp = p.weave_amount * 1.5;
        (
            (hash_f32(s) * amp).round() as i32,
            (hash_f32(s ^ 0x9e37_79b9) * amp * 0.6).round() as i32,
        )
    } else {
        (0, 0)
    };

    for y in 0..h {
        for x in 0..w {
            let sx = (x as i32 + wdx).clamp(0, w as i32 - 1) as usize;
            let sy = (y as i32 + wdy).clamp(0, h as i32 - 1) as usize;
            let si = (sy * w + sx) * 4;
            let di = (y * w + x) * 4;
            let mut rgb = [src[si], src[si + 1], src[si + 2]];
            rgb = decode_input(rgb, p.input_cs);
            rgb = [rgb[0] * exp_mul, rgb[1] * exp_mul, rgb[2] * exp_mul];
            rgb = film_response(rgb, p.stock, p.character, shoulder);

            // Signature: mild stock cast boost.
            let prof = stock_profile(p.stock);
            if p.signature > 1.0e-5 {
                for c in 0..3 {
                    rgb[c] = (rgb[c] + prof.cast[c] * p.signature * 0.25).max(0.0);
                }
            }
            rgb = apply_skin(rgb, p.skin_lift, p.skin_sat, p.skin_hue);

            if p.crossover_enable {
                rgb = apply_crossover(rgb, p.crossover, p.crossover_axis);
            }

            work[di] = rgb[0];
            work[di + 1] = rgb[1];
            work[di + 2] = rgb[2];
            work[di + 3] = src[si + 3];
        }
    }

    let min_dim = w.min(h) as f32;
    let q = p.quality.clamp(0.25, 1.0);

    // 4) Emulsion bleed (irradiation + dye cloud).
    if p.bleed_enable {
        let mut blur_a = vec![0.0f32; work.len()];
        let mut blur_b = vec![0.0f32; work.len()];
        if p.irradiation > 1.0e-5 {
            let r = p.irradiation_radius.max(0.0001) * min_dim;
            blur(&work, &mut blur_a, w, h, r, q);
            // Soft mix toward blurred (side scatter).
            for i in 0..work.len() {
                if i % 4 == 3 {
                    continue;
                }
                work[i] = work[i] * (1.0 - p.irradiation * 0.5) + blur_a[i] * (p.irradiation * 0.5);
            }
        }
        if p.dye_cloud > 1.0e-5 {
            let r = p.dye_radius.max(0.0001) * min_dim;
            blur(&work, &mut blur_b, w, h, r, q * 0.85);
            // Chrominance-weighted mix.
            for i in 0..(w * h) {
                let o = i * 4;
                let y0 = luma([work[o], work[o + 1], work[o + 2]]);
                let yb = luma([blur_b[o], blur_b[o + 1], blur_b[o + 2]]);
                for c in 0..3 {
                    let chroma = work[o + c] - y0;
                    let chroma_b = blur_b[o + c] - yb;
                    let ch = chroma * (1.0 - p.dye_cloud * 0.65) + chroma_b * (p.dye_cloud * 0.65);
                    work[o + c] = (y0 + ch).max(0.0);
                }
            }
        }
        // Interlayer edge: slight complementary fringe via local contrast on chroma.
        if p.interlayer > 1.0e-5 {
            let mut edge = vec![0.0f32; work.len()];
            blur(&work, &mut edge, w, h, 1.2, 0.5);
            for i in 0..(w * h) {
                let o = i * 4;
                for c in 0..3 {
                    let d = work[o + c] - edge[o + c];
                    // Push opposite channel slightly.
                    let opp = (c + 1) % 3;
                    work[o + opp] = (work[o + opp] - d * p.interlayer * 0.15).max(0.0);
                }
            }
        }
    }

    // 5) Halation
    if p.halation_enable && p.halation_strength > 1.0e-5 {
        let mut hi = vec![0.0f32; work.len()];
        let mut glow = vec![0.0f32; work.len()];
        let thr = p.halation_threshold.max(0.05);
        extract_highlights(&work, &mut hi, w, h, thr);
        let r = p.halation_radius.max(0.0005) * min_dim;
        blur(&hi, &mut glow, w, h, r, q);
        let flick = if p.halation_flicker > 1.0e-5 {
            1.0 + hash_f32(p.seed.wrapping_add(p.frame.wrapping_mul(13)))
                * p.halation_flicker
                * 0.35
        } else {
            1.0
        };
        add_tinted(
            &mut work,
            &glow,
            p.halation_color,
            p.halation_strength * flick,
            w,
            h,
        );
    }

    // 6) Print response after emulsion scatter and halation.
    for i in 0..(w * h) {
        let o = i * 4;
        let mut rgb = [work[o], work[o + 1], work[o + 2]];
        for channel in &mut rgb {
            *channel = (0.18 + (*channel - 0.18) * print_c).max(0.0);
        }
        rgb = saturate_around_luma(rgb, p.saturation.max(0.0));
        work[o] = rgb[0];
        work[o + 1] = rgb[1];
        work[o + 2] = rgb[2];
    }

    // 5) MTF + acutance
    if p.mtf_enable {
        let mut blurred = vec![0.0f32; work.len()];
        let soft_r = (0.4 + p.mtf_softness * 2.5).max(0.3);
        blur(&work, &mut blurred, w, h, soft_r, q);
        if p.mtf_softness > 1.0e-5 {
            for i in 0..work.len() {
                if i % 4 == 3 {
                    continue;
                }
                work[i] =
                    work[i] * (1.0 - p.mtf_softness * 0.65) + blurred[i] * (p.mtf_softness * 0.65);
            }
        }
        if p.acutance > 1.0e-5 {
            let mut sharp = vec![0.0f32; work.len()];
            // finer blur for edge extract
            let mut fine = vec![0.0f32; work.len()];
            blur(&work, &mut fine, w, h, 1.0, 0.6);
            acutance_sharpen(&work, &fine, &mut sharp, p.acutance * 0.85, w, h);
            work.copy_from_slice(&sharp);
        }
    }

    // 7) Grain in density space.
    grain(&mut work, w, h, p);
    apply_mottle(&mut work, w, h, p);

    // 8) Bloom / diffusion after print and grain.
    if p.bloom_enable && p.bloom_strength > 1.0e-5 {
        let mut hi = vec![0.0f32; work.len()];
        let mut glow = vec![0.0f32; work.len()];
        extract_highlights(&work, &mut hi, w, h, p.bloom_threshold.max(0.0));
        let r = p.bloom_radius.max(0.001) * min_dim;
        blur(&hi, &mut glow, w, h, r, q);
        mix_add(&mut work, &glow, p.bloom_strength, w, h);
    }
    if p.diffusion > 1.0e-5 {
        let mut soft = vec![0.0f32; work.len()];
        blur(&work, &mut soft, w, h, 0.02 * min_dim, q * 0.7);
        for i in 0..work.len() {
            if i % 4 != 3 {
                work[i] = work[i] * (1.0 - p.diffusion * 0.55) + soft[i] * (p.diffusion * 0.55);
            }
        }
    }

    // 7) Optics: CA, vignette, light leak
    apply_optics(&work, dst, w, h, p);

    // 9) Encode display-referred output.
    for i in 0..(w * h) {
        let o = i * 4;
        let rgb = encode_output([dst[o], dst[o + 1], dst[o + 2]], p.input_cs);
        dst[o] = rgb[0].clamp(0.0, 1.0e3);
        dst[o + 1] = rgb[1].clamp(0.0, 1.0e3);
        dst[o + 2] = rgb[2].clamp(0.0, 1.0e3);
        // alpha already set in apply_optics
    }
}

fn sample_rgba(buf: &[f32], w: usize, h: usize, x: f32, y: f32) -> [f32; 4] {
    let x = x.clamp(0.0, (w.saturating_sub(1)) as f32);
    let y = y.clamp(0.0, (h.saturating_sub(1)) as f32);
    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;
    let x1 = (x0 + 1).min(w - 1);
    let y1 = (y0 + 1).min(h - 1);
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;
    let i00 = (y0 * w + x0) * 4;
    let i10 = (y0 * w + x1) * 4;
    let i01 = (y1 * w + x0) * 4;
    let i11 = (y1 * w + x1) * 4;
    let mut o = [0.0f32; 4];
    for c in 0..4 {
        let a = buf[i00 + c] + (buf[i10 + c] - buf[i00 + c]) * tx;
        let b = buf[i01 + c] + (buf[i11 + c] - buf[i01 + c]) * tx;
        o[c] = a + (b - a) * ty;
    }
    o
}

fn apply_optics(src: &[f32], dst: &mut [f32], w: usize, h: usize, p: &FilmParams) {
    let cx = (w as f32 - 1.0) * 0.5;
    let cy = (h as f32 - 1.0) * 0.5;
    let max_r = (cx * cx + cy * cy).sqrt().max(1.0);

    let leak_boost = if p.leak_flicker > 1.0e-5 {
        (1.0 + hash_f32(p.seed.wrapping_add(p.frame.wrapping_mul(17))) * p.leak_flicker).max(0.0)
    } else {
        1.0
    };
    let leak_ang = p.leak_angle.to_radians();
    let leak_dir = [leak_ang.cos(), leak_ang.sin()];

    for y in 0..h {
        for x in 0..w {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let rn = (dx * dx + dy * dy).sqrt() / max_r;

            let px = x as f32;
            let py = y as f32;
            let mut pr = px;
            let mut pb = px;
            let mut pry = py;
            let mut pby = py;

            if p.ca_enable && p.ca_amount > 1.0e-5 {
                let fall = rn.powf(p.ca_falloff.max(0.1));
                let shift = p.ca_amount * 2.5 * fall;
                let ux = if rn > 1.0e-5 { dx / (rn * max_r) } else { 0.0 };
                let uy = if rn > 1.0e-5 { dy / (rn * max_r) } else { 0.0 };
                pr = px + ux * shift;
                pry = py + uy * shift;
                pb = px - ux * shift;
                pby = py - uy * shift;
            }

            let sr = sample_rgba(src, w, h, pr, pry);
            let sg = sample_rgba(src, w, h, px, py);
            let sb = sample_rgba(src, w, h, pb, pby);
            let mut rgb = [sr[0], sg[1], sb[2]];
            let a = sg[3];

            if p.vignette_enable && p.vignette_amount > 1.0e-5 {
                let soft = p.vignette_softness.max(0.05);
                let edge = p.vignette_radius.clamp(0.0, 1.0);
                let t = ((rn - edge) / soft).clamp(0.0, 1.0);
                let v = 1.0 - t * t * (3.0 - 2.0 * t) * p.vignette_amount;
                rgb[0] *= v;
                rgb[1] *= v;
                rgb[2] *= v;
            }

            if p.leak_amount > 1.0e-5 {
                // Gradient from edge along angle.
                let proj = (dx * leak_dir[0] + dy * leak_dir[1]) / max_r;
                let edge_f = ((proj + 1.0) * 0.5).clamp(0.0, 1.0);
                let soft = p.leak_softness.max(0.05);
                let mask = ((edge_f - (1.0 - soft)) / soft).clamp(0.0, 1.0);
                let mask = mask * mask * (3.0 - 2.0 * mask) * p.leak_amount * leak_boost;
                rgb[0] += p.leak_color[0] * mask;
                rgb[1] += p.leak_color[1] * mask;
                rgb[2] += p.leak_color[2] * mask;
            }

            let o = (y * w + x) * 4;
            dst[o] = rgb[0].max(0.0);
            dst[o + 1] = rgb[1].max(0.0);
            dst[o + 2] = rgb[2].max(0.0);
            dst[o + 3] = a;
        }
    }
}

/// Mean absolute error between two buffers (RGB only).
pub fn mae_rgb(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len()) / 4;
    if n == 0 {
        return 0.0;
    }
    let mut s = 0.0f32;
    for i in 0..n {
        let o = i * 4;
        s += (a[o] - b[o]).abs() + (a[o + 1] - b[o + 1]).abs() + (a[o + 2] - b[o + 2]).abs();
    }
    s / (n as f32 * 3.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::{FilmParams, FilmStock, LookPreset};

    fn solid(w: usize, h: usize, rgb: [f32; 3]) -> Vec<f32> {
        let mut v = vec![0.0f32; w * h * 4];
        for i in 0..(w * h) {
            let o = i * 4;
            v[o] = rgb[0];
            v[o + 1] = rgb[1];
            v[o + 2] = rgb[2];
            v[o + 3] = 1.0;
        }
        v
    }

    #[test]
    fn bypass_is_identity() {
        let w = 8;
        let h = 8;
        let src = solid(w, h, [0.2, 0.4, 0.6]);
        let mut dst = vec![0.0; src.len()];
        let mut p = FilmParams::default();
        p.bypass = true;
        render_cpu(&src, &mut dst, w, h, &p);
        assert_eq!(src, dst);
    }

    #[test]
    fn exposure_brightens() {
        let w = 4;
        let h = 4;
        let src = solid(w, h, [0.18, 0.18, 0.18]);
        let mut a = vec![0.0; src.len()];
        let mut b = vec![0.0; src.len()];
        let mut p0 = FilmParams::default();
        p0.grain_enable = false;
        p0.halation_enable = false;
        p0.bleed_enable = false;
        p0.mtf_enable = false;
        let mut p1 = p0.clone();
        p1.exposure = 1.0;
        render_cpu(&src, &mut a, w, h, &p0);
        render_cpu(&src, &mut b, w, h, &p1);
        assert!(b[0] > a[0]);
    }

    #[test]
    fn mono_stock_desaturates() {
        let w = 4;
        let h = 4;
        let src = solid(w, h, [0.6, 0.2, 0.1]);
        let mut dst = vec![0.0; src.len()];
        let mut p = FilmParams::default();
        p.stock = FilmStock::Mono400;
        p.grain_enable = false;
        p.halation_enable = false;
        p.bleed_enable = false;
        p.mtf_enable = false;
        render_cpu(&src, &mut dst, w, h, &p);
        let dr = (dst[0] - dst[1]).abs();
        let db = (dst[1] - dst[2]).abs();
        assert!(dr < 1.0e-3 && db < 1.0e-3);
    }

    #[test]
    fn classic_look_applies() {
        let mut p = FilmParams::default();
        p.look = LookPreset::Classic;
        p.apply_look();
        assert!((p.print_contrast - 1.12).abs() < 1.0e-5);
    }

    #[test]
    fn grain_changes_pixels() {
        let w = 32;
        let h = 32;
        let src = solid(w, h, [0.25, 0.25, 0.25]);
        let mut dst = vec![0.0; src.len()];
        let mut p = FilmParams::default();
        p.halation_enable = false;
        p.bleed_enable = false;
        p.mtf_enable = false;
        p.bloom_enable = false;
        p.grain_enable = true;
        p.grain_amount = 1.0;
        render_cpu(&src, &mut dst, w, h, &p);
        let mut diff = 0.0;
        for i in 0..(w * h) {
            diff += (dst[i * 4] - 0.25).abs();
        }
        assert!(diff > 0.01);
    }

    #[test]
    fn grey_ramp_is_finite_and_ordered() {
        let w = 64;
        let mut src = vec![0.0; w * 4];
        for x in 0..w {
            let v = x as f32 / (w - 1) as f32;
            src[x * 4] = v;
            src[x * 4 + 1] = v;
            src[x * 4 + 2] = v;
            src[x * 4 + 3] = 1.0;
        }
        let mut dst = vec![0.0; src.len()];
        let mut p = FilmParams::default();
        p.grain_enable = false;
        p.halation_enable = false;
        p.bleed_enable = false;
        render_cpu(&src, &mut dst, w, 1, &p);
        assert!(dst.iter().all(|v| v.is_finite()));
        for x in 1..w {
            assert!(dst[x * 4] + 1.0e-5 >= dst[(x - 1) * 4]);
        }
    }

    #[test]
    fn point_source_creates_halation_neighbours() {
        let (w, h) = (33, 33);
        let mut src = solid(w, h, [0.0, 0.0, 0.0]);
        let c = ((h / 2) * w + w / 2) * 4;
        src[c] = 4.0;
        src[c + 1] = 4.0;
        src[c + 2] = 4.0;
        let mut dst = vec![0.0; src.len()];
        let mut p = FilmParams::default();
        p.grain_enable = false;
        p.bleed_enable = false;
        p.halation_enable = true;
        p.halation_threshold = 0.1;
        p.halation_radius = 0.1;
        p.halation_strength = 1.0;
        render_cpu(&src, &mut dst, w, h, &p);
        let n = c + 4;
        assert!(dst[n] > 0.0);
    }
}
