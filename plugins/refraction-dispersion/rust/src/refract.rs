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
    let m =
        ((12.0 * sigma * sigma - (n * wl as f64 * wl as f64) - (4.0 * n * wl as f64) - (3.0 * n))
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

// ---- Glass-edge inner distance band ----

/// Exact Euclidean distance transform (Meijster's separable 2-pass algorithm,
/// the same exact-EDT used by OpenCV's `DIST_L2`/`DIST_MASK_PRECISE`). For every
/// pixel INSIDE the shape (`mask >= 0.5`) returns the true distance (px) to the
/// nearest outside pixel; outside pixels return 0. O(npx), implemented from the
/// published algorithm (not copied from any plugin source).
fn inner_distance(mask: &[f32], w: usize, h: usize) -> Vec<f32> {
    let n = w * h;
    if n == 0 {
        return Vec::new();
    }
    let inf = (w + h) as f32; // larger than any in-frame distance
                              // Phase 1: vertical 1-D distance g(x,y) to nearest "outside" pixel in column.
    let mut g = vec![0.0f32; n];
    for x in 0..w {
        g[x] = if mask[x] < 0.5 { 0.0 } else { inf };
        for y in 1..h {
            let i = y * w + x;
            g[i] = if mask[i] < 0.5 {
                0.0
            } else {
                (g[i - w] + 1.0).min(inf)
            };
        }
        for y in (0..h - 1).rev() {
            let i = y * w + x;
            let below = g[i + w] + 1.0;
            if below < g[i] {
                g[i] = below;
            }
        }
    }

    // Phase 2: per-row lower envelope of parabolas f(x,i) = (x-i)^2 + g(i)^2.
    let mut d = vec![0.0f32; n];
    let mut s = vec![0usize; w];
    let mut t = vec![0usize; w];
    for y in 0..h {
        let row = y * w;
        let gf = |i: usize| g[row + i] as f64;
        let f = |x: usize, i: usize| {
            let dx = x as f64 - i as f64;
            dx * dx + gf(i) * gf(i)
        };
        let sep = |i: usize, u: usize| {
            (u as f64 * u as f64 - i as f64 * i as f64 + gf(u) * gf(u) - gf(i) * gf(i))
                / (2.0 * (u as f64 - i as f64))
        };
        let mut q: i64 = 0;
        s[0] = 0;
        t[0] = 0;
        for u in 1..w {
            while q >= 0 && f(t[q as usize], s[q as usize]) > f(t[q as usize], u) {
                q -= 1;
            }
            if q < 0 {
                q = 0;
                s[0] = u;
            } else {
                let wf = (1.0 + sep(s[q as usize], u)).floor();
                if wf < w as f64 {
                    q += 1;
                    s[q as usize] = u;
                    t[q as usize] = wf.max(0.0) as usize;
                }
            }
        }
        for u in (0..w).rev() {
            d[row + u] = (f(u, s[q as usize]) as f32).max(0.0).sqrt();
            if u == t[q as usize] && q > 0 {
                q -= 1;
            }
        }
    }
    d
}

#[inline]
fn smoothstep01(edge0: f32, edge1: f32, x: f32) -> f32 {
    if edge1 <= edge0 {
        return if x >= edge1 { 1.0 } else { 0.0 };
    }
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Build the blurred inner rim band: 1 at the shape edge, falling to 0 by
/// `width` px inside, then box-blurred by `blur`.
///
/// The distance field is necessarily computed from a hard (binary) silhouette,
/// which would give the band a 1px stair-stepped outer edge. To avoid that
/// jaggy, the band is modulated by the original (anti-aliased) coverage alpha,
/// so the rim inherits the mask's smooth edge instead of the binary one.
fn edge_band_from_dist(
    dist: &[f32],
    mask: &[f32],
    w: usize,
    h: usize,
    width: f32,
    blur: f64,
) -> Vec<f32> {
    let mut band = vec![0.0f32; w * h];
    let width = width.max(0.5);
    for i in 0..(w * h) {
        let cov = mask[i].clamp(0.0, 1.0);
        if cov > 0.0 {
            band[i] = (1.0 - smoothstep01(0.0, width, dist[i])) * cov;
        }
    }
    if blur < 0.5 {
        return band;
    }
    // The box blur softens the rim, but blurring against the empty (0) outside
    // pulls the band down at the very edge, leaving a few-px strip where the
    // effect barely reaches. Take the max of the blurred and the raw band so the
    // rim stays anchored at the silhouette while only the inner side softens.
    let blurred = box_blur_3pass(&band, w, h, blur);
    for i in 0..(w * h) {
        band[i] = band[i].max(blurred[i]);
    }
    band
}

/// Blur all four channels (A,R,G,B) of an ARGB buffer. Used to pre-soften the
/// mask/map layer so the derived height AND coverage become gentle together.
/// The four channel blurs are independent, so they run in parallel (otherwise
/// this is ~4x the cost of a single-plane blur like Height Blur).
fn blur_rgba_3pass(src: &[u8], w: usize, h: usize, sigma: f64) -> Vec<u8> {
    if sigma < 0.5 {
        return src.to_vec();
    }
    let npx = w * h;
    let mut planes: Vec<Vec<f32>> = (0..4)
        .map(|c| (0..npx).map(|i| src[i * 4 + c] as f32).collect())
        .collect();
    planes
        .par_iter_mut()
        .for_each(|p| *p = box_blur_3pass(p, w, h, sigma));
    let mut dst = vec![0u8; npx * 4];
    for i in 0..npx {
        let off = i * 4;
        for c in 0..4 {
            dst[off + c] = planes[c][i].clamp(0.0, 255.0) as u8;
        }
    }
    dst
}

fn blur_rgb_3pass(src: &[u8], w: usize, h: usize, sigma: f64) -> Vec<u8> {
    if sigma < 0.5 {
        return src.to_vec();
    }

    let npx = w * h;
    // Blur R/G/B independently in parallel (alpha is left untouched).
    let mut planes: Vec<Vec<f32>> = (1..4)
        .map(|c| (0..npx).map(|i| src[i * 4 + c] as f32).collect())
        .collect();
    planes
        .par_iter_mut()
        .for_each(|p| *p = box_blur_3pass(p, w, h, sigma));

    let mut dst = src.to_vec();
    for i in 0..npx {
        let off = i * 4;
        dst[off + 1] = planes[0][i].clamp(0.0, 255.0) as u8;
        dst[off + 2] = planes[1][i].clamp(0.0, 255.0) as u8;
        dst[off + 3] = planes[2][i].clamp(0.0, 255.0) as u8;
    }
    dst
}

fn apply_affected_blur(out: &mut [u8], w: usize, h: usize, sigma: f64, mask: &[f32]) {
    if sigma < 0.5 {
        return;
    }

    let blurred = blur_rgb_3pass(out, w, h, sigma);
    for i in 0..(w * h) {
        let m = mask[i].clamp(0.0, 1.0);
        if m <= 1e-4 {
            continue;
        }

        let off = i * 4;
        let inv = 1.0 - m;
        out[off + 1] =
            (out[off + 1] as f32 * inv + blurred[off + 1] as f32 * m).clamp(0.0, 255.0) as u8;
        out[off + 2] =
            (out[off + 2] as f32 * inv + blurred[off + 2] as f32 * m).clamp(0.0, 255.0) as u8;
        out[off + 3] =
            (out[off + 3] as f32 * inv + blurred[off + 3] as f32 * m).clamp(0.0, 255.0) as u8;
    }
}

fn mask_map(src: &[u8], mask: &[f32], w: usize, h: usize) -> Vec<u8> {
    let mut out = vec![0u8; w * h * 4];
    for i in 0..(w * h) {
        let off = i * 4;
        let v = (mask[i].clamp(0.0, 1.0) * 255.0) as u8;
        out[off] = src[off];
        out[off + 1] = v;
        out[off + 2] = v;
        out[off + 3] = v;
    }
    out
}

fn delta_map(src: &[u8], out: &[u8], w: usize, h: usize) -> Vec<u8> {
    let mut map = vec![0u8; w * h * 4];
    for i in 0..(w * h) {
        let off = i * 4;
        map[off] = src[off];
        map[off + 1] = out[off + 1].abs_diff(src[off + 1]);
        map[off + 2] = out[off + 2].abs_diff(src[off + 2]);
        map[off + 3] = out[off + 3].abs_diff(src[off + 3]);
    }
    map
}

/// Diagnostic: visualise the region AE actually handed us and where the mask
/// landed. GREEN border = the edge of the rendered buffer (the requested
/// region). If AE renders the full frame it hugs the comp edges; if it renders
/// a partial ROI it appears as a rectangle INSIDE the comp (and will jump as the
/// ROI moves frame-to-frame). MAGENTA = mask coverage (where the effect applies
/// / how the mask layer was placed).
fn debug_regions(src: &[u8], mask: &[f32], w: usize, h: usize) -> Vec<u8> {
    let mut out = vec![0u8; w * h * 4];
    let border = if w.min(h) > 8 { 3 } else { 1 };
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let off = i * 4;
            // Dimmed source as backdrop.
            let mut r = src[off + 1] as f32 * 0.3;
            let mut g = src[off + 2] as f32 * 0.3;
            let mut b = src[off + 3] as f32 * 0.3;
            // Mask coverage -> magenta tint.
            let m = mask[i].clamp(0.0, 1.0);
            if m > 1e-3 {
                r += 200.0 * m;
                b += 200.0 * m;
            }
            // Buffer-edge border -> green.
            if x < border || x + border >= w || y < border || y + border >= h {
                r = 0.0;
                g = 255.0;
                b = 0.0;
            }
            out[off] = src[off];
            out[off + 1] = r.clamp(0.0, 255.0) as u8;
            out[off + 2] = g.clamp(0.0, 255.0) as u8;
            out[off + 3] = b.clamp(0.0, 255.0) as u8;
        }
    }
    out
}

