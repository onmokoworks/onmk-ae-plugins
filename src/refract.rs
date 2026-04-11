/// Refraction + chromatic dispersion image effect.
///
/// Port of the shader principles from
/// https://blog.maximeheckel.com/posts/refraction-dispersion-and-other-shader-light-effects/
/// to a 2D compositing context.
///
/// Pipeline:
///   1. Rasterized mask layer -> blurred height field H(x,y)
///   2. H -> pseudo surface normal N(x,y) via gradient
///   3. For each pixel inside the mask:
///        eye = (0,0,-1)
///        for i in 0..samples:
///            refractVec_C = refract(eye, N, 1/IOR_C)   for C in {R,G,B} or rygcbv
///            accumulate bg[uv + refractVec_C.xy * strength_C(i)]
///        saturation boost
///        + Blinn-Phong specular (diffuse+spec) modulated by Fresnel
///   4. Composite result over input using mask alpha, then Mix slider
///
/// Pixel format: flat ARGB u8, 4 bytes per pixel (same convention as DistortChroma).

use crate::RefractParams;
use rayon::prelude::*;

// ---- Box blur (3-pass Gaussian approximation), operating on an f32 plane ----

fn box_blur_3pass(src: &[f32], w: usize, h: usize, sigma: f64) -> Vec<f32> {
    if sigma < 0.5 {
        return src.to_vec();
    }
    let boxes = boxes_for_gauss(sigma);
    let mut buf_a = src.to_vec();
    let mut buf_b = vec![0.0f32; w * h];

    for &box_r in &boxes {
        box_blur_pass(&buf_a, &mut buf_b, w, h, box_r);
        std::mem::swap(&mut buf_a, &mut buf_b);
    }
    buf_a
}

fn boxes_for_gauss(sigma: f64) -> [usize; 3] {
    let n = 3.0f64;
    let w_ideal = ((12.0 * sigma * sigma / n) + 1.0).sqrt();
    let mut wl = w_ideal.floor() as usize;
    if wl % 2 == 0 && wl > 0 {
        wl -= 1;
    }
    if wl == 0 {
        wl = 1;
    }
    let wu = wl + 2;
    let m = ((12.0 * sigma * sigma
        - (n * wl as f64 * wl as f64)
        - (4.0 * n * wl as f64)
        - (3.0 * n))
        / (-4.0 * wl as f64 - 4.0))
        .round() as usize;

    let mut sizes = [0usize; 3];
    for i in 0..3 {
        sizes[i] = if i < m { wl } else { wu };
    }
    for s in &mut sizes {
        *s = (*s).max(1) / 2;
    }
    sizes
}

fn box_blur_pass(src: &[f32], dst: &mut [f32], w: usize, h: usize, r: usize) {
    if r == 0 {
        dst.copy_from_slice(src);
        return;
    }
    let mut tmp = vec![0.0f32; w * h];

    let diam = (2 * r + 1) as f32;
    let inv = 1.0 / diam;

    // Horizontal
    for y in 0..h {
        let row = y * w;
        let mut running = 0.0f32;
        for i in 0..=r.min(w - 1) {
            running += src[row + i];
        }
        running += r as f32 * src[row];

        for x in 0..w {
            tmp[row + x] = running * inv;
            let add_x = (x + r + 1).min(w - 1);
            let rem_x = (x as isize - r as isize).max(0) as usize;
            running += src[row + add_x] - src[row + rem_x];
        }
    }

    // Vertical
    for x in 0..w {
        let mut running = 0.0f32;
        for i in 0..=r.min(h - 1) {
            running += tmp[i * w + x];
        }
        running += r as f32 * tmp[x];

        for y in 0..h {
            dst[y * w + x] = running * inv;
            let add_y = (y + r + 1).min(h - 1);
            let rem_y = (y as isize - r as isize).max(0) as usize;
            running += tmp[add_y * w + x] - tmp[rem_y * w + x];
        }
    }
}

// ---- Bilinear RGB sampling with clamped boundary ----

#[inline]
fn clamp_coord(v: f32, max: f32) -> f32 {
    if max <= 1.0 {
        return 0.0;
    }
    v.clamp(0.0, max - 1.0)
}

