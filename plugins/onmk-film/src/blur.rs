//! Separable Gaussian blur and thresholded glow helpers (CPU).

fn box_blur_pass(src: &[f32], dst: &mut [f32], w: usize, h: usize, radius: usize) {
    let diameter = (radius * 2 + 1) as f32;
    let mut tmp = vec![0.0f32; src.len()];
    let mut prefix = vec![[0.0f32; 3]; w.max(h) + 1];

    for y in 0..h {
        prefix[0] = [0.0; 3];
        for x in 0..w {
            let o = (y * w + x) * 4;
            for c in 0..3 {
                prefix[x + 1][c] = prefix[x][c] + src[o + c];
            }
        }
        for x in 0..w {
            let lo = x.saturating_sub(radius);
            let hi = (x + radius + 1).min(w);
            let left = radius.saturating_sub(x);
            let right = (x + radius + 1).saturating_sub(w);
            let o = (y * w + x) * 4;
            for c in 0..3 {
                let sum = prefix[hi][c] - prefix[lo][c]
                    + src[y * w * 4 + c] * left as f32
                    + src[(y * w + w - 1) * 4 + c] * right as f32;
                tmp[o + c] = sum / diameter;
            }
            tmp[o + 3] = src[o + 3];
        }
    }

    for x in 0..w {
        prefix[0] = [0.0; 3];
        for y in 0..h {
            let o = (y * w + x) * 4;
            for c in 0..3 {
                prefix[y + 1][c] = prefix[y][c] + tmp[o + c];
            }
        }
        for y in 0..h {
            let lo = y.saturating_sub(radius);
            let hi = (y + radius + 1).min(h);
            let top = radius.saturating_sub(y);
            let bottom = (y + radius + 1).saturating_sub(h);
            let o = (y * w + x) * 4;
            for c in 0..3 {
                let sum = prefix[hi][c] - prefix[lo][c]
                    + tmp[x * 4 + c] * top as f32
                    + tmp[((h - 1) * w + x) * 4 + c] * bottom as f32;
                dst[o + c] = sum / diameter;
            }
            dst[o + 3] = src[o + 3];
        }
    }
}

/// Blur RGB (alpha copied). `src`/`dst` are RGBA f32 interleaved.
pub fn gaussian_blur_rgba(
    src: &[f32],
    dst: &mut [f32],
    w: usize,
    h: usize,
    radius_px: f32,
    quality: f32,
) {
    assert_eq!(src.len(), w * h * 4);
    assert_eq!(dst.len(), w * h * 4);
    if radius_px < 0.25 || w == 0 || h == 0 {
        dst.copy_from_slice(src);
        return;
    }
    // Three linear-time box passes approximate a Gaussian without cost growing
    // with the radius. The previous convolution made full-frame adjustment
    // layers appear hung at 1080p/4K in debug builds.
    let effective = radius_px * quality.clamp(0.25, 1.0);
    let box_radius = (effective * 0.5).round().max(1.0) as usize;
    let mut a = vec![0.0f32; src.len()];
    let mut b = vec![0.0f32; src.len()];
    box_blur_pass(src, &mut a, w, h, box_radius);
    box_blur_pass(&a, &mut b, w, h, box_radius);
    box_blur_pass(&b, dst, w, h, box_radius);
}

/// Extract highlights above threshold (soft knee).
pub fn extract_highlights(src: &[f32], dst: &mut [f32], w: usize, h: usize, threshold: f32) {
    let thr = threshold.max(0.0);
    for i in 0..(w * h) {
        let o = i * 4;
        let y = 0.2126 * src[o] + 0.7152 * src[o + 1] + 0.0722 * src[o + 2];
        let m = ((y - thr) / (thr * 0.5 + 1.0e-3)).clamp(0.0, 1.0);
        let m = m * m * (3.0 - 2.0 * m);
        dst[o] = src[o] * m;
        dst[o + 1] = src[o + 1] * m;
        dst[o + 2] = src[o + 2] * m;
        dst[o + 3] = src[o + 3];
    }
}

pub fn add_tinted(
    base: &mut [f32],
    glow: &[f32],
    color: [f32; 3],
    strength: f32,
    w: usize,
    h: usize,
) {
    if strength <= 1.0e-5 {
        return;
    }
    for i in 0..(w * h) {
        let o = i * 4;
        base[o] += glow[o] * color[0] * strength;
        base[o + 1] += glow[o + 1] * color[1] * strength;
        base[o + 2] += glow[o + 2] * color[2] * strength;
    }
}

pub fn mix_add(base: &mut [f32], layer: &[f32], strength: f32, w: usize, h: usize) {
    if strength <= 1.0e-5 {
        return;
    }
    for i in 0..(w * h) {
        let o = i * 4;
        base[o] += layer[o] * strength;
        base[o + 1] += layer[o + 1] * strength;
        base[o + 2] += layer[o + 2] * strength;
    }
}

/// Unsharp / acutance: base + amount * (base - blur)
pub fn acutance_sharpen(
    src: &[f32],
    blurred: &[f32],
    dst: &mut [f32],
    amount: f32,
    w: usize,
    h: usize,
) {
    for i in 0..(w * h) {
        let o = i * 4;
        for c in 0..3 {
            let d = src[o + c] - blurred[o + c];
            dst[o + c] = (src[o + c] + d * amount).max(0.0);
        }
        dst[o + 3] = src[o + 3];
    }
}

#[allow(dead_code)]
pub fn lerp_buffers(a: &[f32], b: &[f32], t: f32, dst: &mut [f32]) {
    let t = t.clamp(0.0, 1.0);
    for i in 0..dst.len() {
        dst[i] = a[i] * (1.0 - t) + b[i] * t;
    }
}