#[inline]
fn apply_color_adjust(v: f32, brightness: f32, contrast: f32) -> f32 {
    let contrast_factor = (1.0 + contrast).max(0.0);
    (v - 128.0) * contrast_factor + 128.0 + brightness * 255.0
}

// ---- Out-of-frame handling ----
//
// AE only hands us the clipped layer tile, so any refracted tap that lands
// beyond the frame has no real data behind it. `Edge Mode` picks how to invent
// it:
//   1 Mirror      reflect the coordinate back into the frame (continuous at the
//                 border -> no seam, lens never "vanishes")
//   2 Clamp       repeat the last edge pixel (stretched edge)
//   3 Clamp+Fade  clamp, but the further a tap reaches outside the frame the
//                 more it fades back to the source pixel, so the stretched edge
//                 dissolves into the original image instead of streaking
pub const EDGE_MIRROR: i32 = 1;
/// Documents the popup value; Clamp is the default branch so the constant is
/// not referenced directly.
#[allow(dead_code)]
pub const EDGE_CLAMP: i32 = 2;
pub const EDGE_FADE: i32 = 3;

/// Ramp width (in pixels of outside-distance) over which Clamp+Fade blends from
/// the clamped edge color back to the source pixel.
const EDGE_FADE_RAMP: f32 = 24.0;