#[inline]
fn sample_bilinear_rgb(buf: &[u8], w: usize, h: usize, fx: f32, fy: f32) -> (f32, f32, f32) {
    let fx = clamp_coord(fx, w as f32);
    let fy = clamp_coord(fy, h as f32);

    let x0 = fx as usize;
    let y0 = fy as usize;
    let x1 = (x0 + 1).min(w - 1);
    let y1 = (y0 + 1).min(h - 1);
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
        buf[o00 + 1] as f32 * w00 + buf[o10 + 1] as f32 * w10
            + buf[o01 + 1] as f32 * w01 + buf[o11 + 1] as f32 * w11,
        buf[o00 + 2] as f32 * w00 + buf[o10 + 2] as f32 * w10
            + buf[o01 + 2] as f32 * w01 + buf[o11 + 2] as f32 * w11,
        buf[o00 + 3] as f32 * w00 + buf[o10 + 3] as f32 * w10
            + buf[o01 + 3] as f32 * w01 + buf[o11 + 3] as f32 * w11,
    )
}

// ---- Vec3 helpers ----

#[inline]
fn v_normalize(x: f32, y: f32, z: f32) -> (f32, f32, f32) {
    let m = (x * x + y * y + z * z).sqrt().max(1e-8);
    (x / m, y / m, z / m)
}

#[inline]
fn v_dot(ax: f32, ay: f32, az: f32, bx: f32, by: f32, bz: f32) -> f32 {
    ax * bx + ay * by + az * bz
}

/// GLSL-style refract().
/// I is the incident vector (pointing into the surface),
/// N is the surface normal (pointing out of the surface),
/// eta is the ratio n1/n2.
#[inline]
fn refract(ix: f32, iy: f32, iz: f32, nx: f32, ny: f32, nz: f32, eta: f32) -> (f32, f32, f32) {
    let n_dot_i = v_dot(nx, ny, nz, ix, iy, iz);
    let k = 1.0 - eta * eta * (1.0 - n_dot_i * n_dot_i);
    if k < 0.0 {
        (0.0, 0.0, 0.0)
    } else {
        let s = eta * n_dot_i + k.sqrt();
        (eta * ix - s * nx, eta * iy - s * ny, eta * iz - s * nz)
    }
}

// ---- Main render ----

