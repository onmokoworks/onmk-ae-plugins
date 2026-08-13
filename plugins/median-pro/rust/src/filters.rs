/// CPU filter implementations for MedianPro.
/// All filters operate on flat &[u8] pixel slices (ARGB 8-bit, 4 bytes per pixel).
/// When a luminance map is provided, the effective radius is scaled per-pixel:
/// effective_radius = round(radius * luma).

#[inline]
fn clamp8(v: i32) -> u8 {
    v.clamp(0, 255) as u8
}

#[inline]
fn lerp8(a: u8, b: u8, t: f64) -> u8 {
    clamp8((a as f64 * (1.0 - t) + b as f64 * t + 0.5) as i32)
}

#[inline]
fn read_pixel(buf: &[u8], w: usize, h: usize, x: i32, y: i32) -> (u8, u8, u8, u8) {
    let cx = x.clamp(0, w as i32 - 1) as usize;
    let cy = y.clamp(0, h as i32 - 1) as usize;
    let off = (cy * w + cx) * 4;
    (buf[off], buf[off + 1], buf[off + 2], buf[off + 3])
}

#[inline]
fn write_pixel(buf: &mut [u8], w: usize, x: usize, y: usize, a: u8, r: u8, g: u8, b: u8) {
    let off = (y * w + x) * 4;
    buf[off] = a;
    buf[off + 1] = r;
    buf[off + 2] = g;
    buf[off + 3] = b;
}

#[inline]
fn effective_radius(base_radius: usize, luma_map: Option<&[f64]>, px_index: usize) -> usize {
    match luma_map {
        None => base_radius,
        Some(map) => {
            let luma = map.get(px_index).copied().unwrap_or(1.0);
            (base_radius as f64 * luma).round().max(0.0) as usize
        }
    }
}

pub fn filter_median(
    src: &[u8],
    dst: &mut [u8],
    w: usize,
    h: usize,
    radius: usize,
    luma_map: Option<&[f64]>,
) {
    if luma_map.is_none() {
        filter_median_sliding(src, dst, w, h, radius);
    } else {
        filter_median_perpixel(src, dst, w, h, radius, luma_map);
    }
}

fn filter_median_sliding(src: &[u8], dst: &mut [u8], w: usize, h: usize, radius: usize) {
    let mut hist_r = [0i32; 256];
    let mut hist_g = [0i32; 256];
    let mut hist_b = [0i32; 256];
    let r = radius as i32;

    let find_median = |hist: &[i32; 256], mid: i32| -> u8 {
        let mut cum = 0i32;
        for (i, count) in hist.iter().enumerate() {
            cum += count;
            if cum > mid {
                return i as u8;
            }
        }
        255
    };

    for y in 0..h as i32 {
        hist_r.fill(0);
        hist_g.fill(0);
        hist_b.fill(0);
        let mut total_alpha = 0i32;

        for ky in -r..=r {
            for kx in -r..=r {
                let (pa, pr, pg, pb) = read_pixel(src, w, h, kx, y + ky);
                let weight = pa as i32;
                hist_r[pr as usize] += weight;
                hist_g[pg as usize] += weight;
                hist_b[pb as usize] += weight;
                total_alpha += weight;
            }
        }

        let (a, center_r, center_g, center_b) = read_pixel(src, w, h, 0, y);
        let mid = total_alpha / 2;
        write_pixel(
            dst,
            w,
            0,
            y as usize,
            a,
            if total_alpha == 0 {
                center_r
            } else {
                find_median(&hist_r, mid)
            },
            if total_alpha == 0 {
                center_g
            } else {
                find_median(&hist_g, mid)
            },
            if total_alpha == 0 {
                center_b
            } else {
                find_median(&hist_b, mid)
            },
        );

        for x in 1..w as i32 {
            let rem_x = x - r - 1;
            let add_x = x + r;

            for ky in -r..=r {
                let (ra, rr, rg, rb) = read_pixel(src, w, h, rem_x, y + ky);
                let remove_weight = ra as i32;
                hist_r[rr as usize] -= remove_weight;
                hist_g[rg as usize] -= remove_weight;
                hist_b[rb as usize] -= remove_weight;
                total_alpha -= remove_weight;

                let (aa, ar, ag, ab) = read_pixel(src, w, h, add_x, y + ky);
                let add_weight = aa as i32;
                hist_r[ar as usize] += add_weight;
                hist_g[ag as usize] += add_weight;
                hist_b[ab as usize] += add_weight;
                total_alpha += add_weight;
            }

            let (a, center_r, center_g, center_b) = read_pixel(src, w, h, x, y);
            let mid = total_alpha / 2;
            write_pixel(
                dst,
                w,
                x as usize,
                y as usize,
                a,
                if total_alpha == 0 {
                    center_r
                } else {
                    find_median(&hist_r, mid)
                },
                if total_alpha == 0 {
                    center_g
                } else {
                    find_median(&hist_g, mid)
                },
                if total_alpha == 0 {
                    center_b
                } else {
                    find_median(&hist_b, mid)
                },
            );
        }
    }
}

