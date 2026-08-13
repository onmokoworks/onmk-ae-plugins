//! Squared Euclidean distance transform (Felzenszwalb & Huttenlocher), separable 1D passes.

const INF: f32 = 1e20;

fn edt_1d_squared(f: &mut [f32], v: &mut [i32], z: &mut [f32]) {
    let n = f.len();
    if n == 0 {
        return;
    }
    let mut k = 0;
    v[0] = 0;
    z[0] = -INF;
    z[1] = INF;

    for q in 1..n {
        let fq = f[q];
        let mut s = ((fq + (q * q) as f32) - (f[v[k] as usize] + (v[k] * v[k]) as f32))
            / (2.0 * q as f32 - 2.0 * v[k] as f32);
        while s <= z[k] {
            k -= 1;
            s = ((fq + (q * q) as f32) - (f[v[k] as usize] + (v[k] * v[k]) as f32))
                / (2.0 * q as f32 - 2.0 * v[k] as f32);
        }
        k += 1;
        v[k] = q as i32;
        z[k] = s;
        z[k + 1] = INF;
    }

    k = 0;
    for q in 0..n {
        while z[k + 1] < q as f32 {
            k += 1;
        }
        let d = q as f32 - v[k] as f32;
        f[q] = d * d + f[v[k] as usize];
    }
}

/// `mask` values > 0.5 are foreground seeds. Writes Euclidean distance (pixels) into `dist`.
pub fn euclidean_dt(mask: &[f32], width: usize, height: usize, dist: &mut [f32]) {
    assert_eq!(mask.len(), width * height);
    assert_eq!(dist.len(), mask.len());
    if width == 0 || height == 0 {
        return;
    }

    let m = width.max(height);
    let mut horiz = vec![0.0f32; width * height];
    let mut v = vec![0i32; m];
    let mut z = vec![0.0f32; m + 1];
    let mut col = vec![0.0f32; height];

    for y in 0..height {
        let row = &mut horiz[y * width..(y + 1) * width];
        for x in 0..width {
            let msk = mask[y * width + x];
            row[x] = if msk > 0.5 { 0.0 } else { INF };
        }
        edt_1d_squared(row, &mut v[..width], &mut z[..width + 1]);
    }

    for x in 0..width {
        for y in 0..height {
            col[y] = horiz[y * width + x];
        }
        edt_1d_squared(&mut col, &mut v[..height], &mut z[..height + 1]);
        for y in 0..height {
            let d2 = col[y];
            dist[y * width + x] = if d2 >= INF * 0.5 {
                INF
            } else {
                d2.max(0.0).sqrt()
            };
        }
    }
}