pub fn render(
    rp: &RefractParams,
    src: &[u8],
    mask: Option<&[u8]>,
    bg: Option<&[u8]>,
    w: usize,
    h: usize,
) -> Vec<u8> {
    let npx = w * h;

    // Background texture: dedicated layer if provided, else the input layer.
    let bg_buf = bg.unwrap_or(src);

    // 1) Mask luminance -> height field in [0,1]
    let mask_buf = mask.unwrap_or(src);
    let mut height = vec![0.0f32; npx];
    let mut mask_a = vec![0.0f32; npx];
    for i in 0..npx {
        let off = i * 4;
        let a = mask_buf[off] as f32 / 255.0;
        let r = mask_buf[off + 1] as f32;
        let g = mask_buf[off + 2] as f32;
        let b = mask_buf[off + 3] as f32;
        // Height uses luminance pre-multiplied by alpha so that a transparent
        // mask layer contributes zero height. This lets the user draw an AE
        // mask path and feed the masked layer in as "shape".
        let luma = (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255.0;
        height[i] = luma * a;
        mask_a[i] = a;
    }

    // 2) Blur the height field (for smooth normals near the edge) and
    //    OPTIONALLY blur the compositing mask (for soft silhouettes).
    //    We keep these as two separate blurs so height normal scale is
    //    independent of silhouette softness.
    let h_blurred = box_blur_3pass(&height, w, h, rp.height_blur);
    let composite_mask = if rp.edge_blur >= 0.5 {
        box_blur_3pass(&mask_a, w, h, rp.edge_blur)
    } else {
        mask_a
    };

    // 3) Compute normals from height gradient.
    //    N = normalize(-dH/dx * k, -dH/dy * k, 1)
    //
    // At a blurred edge the gradient magnitude is roughly 1/(2*sigma), so we
    // compensate by multiplying by the blur radius to keep the normal tilt
    // canvas-independent.
    let strength = rp.height_strength.max(0.0);
    let blur_scale = (rp.height_blur as f32).max(1.0);
    let k = strength * blur_scale * 1.5;
    let mut normals = vec![(0.0f32, 0.0f32, 1.0f32); npx];
    normals
        .par_chunks_mut(w)
        .enumerate()
        .for_each(|(y, row)| {
            let y0 = if y > 0 { y - 1 } else { 0 };
            let y1 = if y + 1 < h { y + 1 } else { h - 1 };
            let r0 = y0 * w;
            let r1 = y1 * w;
            let rc = y * w;
            for x in 0..w {
                let x0 = if x > 0 { x - 1 } else { 0 };
                let x1 = if x + 1 < w { x + 1 } else { w - 1 };
                let dhdx = (h_blurred[rc + x1] - h_blurred[rc + x0]) * 0.5;
                let dhdy = (h_blurred[r1 + x] - h_blurred[r0 + x]) * 0.5;
                let nx = -dhdx * k;
                let ny = -dhdy * k;
                let nz = 1.0;
                row[x] = v_normalize(nx, ny, nz);
            }
        });

    // 4) Precompute light direction (unit vector pointing TOWARD the light).
    //    Angle X = azimuth, Angle Y = elevation (both degrees).
    let ax = rp.light_angle_x.to_radians();
    let ay = rp.light_angle_y.to_radians();
    let lx = ay.cos() * ax.sin();
    let ly = ay.sin();
    let lz = ay.cos() * ax.cos();
    let (lx, ly, lz) = v_normalize(lx, ly, lz);

    // Eye = incident direction into the surface (camera at +Z, looking -Z).
    let (ex, ey, ez) = (0.0f32, 0.0f32, -1.0f32);

    // Half-vector: H = normalize(-eye + L) = normalize((0,0,1) + L)
    let (hx, hy, hz) = v_normalize(-ex + lx, -ey + ly, -ez + lz);

    // 5) Precompute per-sample strengths.
    let samples = rp.samples.max(1);
    let power = rp.refract_power;

    // Per-axis chroma: when disabled, both axes share the same `chromatic_ab`.
    // When enabled, X and Y use independent values so the user can e.g. get
    // a purely horizontal rainbow smear.
    let (chroma_x, chroma_y) = if rp.use_per_axis_chroma {
        (rp.chromatic_ab_x, rp.chromatic_ab_y)
    } else {
        (rp.chromatic_ab, rp.chromatic_ab)
    };

    // For each sample i, offset multiplier per channel: (1 + slide * ch_mult * chroma)
    // slide = (i/samples) * 0.1 (mirrors the blog's loop).
    let mut strength_r_x = vec![0.0f32; samples];
    let mut strength_g_x = vec![0.0f32; samples];
    let mut strength_b_x = vec![0.0f32; samples];
    let mut strength_r_y = vec![0.0f32; samples];
    let mut strength_g_y = vec![0.0f32; samples];
    let mut strength_b_y = vec![0.0f32; samples];
    for i in 0..samples {
        let slide = (i as f32 / samples as f32) * 0.1;
        strength_r_x[i] = power * (1.0 + slide * 1.0 * chroma_x);
        strength_g_x[i] = power * (1.0 + slide * 2.0 * chroma_x);
        strength_b_x[i] = power * (1.0 + slide * 3.0 * chroma_x);
        strength_r_y[i] = power * (1.0 + slide * 1.0 * chroma_y);
        strength_g_y[i] = power * (1.0 + slide * 2.0 * chroma_y);
        strength_b_y[i] = power * (1.0 + slide * 3.0 * chroma_y);
    }

    // 6ch (rygcbv) extra precomputation. We spread IORs across 6 wavelength
    // slots between IorR and IorB (with a small extension beyond blue for
    // violet), each with its own per-sample strength array.
    let iors_6 = if rp.use_6ch {
        let i_r = rp.ior_r;
        let i_g = rp.ior_g;
        let i_b = rp.ior_b;
        // red, yellow, green, cyan, blue, violet
        [
            i_r,
            i_r + (i_g - i_r) * 0.5,
            i_g,
            i_g + (i_b - i_g) * 0.5,
            i_b,
            i_b + (i_b - i_r) * 0.25,
        ]
    } else {
        [1.0; 6]
    };
    let mut strengths_6_x = [[0.0f32; 64]; 6];
    let mut strengths_6_y = [[0.0f32; 64]; 6];
    if rp.use_6ch {
        for ch in 0..6 {
            let mult = 1.0 + ch as f32 * 0.4; // spread channel separation
            for i in 0..samples {
                let slide = (i as f32 / samples as f32) * 0.1;
                strengths_6_x[ch][i] = power * (1.0 + slide * mult * chroma_x);
                strengths_6_y[ch][i] = power * (1.0 + slide * mult * chroma_y);
            }
        }
    }

    // Precompute IOR ratios eta = 1/IOR.
    let eta_r = 1.0 / rp.ior_r.max(1.0);
    let eta_g = 1.0 / rp.ior_g.max(1.0);
    let eta_b = 1.0 / rp.ior_b.max(1.0);
    let etas_6 = [
        1.0 / iors_6[0].max(1.0),
        1.0 / iors_6[1].max(1.0),
        1.0 / iors_6[2].max(1.0),
        1.0 / iors_6[3].max(1.0),
        1.0 / iors_6[4].max(1.0),
        1.0 / iors_6[5].max(1.0),
    ];

    let inv_samples = 1.0 / samples as f32;

    // 6) Main loop — parallelized per row.
    let mut out = vec![0u8; npx * 4];
    let mix = rp.mix;
    let inv_mix = 1.0 - mix;

    let normals_ref = &normals;
    let composite_mask_ref = &composite_mask;
    let src_ref = src;
    let bg_ref = bg_buf;
    let strength_r_x = &strength_r_x;
    let strength_g_x = &strength_g_x;
    let strength_b_x = &strength_b_x;
    let strength_r_y = &strength_r_y;
    let strength_g_y = &strength_g_y;
    let strength_b_y = &strength_b_y;
    let strengths_6_x_ref = &strengths_6_x;
    let strengths_6_y_ref = &strengths_6_y;
    let etas_6_ref = &etas_6;

    out.par_chunks_mut(w * 4).enumerate().for_each(|(y, out_row)| {
      for x in 0..w {
        let idx = y * w + x;
        let off = x * 4;
        let src_off = idx * 4;

        let src_a = src_ref[src_off];
        let src_r = src_ref[src_off + 1] as f32;
        let src_g = src_ref[src_off + 2] as f32;
        let src_b = src_ref[src_off + 3] as f32;

        let m = composite_mask_ref[idx].clamp(0.0, 1.0);

        // Outside the mask entirely: pass through.
        if m <= 1e-4 {
            out_row[off] = src_a;
            out_row[off + 1] = src_ref[src_off + 1];
            out_row[off + 2] = src_ref[src_off + 2];
            out_row[off + 3] = src_ref[src_off + 3];
            continue;
        }

        let (nx, ny, nz) = normals_ref[idx];

        // ---- Refraction sampling ----
        let (mut acc_r, mut acc_g, mut acc_b);
        if !rp.use_6ch {
            let (rvrx, rvry, _) = refract(ex, ey, ez, nx, ny, nz, eta_r);
            let (rvgx, rvgy, _) = refract(ex, ey, ez, nx, ny, nz, eta_g);
            let (rvbx, rvby, _) = refract(ex, ey, ez, nx, ny, nz, eta_b);

            let mut ar = 0.0f32;
            let mut ag = 0.0f32;
            let mut ab = 0.0f32;
            for i in 0..samples {
                let (r_px, _, _) = sample_bilinear_rgb(
                    bg_ref, w, h,
                    x as f32 + rvrx * strength_r_x[i],
                    y as f32 + rvry * strength_r_y[i],
                );
                let (_, g_px, _) = sample_bilinear_rgb(
                    bg_ref, w, h,
                    x as f32 + rvgx * strength_g_x[i],
                    y as f32 + rvgy * strength_g_y[i],
                );
                let (_, _, b_px) = sample_bilinear_rgb(
                    bg_ref, w, h,
                    x as f32 + rvbx * strength_b_x[i],
                    y as f32 + rvby * strength_b_y[i],
                );
                ar += r_px;
                ag += g_px;
                ab += b_px;
            }
            acc_r = ar * inv_samples;
            acc_g = ag * inv_samples;
            acc_b = ab * inv_samples;
        } else {
            let mut ch = [0.0f32; 6];
            for slot in 0..6 {
                let (rvx, rvy, _) = refract(ex, ey, ez, nx, ny, nz, etas_6_ref[slot]);
                let mut acc_sr = 0.0f32;
                let mut acc_sg = 0.0f32;
                let mut acc_sb = 0.0f32;
                for i in 0..samples {
                    let sx = strengths_6_x_ref[slot][i];
                    let sy = strengths_6_y_ref[slot][i];
                    let (sr, sg, sb) = sample_bilinear_rgb(
                        bg_ref, w, h,
                        x as f32 + rvx * sx,
                        y as f32 + rvy * sy,
                    );
                    acc_sr += sr;
                    acc_sg += sg;
                    acc_sb += sb;
                }
                acc_sr *= inv_samples;
                acc_sg *= inv_samples;
                acc_sb *= inv_samples;
                ch[slot] = match slot {
                    0 => acc_sr * 0.5,
                    1 => (2.0 * acc_sr + 2.0 * acc_sg - acc_sb) / 6.0,
                    2 => acc_sg * 0.5,
                    3 => (2.0 * acc_sg + 2.0 * acc_sb - acc_sr) / 6.0,
                    4 => acc_sb * 0.5,
                    _ => (2.0 * acc_sb + 2.0 * acc_sr - acc_sg) / 6.0,
                };
            }
            let (r_c, y_c, g_c, c_c, b_c, v_c) = (ch[0], ch[1], ch[2], ch[3], ch[4], ch[5]);
            acc_r = r_c + (2.0 * v_c + 2.0 * y_c - c_c) / 3.0;
            acc_g = g_c + (2.0 * y_c + 2.0 * c_c - v_c) / 3.0;
            acc_b = b_c + (2.0 * c_c + 2.0 * v_c - y_c) / 3.0;
        }

        // ---- Saturation boost ----
        let luma = 0.2125 * acc_r + 0.7154 * acc_g + 0.0721 * acc_b;
        acc_r = luma + (acc_r - luma) * rp.saturation;
        acc_g = luma + (acc_g - luma) * rp.saturation;
        acc_b = luma + (acc_b - luma) * rp.saturation;

        // ---- Blinn-Phong lighting ----
        let ndoth = v_dot(nx, ny, nz, hx, hy, hz).max(0.0);
        let k_spec = ndoth.powf(rp.shininess.max(1.0));
        let ndotl = v_dot(nx, ny, nz, lx, ly, lz).max(0.0);
        let k_diff = ndotl * rp.diffuseness;

        let fresnel_factor = v_dot(ex, ey, ez, nx, ny, nz).abs();
        let fresnel = (1.0 - fresnel_factor).max(0.0).powf(rp.fresnel_power.max(0.0));

        let diff_mult = 1.0 + k_diff;
        let spec_add = k_spec * fresnel * 200.0;
        acc_r = acc_r * diff_mult + spec_add;
        acc_g = acc_g * diff_mult + spec_add;
        acc_b = acc_b * diff_mult + spec_add;

        // ---- Mask composite ----
        let comp_r = src_r * (1.0 - m) + acc_r * m;
        let comp_g = src_g * (1.0 - m) + acc_g * m;
        let comp_b = src_b * (1.0 - m) + acc_b * m;

        let fin_r = src_r * inv_mix + comp_r * mix;
        let fin_g = src_g * inv_mix + comp_g * mix;
        let fin_b = src_b * inv_mix + comp_b * mix;

        out_row[off] = src_a;
        out_row[off + 1] = fin_r.clamp(0.0, 255.0) as u8;
        out_row[off + 2] = fin_g.clamp(0.0, 255.0) as u8;
        out_row[off + 3] = fin_b.clamp(0.0, 255.0) as u8;
      }
    });

    out
}