// Glass Edge bevel modes.
/// Documents the popup value; Height-add is the default branch so the constant
/// is not referenced directly.
#[allow(dead_code)]
pub const BEVEL_HEIGHT_ADD: i32 = 1;
pub const BEVEL_SDF: i32 = 2;

#[inline]
fn clamp_coord(v: f32, max: f32) -> f32 {
    if max <= 1.0 {
        return 0.0;
    }
    v.clamp(0.0, max - 1.0)
}

/// Reflect a coordinate into [0, max] by folding at the borders. Continuous at
/// the edge (value matches), so refracting across it produces no hard seam.
#[inline]
fn reflect_coord(v: f32, max: f32) -> f32 {
    if max <= 0.0 {
        return 0.0;
    }
    let period = 2.0 * max;
    let m = v.rem_euclid(period);
    if m > max {
        period - m
    } else {
        m
    }
}

/// How far (in px) a coordinate lies outside the frame; 0 when inside.
#[inline]
fn outside_dist(w: usize, h: usize, fx: f32, fy: f32) -> f32 {
    let mx = (w - 1) as f32;
    let my = (h - 1) as f32;
    let dx = if fx < 0.0 {
        -fx
    } else if fx > mx {
        fx - mx
    } else {
        0.0
    };
    let dy = if fy < 0.0 {
        -fy
    } else if fy > my {
        fy - my
    } else {
        0.0
    };
    dx.max(dy)
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
        buf[o00 + 1] as f32 * w00
            + buf[o10 + 1] as f32 * w10
            + buf[o01 + 1] as f32 * w01
            + buf[o11 + 1] as f32 * w11,
        buf[o00 + 2] as f32 * w00
            + buf[o10 + 2] as f32 * w10
            + buf[o01 + 2] as f32 * w01
            + buf[o11 + 2] as f32 * w11,
        buf[o00 + 3] as f32 * w00
            + buf[o10 + 3] as f32 * w10
            + buf[o01 + 3] as f32 * w01
            + buf[o11 + 3] as f32 * w11,
    )
}