fn filter_median_perpixel(
    src: &[u8],
    dst: &mut [u8],
    w: usize,
    h: usize,
    radius: usize,
    luma_map: Option<&[f64]>,
) {
    let find_median = |hist: &[i32; 256], mid: i32| -> u8 {
        let mut cum = 0i32;
        for (i, count) in hist.iter().enumerate() {
            cum += count;
            if cum > mid {
                return i as u8;
            }
        }
        255
    };

    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let px_idx = y as usize * w + x as usize;
            let er = effective_radius(radius, luma_map, px_idx) as i32;

            let mut hist_r = [0i32; 256];
            let mut hist_g = [0i32; 256];
            let mut hist_b = [0i32; 256];
            let mut total_alpha = 0i32;

            for ky in -er..=er {
                for kx in -er..=er {
                    let (pa, pr, pg, pb) = read_pixel(src, w, h, x + kx, y + ky);
                    let weight = pa as i32;
                    hist_r[pr as usize] += weight;
                    hist_g[pg as usize] += weight;
                    hist_b[pb as usize] += weight;
                    total_alpha += weight;
                }
            }

            let (a, center_r, center_g, center_b) = read_pixel(src, w, h, x, y);
            let mid = total_alpha / 2;
            write_pixel(
                dst,
                w,
                x as usize,
                y as usize,
                a,
                if total_alpha == 0 {
                    center_r
                } else {
                    find_median(&hist_r, mid)
                },
                if total_alpha == 0 {
                    center_g
                } else {
                    find_median(&hist_g, mid)
                },
                if total_alpha == 0 {
                    center_b
                } else {
                    find_median(&hist_b, mid)
                },
            );
        }
    }
}

pub fn filter_weighted_median(
    src: &[u8],
    dst: &mut [u8],
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

            let sigma = (er as f64 * (edge_param / 100.0 * 1.5 + 0.3)).max(0.5);
            let inv_sigma2 = 1.0 / (2.0 * sigma * sigma);

            let mut hist_r = [0.0f64; 256];
            let mut hist_g = [0.0f64; 256];
            let mut hist_b = [0.0f64; 256];
            let mut total_w = 0.0f64;

            for ky in -r..=r {
                for kx in -r..=r {
                    let spatial_weight = (-((kx * kx + ky * ky) as f64 * inv_sigma2)).exp();
                    let (pa, pr, pg, pb) = read_pixel(src, w, h, x + kx, y + ky);
                    let wt = spatial_weight * (pa as f64 / 255.0);
                    hist_r[pr as usize] += wt;
                    hist_g[pg as usize] += wt;
                    hist_b[pb as usize] += wt;
                    total_w += wt;
                }
            }

            let half = total_w * 0.5;
            let find_wmedian = |hist: &[f64; 256]| -> u8 {
                let mut cum = 0.0;
                for (i, weight) in hist.iter().enumerate() {
                    cum += weight;
                    if cum >= half {
                        return i as u8;
                    }
                }
                255
            };

            let (a, center_r, center_g, center_b) = read_pixel(src, w, h, x, y);
            write_pixel(
                dst,
                w,
                x as usize,
                y as usize,
                a,
                if total_w <= f64::EPSILON {
                    center_r
                } else {
                    find_wmedian(&hist_r)
                },
                if total_w <= f64::EPSILON {
                    center_g
                } else {
                    find_wmedian(&hist_g)
                },
                if total_w <= f64::EPSILON {
                    center_b
                } else {
                    find_wmedian(&hist_b)
                },
            );
        }
    }
}

pub fn dispatch_filter(
    filter_type: i32,
    src: &[u8],
    dst: &mut [u8],
    w: usize,
    h: usize,
    radius: usize,
    edge_p: f64,
    luma_map: Option<&[f64]>,
) {
    match filter_type {
        1 => filter_median(src, dst, w, h, radius, luma_map),
        2 => filter_weighted_median(src, dst, w, h, radius, edge_p, luma_map),
        _ => filter_median(src, dst, w, h, radius, luma_map),
    }
}

pub fn mix_buffers(original: &[u8], filtered: &[u8], output: &mut [u8], mix: f64) {
    if mix >= 1.0 - 1e-6 {
        output.copy_from_slice(filtered);
    } else {
        for i in (0..original.len()).step_by(4) {
            output[i] = original[i];
            output[i + 1] = lerp8(original[i + 1], filtered[i + 1], mix);
            output[i + 2] = lerp8(original[i + 2], filtered[i + 2], mix);
            output[i + 3] = lerp8(original[i + 3], filtered[i + 3], mix);
        }
    }
}
