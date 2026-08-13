pub const IMAGE_NET_MEAN: [f32; 3] = [0.485, 0.456, 0.406];
pub const IMAGE_NET_STD: [f32; 3] = [0.229, 0.224, 0.225];

/// Resize ARGB float buffer to NCHW ImageNet-normalized tensor.
pub fn preprocess_to_nchw(
    src: &[f32],
    src_width: i32,
    src_height: i32,
    src_is_argb: bool,
    size: i32,
) -> Vec<f32> {
    let sw = src_width.max(1);
    let sh = src_height.max(1);
    let size = size.max(1);
    let plane = (size * size) as usize;
    let mut dst = vec![0.0f32; 3 * plane];

    let (r_off, g_off, b_off) = if src_is_argb { (1, 2, 3) } else { (0, 1, 2) };
    let xs = sw as f32 / size as f32;
    let ys = sh as f32 / size as f32;

    for y in 0..size {
        let fy = (y as f32 + 0.5) * ys - 0.5;
        let y0 = fy.floor().clamp(0.0, (sh - 1) as f32) as i32;
        let y1 = (y0 + 1).min(sh - 1);
        let wy = fy - fy.floor();

        for x in 0..size {
            let fx = (x as f32 + 0.5) * xs - 0.5;
            let x0 = fx.floor().clamp(0.0, (sw - 1) as f32) as i32;
            let x1 = (x0 + 1).min(sw - 1);
            let wx = fx - fx.floor();

            let idx = |row: i32, col: i32| -> usize { (row * sw + col) as usize * 4 };

            for c in 0..3 {
                let off = match c {
                    0 => r_off,
                    1 => g_off,
                    _ => b_off,
                };
                let p00 = src[idx(y0, x0) + off];
                let p01 = src[idx(y0, x1) + off];
                let p10 = src[idx(y1, x0) + off];
                let p11 = src[idx(y1, x1) + off];
                let v0 = p00 * (1.0 - wx) + p01 * wx;
                let v1 = p10 * (1.0 - wx) + p11 * wx;
                let v = v0 * (1.0 - wy) + v1 * wy;
                let n = (v.clamp(0.0, 1.0) - IMAGE_NET_MEAN[c as usize])
                    / IMAGE_NET_STD[c as usize];
                dst[c as usize * plane + y as usize * size as usize + x as usize] = n;
            }
        }
    }

    dst
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_shape_is_nchw() {
        let src = vec![1.0f32; 4 * 4 * 4];
        let out = preprocess_to_nchw(&src, 2, 2, true, 266);
        assert_eq!(out.len(), 3 * 266 * 266);
    }
}