/// Sample with Mirror or Clamp edge handling (no fallback needed: both always
/// return real pixel data). Mirror reflects the coordinate; Clamp lets the
/// bilinear sampler clamp to the edge.
#[inline]
fn sample_edge(
    buf: &[u8],
    w: usize,
    h: usize,
    fx: f32,
    fy: f32,
    edge_mode: i32,
) -> (f32, f32, f32) {
    if edge_mode == EDGE_MIRROR {
        let rx = reflect_coord(fx, (w - 1) as f32);
        let ry = reflect_coord(fy, (h - 1) as f32);
        sample_bilinear_rgb(buf, w, h, rx, ry)
    } else {
        sample_bilinear_rgb(buf, w, h, fx, fy)
    }
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
    // Optionally pre-blur it so the refraction samples a softened backdrop
    // (frosted-glass look), independent of any mask.
    let bg_src = bg.unwrap_or(src);
    let bg_blurred_storage: Vec<u8>;
    let bg_buf: &[u8] = if rp.bg_blur >= 0.5 {
        bg_blurred_storage = blur_rgb_3pass(bg_src, w, h, rp.bg_blur);
        &bg_blurred_storage
    } else {
        bg_src
    };

    // 1) Mask layer -> height field [0,1] and coverage [0,1].
    //    `Height Source` picks which channel/value drives the bump (and thus the
    //    refraction normals); `Coverage Source` picks where the effect is
    //    applied. They are independent so e.g. a grayscale map can drive height
    //    across the whole frame (coverage = Full).
    let mask_src = mask.unwrap_or(src);
    // Optional pre-blur of the map layer itself (softens height AND coverage
    // together, so the whole effect applies more gradually).
    let mask_blurred_storage: Vec<u8>;
    let mask_buf: &[u8] = if rp.map_blur >= 0.5 {
        mask_blurred_storage = blur_rgba_3pass(mask_src, w, h, rp.map_blur);
        &mask_blurred_storage
    } else {
        mask_src
    };
    let mut height = vec![0.0f32; npx];
    let mut mask_a = vec![0.0f32; npx];
    let invert = rp.height_invert;
    for i in 0..npx {
        let off = i * 4;
        let a = mask_buf[off] as f32 / 255.0;
        let r = mask_buf[off + 1] as f32 / 255.0;
        let g = mask_buf[off + 2] as f32 / 255.0;
        let b = mask_buf[off + 3] as f32 / 255.0;
        let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;

        // Height source (see lib.rs popup order).
        let src_val = match rp.height_source {
            2 => luma,
            3 => a,
            4 => r,
            5 => g,
            6 => b,
            7 => r.max(g).max(b),
            _ => luma * a, // 1 = Luminance x Alpha (default)
        };
        height[i] = if invert { 1.0 - src_val } else { src_val };

        // Coverage source.
        mask_a[i] = match rp.coverage_source {
            2 => luma,
            3 => 1.0,
            4 => src_val,
            _ => a, // 1 = Alpha (default)
        }
        .clamp(0.0, 1.0);
    }

    // 1b) Glassmorphism edge: an inner distance band along the shape rim.
    //     Drives a height bevel (refraction concentrates at the rim), a rim
    //     highlight, and a localized frosted blur. Computed from the coverage
    //     mask so it follows the silhouette.
    let (glass_band, glass_dist) = if rp.edge_enable {
        let dist = inner_distance(&mask_a, w, h);
        let band = edge_band_from_dist(&dist, &mask_a, w, h, rp.edge_width, rp.edge_rim_blur);
        if rp.bevel_mode == BEVEL_SDF {
            // SDF Normal mode: the rim normal is synthesized from the distance
            // gradient. The raw EDT is quantized (staircase) and has gradient
            // creases along the medial axis, which make the normal — and thus the
            // refraction — jagged. Smooth the distance field first so the
            // gradient is clean. Always smooth a little, even at rim blur 0.
            let smooth = rp.edge_rim_blur.max(2.0);
            let dist_smooth = box_blur_3pass(&dist, w, h, smooth);
            (Some(band), Some(dist_smooth))
        } else {
            // Height-add mode: fold the bevel into the height field BEFORE the
            // normal pass so the rim tilts the surface.
            if rp.bevel_height.abs() > 1e-4 {
                for i in 0..npx {
                    height[i] += band[i] * rp.bevel_height;
                }
            }
            (Some(band), None)
        }
    } else {
        (None, None)
    };

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
    // SDF Normal bevel: synthesize the rim normal straight from the distance
    // field gradient. `dist` increases inward, so its gradient points inward —
    // tilting the normal inward at the rim (matching Height-add's sign) with a
    // round-over profile that's full at the silhouette and flat by `edge_width`.
    let sdf_dist = glass_dist.as_deref();
    let sdf_width = rp.edge_width.max(0.5);
    let sdf_amount = rp.bevel_height;
    let mut normals = vec![(0.0f32, 0.0f32, 1.0f32); npx];
    normals.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
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
            let mut nx = -dhdx * k;
            let mut ny = -dhdy * k;
            if let Some(dist) = sdf_dist {
                let ddx = (dist[rc + x1] - dist[rc + x0]) * 0.5;
                let ddy = (dist[r1 + x] - dist[r0 + x]) * 0.5;
                let gl = (ddx * ddx + ddy * ddy).sqrt();
                if gl > 1e-4 {
                    let t = (dist[rc + x] / sdf_width).clamp(0.0, 1.0);
                    // Round-over profile: full tilt at the edge, 0 in the interior.
                    let slope = (1.0 - t * t).max(0.0).sqrt();
                    let tilt = sdf_amount * slope;
                    nx += (ddx / gl) * tilt;
                    ny += (ddy / gl) * tilt;
                }
            }
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

    // Per-sample offset magnitude (×power) for each channel.
    //
    // The blog's loop fans every channel one-directionally outward with a
    // per-channel multiplier of 1/2/3 (r/g/b). That has two problems: the fan is
    // asymmetric (only grows outward, so the channels pile up on the near side)
    // and red gets the *smallest* fan, so it stays densest -> a concentrated red
    // pool that fades out ("red fade").
    //
    // Instead we:
    //   * center the multi-sample blur symmetrically around the base offset
    //     (`slide` in [-0.5, 0.5)) and give every channel the SAME blur width,
    //     so no channel ends up denser than another, and
    //   * separate the colors symmetrically around green (red pulled in, blue
    //     pushed out by an equal amount), so the dispersion is balanced instead
    //     of pooling on one side.
    // Color separation magnitude and blur width both scale with `chroma`.
    const SEP_SCALE: f32 = 0.1; // symmetric R<->B separation per chroma
    const BLUR_SCALE: f32 = 0.2; // shared per-channel blur width per chroma
    let sep = [-1.0f32, 0.0, 1.0]; // r, g, b around green
    let mut strength_r_x = vec![0.0f32; samples];
    let mut strength_g_x = vec![0.0f32; samples];
    let mut strength_b_x = vec![0.0f32; samples];
    let mut strength_r_y = vec![0.0f32; samples];
    let mut strength_g_y = vec![0.0f32; samples];
    let mut strength_b_y = vec![0.0f32; samples];
    for i in 0..samples {
        let slide = (i as f32 + 0.5) / samples as f32 - 0.5; // [-0.5, 0.5)
        let blur_x = slide * BLUR_SCALE * chroma_x;
        let blur_y = slide * BLUR_SCALE * chroma_y;
        strength_r_x[i] = power * (1.0 + sep[0] * SEP_SCALE * chroma_x + blur_x);
        strength_g_x[i] = power * (1.0 + sep[1] * SEP_SCALE * chroma_x + blur_x);
        strength_b_x[i] = power * (1.0 + sep[2] * SEP_SCALE * chroma_x + blur_x);
        strength_r_y[i] = power * (1.0 + sep[0] * SEP_SCALE * chroma_y + blur_y);
        strength_g_y[i] = power * (1.0 + sep[1] * SEP_SCALE * chroma_y + blur_y);
        strength_b_y[i] = power * (1.0 + sep[2] * SEP_SCALE * chroma_y + blur_y);
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
        // Same symmetric / equal-width scheme as the RGB path: separate the six
        // wavelength slots symmetrically (−1..+1 around the center) with a shared
        // blur width, so no slot pools denser than the others.
        const SEP_SCALE_6: f32 = 0.15;
        const BLUR_SCALE_6: f32 = 0.2;
        for ch in 0..6 {
            let sep = ch as f32 / 2.5 - 1.0; // -1, -0.6, -0.2, 0.2, 0.6, 1.0
            for i in 0..samples {
                let slide = (i as f32 + 0.5) / samples as f32 - 0.5; // [-0.5, 0.5)
                strengths_6_x[ch][i] =
                    power * (1.0 + sep * SEP_SCALE_6 * chroma_x + slide * BLUR_SCALE_6 * chroma_x);
                strengths_6_y[ch][i] =
                    power * (1.0 + sep * SEP_SCALE_6 * chroma_y + slide * BLUR_SCALE_6 * chroma_y);
            }
        }
    }

    // Precompute IOR ratios eta = 1/IOR.
    let eta_r = 1.0 / rp.ior_r.max(1.0);
    let eta_g = 1.0 / rp.ior_g.max(1.0);
    let eta_b = 1.0 / rp.ior_b.max(1.0);
    let eta_base = 1.0 / rp.base_ior.max(1.0);
    let etas_6 = [
        1.0 / iors_6[0].max(1.0),
        1.0 / iors_6[1].max(1.0),
        1.0 / iors_6[2].max(1.0),
        1.0 / iors_6[3].max(1.0),
        1.0 / iors_6[4].max(1.0),
        1.0 / iors_6[5].max(1.0),
    ];

    // 6) Main loop — parallelized per row.
    let mut out = vec![0u8; npx * 4];
    let mix = rp.mix;
    let inv_mix = 1.0 - mix;
    let brightness = rp.brightness;
    let contrast = rp.contrast;

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
    let glass_band_ref = glass_band.as_deref();
    let rim_highlight = rp.rim_highlight;

    out.par_chunks_mut(w * 4)
        .enumerate()
        .for_each(|(y, out_row)| {
            for x in 0..w {
                let idx = y * w + x;
                let off = x * 4;
                let src_off = idx * 4;

                let src_a = src_ref[src_off];
                let src_r = src_ref[src_off + 1] as f32;
                let src_g = src_ref[src_off + 2] as f32;
                let src_b = src_ref[src_off + 3] as f32;
                let fallback_rgb = (src_r, src_g, src_b);

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
                if rp.ior_mode == 2 {
                    let (rvx, rvy, _) = refract(ex, ey, ez, nx, ny, nz, eta_base);
                    let mut ar = 0.0f32;
                    let mut ag = 0.0f32;
                    let mut ab = 0.0f32;
                    for _ in 0..samples {
                        let sx = x as f32 + rvx * power;
                        let sy = y as f32 + rvy * power;
                        if rp.edge_mode == EDGE_FADE {
                            let t = (outside_dist(w, h, sx, sy) / EDGE_FADE_RAMP).clamp(0.0, 1.0);
                            let it = 1.0 - t;
                            let (r_px, g_px, b_px) = sample_bilinear_rgb(bg_ref, w, h, sx, sy);
                            ar += r_px * it + fallback_rgb.0 * t;
                            ag += g_px * it + fallback_rgb.1 * t;
                            ab += b_px * it + fallback_rgb.2 * t;
                        } else {
                            let (r_px, g_px, b_px) =
                                sample_edge(bg_ref, w, h, sx, sy, rp.edge_mode);
                            ar += r_px;
                            ag += g_px;
                            ab += b_px;
                        }
                    }
                    let inv_s = 1.0 / samples as f32;
                    acc_r = ar * inv_s;
                    acc_g = ag * inv_s;
                    acc_b = ab * inv_s;
                } else if !rp.use_6ch {
                    let (rvrx, rvry, _) = refract(ex, ey, ez, nx, ny, nz, eta_r);
                    let (rvgx, rvgy, _) = refract(ex, ey, ez, nx, ny, nz, eta_g);
                    let (rvbx, rvby, _) = refract(ex, ey, ez, nx, ny, nz, eta_b);

                    let mut ar = 0.0f32;
                    let mut ag = 0.0f32;
                    let mut ab = 0.0f32;
                    for i in 0..samples {
                        let sr_x = x as f32 + rvrx * strength_r_x[i];
                        let sr_y = y as f32 + rvry * strength_r_y[i];
                        let sg_x = x as f32 + rvgx * strength_g_x[i];
                        let sg_y = y as f32 + rvgy * strength_g_y[i];
                        let sb_x = x as f32 + rvbx * strength_b_x[i];
                        let sb_y = y as f32 + rvby * strength_b_y[i];
                        if rp.edge_mode == EDGE_FADE {
                            // Shared fade factor from the tap that reaches
                            // furthest outside, so the R/G/B channels stay
                            // coupled (no colored seam) while the stretched edge
                            // dissolves back into the source image.
                            let d = outside_dist(w, h, sr_x, sr_y)
                                .max(outside_dist(w, h, sg_x, sg_y))
                                .max(outside_dist(w, h, sb_x, sb_y));
                            let t = (d / EDGE_FADE_RAMP).clamp(0.0, 1.0);
                            let it = 1.0 - t;
                            let (r_px, _, _) = sample_bilinear_rgb(bg_ref, w, h, sr_x, sr_y);
                            let (_, g_px, _) = sample_bilinear_rgb(bg_ref, w, h, sg_x, sg_y);
                            let (_, _, b_px) = sample_bilinear_rgb(bg_ref, w, h, sb_x, sb_y);
                            ar += r_px * it + fallback_rgb.0 * t;
                            ag += g_px * it + fallback_rgb.1 * t;
                            ab += b_px * it + fallback_rgb.2 * t;
                        } else {
                            // Mirror / Clamp: each tap returns real data, so the
                            // channels can be sampled independently.
                            let (r_px, _, _) = sample_edge(bg_ref, w, h, sr_x, sr_y, rp.edge_mode);
                            let (_, g_px, _) = sample_edge(bg_ref, w, h, sg_x, sg_y, rp.edge_mode);
                            let (_, _, b_px) = sample_edge(bg_ref, w, h, sb_x, sb_y, rp.edge_mode);
                            ar += r_px;
                            ag += g_px;
                            ab += b_px;
                        }
                    }
                    let inv_s = 1.0 / samples as f32;
                    acc_r = ar * inv_s;
                    acc_g = ag * inv_s;
                    acc_b = ab * inv_s;
                } else {
                    let inv_s = 1.0 / samples as f32;
                    // Precompute the refraction vector for each wavelength slot.
                    let mut rvs = [(0.0f32, 0.0f32); 6];
                    for slot in 0..6 {
                        let (rvx, rvy, _) = refract(ex, ey, ez, nx, ny, nz, etas_6_ref[slot]);
                        rvs[slot] = (rvx, rvy);
                    }
                    // Accumulate per slot. In Clamp+Fade a single shared fade
                    // factor (from the slot reaching furthest outside) keeps all
                    // wavelengths in lock-step so no colored seam forms.
                    let mut acc_s = [(0.0f32, 0.0f32, 0.0f32); 6];
                    for i in 0..samples {
                        let mut pos = [(0.0f32, 0.0f32); 6];
                        for slot in 0..6 {
                            let sx = strengths_6_x_ref[slot][i];
                            let sy = strengths_6_y_ref[slot][i];
                            pos[slot] = (x as f32 + rvs[slot].0 * sx, y as f32 + rvs[slot].1 * sy);
                        }
                        if rp.edge_mode == EDGE_FADE {
                            let mut d = 0.0f32;
                            for slot in 0..6 {
                                d = d.max(outside_dist(w, h, pos[slot].0, pos[slot].1));
                            }
                            let t = (d / EDGE_FADE_RAMP).clamp(0.0, 1.0);
                            let it = 1.0 - t;
                            for slot in 0..6 {
                                let (sr, sg, sb) =
                                    sample_bilinear_rgb(bg_ref, w, h, pos[slot].0, pos[slot].1);
                                acc_s[slot].0 += sr * it + fallback_rgb.0 * t;
                                acc_s[slot].1 += sg * it + fallback_rgb.1 * t;
                                acc_s[slot].2 += sb * it + fallback_rgb.2 * t;
                            }
                        } else {
                            for slot in 0..6 {
                                let (sr, sg, sb) = sample_edge(
                                    bg_ref,
                                    w,
                                    h,
                                    pos[slot].0,
                                    pos[slot].1,
                                    rp.edge_mode,
                                );
                                acc_s[slot].0 += sr;
                                acc_s[slot].1 += sg;
                                acc_s[slot].2 += sb;
                            }
                        }
                    }
                    let mut ch = [0.0f32; 6];
                    for slot in 0..6 {
                        let acc_sr = acc_s[slot].0 * inv_s;
                        let acc_sg = acc_s[slot].1 * inv_s;
                        let acc_sb = acc_s[slot].2 * inv_s;
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
                let fresnel = (1.0 - fresnel_factor)
                    .max(0.0)
                    .powf(rp.fresnel_power.max(0.0));

                let diff_mult = 1.0 + k_diff;
                let spec_add = k_spec * fresnel * 200.0;
                acc_r = acc_r * diff_mult + spec_add;
                acc_g = acc_g * diff_mult + spec_add;
                acc_b = acc_b * diff_mult + spec_add;

                // ---- Glass rim highlight ----
                if let Some(band) = glass_band_ref {
                    if rim_highlight.abs() > 1e-4 {
                        let rim = band[idx] * rim_highlight;
                        acc_r += rim;
                        acc_g += rim;
                        acc_b += rim;
                    }
                }

                // ---- Affected color correction ----
                acc_r = apply_color_adjust(acc_r, brightness, contrast);
                acc_g = apply_color_adjust(acc_g, brightness, contrast);
                acc_b = apply_color_adjust(acc_b, brightness, contrast);

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

    apply_affected_blur(&mut out, w, h, rp.affected_blur, &composite_mask);

    // Localized frosted blur on the rim band (glassmorphism edge).
    if let Some(band) = glass_band.as_ref() {
        apply_affected_blur(&mut out, w, h, rp.rim_frost, band);
    }

    match rp.output_mode {
        2 => src.to_vec(),
        3 => mask_map(src, &composite_mask, w, h),
        4 => delta_map(src, &out, w, h),
        5 => debug_regions(src, &composite_mask, w, h),
        _ => out,
    }
}
