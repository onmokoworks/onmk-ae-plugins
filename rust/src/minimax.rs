/// Signed linear Minimax with optional per-pixel radius from a luminance map.
///
/// Negative Amount runs Minimum, positive Amount runs Maximum, and 0.0 is a
/// pass-through. Fractional radii are blended between floor(radius) and
/// ceil(radius), so small values such as 0.01 produce smooth changes.
///
/// Pixel format: flat ARGB u8, 4 bytes per pixel.
use crate::MinimaxParams;

const DIR_HORIZONTAL: i32 = 1;
const DIR_VERTICAL: i32 = 2;
const DIR_BOTH: i32 = 3;

const CH_COLOR: i32 = 1;
const CH_ALPHA: i32 = 2;
const CH_COLOR_ALPHA: i32 = 3;

pub fn minimax(
    mp: &MinimaxParams,
    src: &[u8],
    w: usize,
    h: usize,
    luma_map: Option<&[f64]>,
) -> Vec<u8> {
    let npx = w * h;
    if mp.amount.abs() < f32::EPSILON || w == 0 || h == 0 {
        return src.to_vec();
    }

    let is_min = mp.amount < 0.0;
    let radius = mp.amount.abs();
    let radius_h = radius * mp.ratio_x.max(0.0);
    let radius_v = radius * mp.ratio_y.max(0.0);

    let do_alpha = mp.channel == CH_ALPHA || mp.channel == CH_COLOR_ALPHA;
    let do_color = mp.channel == CH_COLOR || mp.channel == CH_COLOR_ALPHA;

    let mut ch_a = vec![0u8; npx];
    let mut ch_r = vec![0u8; npx];
    let mut ch_g = vec![0u8; npx];
    let mut ch_b = vec![0u8; npx];

    for i in 0..npx {
        let off = i * 4;
        ch_a[i] = src[off];
        ch_r[i] = src[off + 1];
        ch_g[i] = src[off + 2];
        ch_b[i] = src[off + 3];
    }

    let do_h = (mp.direction == DIR_HORIZONTAL || mp.direction == DIR_BOTH) && radius_h > 0.0;
    let do_v = (mp.direction == DIR_VERTICAL || mp.direction == DIR_BOTH) && radius_v > 0.0;
    let repeat = mp.repeat_edge;

    if do_color {
        if do_h {
            ch_r = minimax_1d_h_linear(&ch_r, w, h, radius_h, is_min, luma_map, repeat);
            ch_g = minimax_1d_h_linear(&ch_g, w, h, radius_h, is_min, luma_map, repeat);
            ch_b = minimax_1d_h_linear(&ch_b, w, h, radius_h, is_min, luma_map, repeat);
        }
        if do_v {
            ch_r = minimax_1d_v_linear(&ch_r, w, h, radius_v, is_min, luma_map, repeat);
            ch_g = minimax_1d_v_linear(&ch_g, w, h, radius_v, is_min, luma_map, repeat);
            ch_b = minimax_1d_v_linear(&ch_b, w, h, radius_v, is_min, luma_map, repeat);
        }
    }

    if do_alpha {
        if do_h {
            ch_a = minimax_1d_h_linear(&ch_a, w, h, radius_h, is_min, luma_map, repeat);
        }
        if do_v {
            ch_a = minimax_1d_v_linear(&ch_a, w, h, radius_v, is_min, luma_map, repeat);
        }
    }

    let mix = mp.mix;

    let mut out = vec![0u8; npx * 4];
    for i in 0..npx {
        let off = i * 4;
        out[off] = lerp(src[off], ch_a[i], mix);
        out[off + 1] = lerp(src[off + 1], ch_r[i], mix);
        out[off + 2] = lerp(src[off + 2], ch_g[i], mix);
        out[off + 3] = lerp(src[off + 3], ch_b[i], mix);
    }

    out
}

fn minimax_1d_h_linear(
    ch: &[u8],
    w: usize,
    h: usize,
    base_radius: f32,
    is_min: bool,
    luma_map: Option<&[f64]>,
    repeat_edge: bool,
) -> Vec<u8> {
    match luma_map {
        Some(map) => minimax_1d_h_variable(ch, w, h, base_radius, is_min, map, repeat_edge),
        None => minimax_1d_uniform_linear(ch, w, h, base_radius, is_min, repeat_edge, true),
    }
}

fn minimax_1d_v_linear(
    ch: &[u8],
    w: usize,
    h: usize,
    base_radius: f32,
    is_min: bool,
    luma_map: Option<&[f64]>,
    repeat_edge: bool,
) -> Vec<u8> {
    match luma_map {
        Some(map) => minimax_1d_v_variable(ch, w, h, base_radius, is_min, map, repeat_edge),
        None => minimax_1d_uniform_linear(ch, w, h, base_radius, is_min, repeat_edge, false),
    }
}

fn minimax_1d_uniform_linear(
    ch: &[u8],
    w: usize,
    h: usize,
    radius: f32,
    is_min: bool,
    repeat_edge: bool,
    horizontal: bool,
) -> Vec<u8> {
    let r0 = radius.floor() as usize;
    let r1 = radius.ceil() as usize;
    let t = radius - r0 as f32;

    if r1 == 0 {
        return ch.to_vec();
    }

    let high = if horizontal {
        minimax_1d_h(ch, w, h, r1, is_min, None, repeat_edge)
    } else {
        minimax_1d_v(ch, w, h, r1, is_min, None, repeat_edge)
    };

    if t <= f32::EPSILON {
        return if r0 == r1 {
            high
        } else if horizontal {
            minimax_1d_h(ch, w, h, r0, is_min, None, repeat_edge)
        } else {
            minimax_1d_v(ch, w, h, r0, is_min, None, repeat_edge)
        };
    }

    let low = if r0 == 0 {
        ch.to_vec()
    } else if horizontal {
        minimax_1d_h(ch, w, h, r0, is_min, None, repeat_edge)
    } else {
        minimax_1d_v(ch, w, h, r0, is_min, None, repeat_edge)
    };

    blend_channels(&low, &high, t)
}

fn minimax_1d_h_variable(
    ch: &[u8],
    w: usize,
    h: usize,
    base_radius: f32,
    is_min: bool,
    luma_map: &[f64],
    repeat_edge: bool,
) -> Vec<u8> {
    let mut out = vec![0u8; w * h];

    for y in 0..h {
        let row = y * w;
        for x in 0..w {
            let idx = row + x;
            let radius = base_radius * luma_map.get(idx).copied().unwrap_or(1.0) as f32;
            out[idx] = sample_linear_radius(ch[idx], radius, |r| {
                sample_h(ch, w, h, x, y, r, is_min, repeat_edge)
            });
        }
    }

    out
}

fn minimax_1d_v_variable(
    ch: &[u8],
    w: usize,
    h: usize,
    base_radius: f32,
    is_min: bool,
    luma_map: &[f64],
    repeat_edge: bool,
) -> Vec<u8> {
    let mut out = vec![0u8; w * h];

    for x in 0..w {
        for y in 0..h {
            let idx = y * w + x;
            let radius = base_radius * luma_map.get(idx).copied().unwrap_or(1.0) as f32;
            out[idx] = sample_linear_radius(ch[idx], radius, |r| {
                sample_v(ch, w, h, x, y, r, is_min, repeat_edge)
            });
        }
    }

    out
}

fn sample_linear_radius<F>(original: u8, radius: f32, mut sample: F) -> u8
where
    F: FnMut(usize) -> u8,
{
    if radius <= f32::EPSILON {
        return original;
    }

    let r0 = radius.floor() as usize;
    let r1 = radius.ceil() as usize;
    let t = radius - r0 as f32;

    if r0 == r1 {
        return sample(r0);
    }

    let low = if r0 == 0 { original } else { sample(r0) };
    let high = sample(r1);
    lerp(low, high, t)
}

fn sample_h(
    ch: &[u8],
    w: usize,
    _h: usize,
    x: usize,
    y: usize,
    r: usize,
    is_min: bool,
    repeat_edge: bool,
) -> u8 {
    if r == 0 {
        return ch[y * w + x];
    }

    let row = y * w;
    let mut value = neutral_value(is_min);

    for di in 0..=(2 * r) {
        let xi = x as i32 - r as i32 + di as i32;
        let sample = if xi < 0 {
            if repeat_edge {
                ch[row]
            } else {
                neutral_value(is_min)
            }
        } else if xi >= w as i32 {
            if repeat_edge {
                ch[row + w - 1]
            } else {
                neutral_value(is_min)
            }
        } else {
            ch[row + xi as usize]
        };

        value = if di == 0 {
            sample
        } else if is_min {
            value.min(sample)
        } else {
            value.max(sample)
        };
    }

    value
}

fn sample_v(
    ch: &[u8],
    w: usize,
    h: usize,
    x: usize,
    y: usize,
    r: usize,
    is_min: bool,
    repeat_edge: bool,
) -> u8 {
    if r == 0 {
        return ch[y * w + x];
    }

    let mut value = neutral_value(is_min);

    for di in 0..=(2 * r) {
        let yi = y as i32 - r as i32 + di as i32;
        let sample = if yi < 0 {
            if repeat_edge {
                ch[x]
            } else {
                neutral_value(is_min)
            }
        } else if yi >= h as i32 {
            if repeat_edge {
                ch[(h - 1) * w + x]
            } else {
                neutral_value(is_min)
            }
        } else {
            ch[yi as usize * w + x]
        };

        value = if di == 0 {
            sample
        } else if is_min {
            value.min(sample)
        } else {
            value.max(sample)
        };
    }

    value
}

fn minimax_1d_h(
    ch: &[u8],
    w: usize,
    h: usize,
    base_radius: usize,
    is_min: bool,
    luma_map: Option<&[f64]>,
    repeat_edge: bool,
) -> Vec<u8> {
    let mut out = vec![0u8; w * h];

    match luma_map {
        Some(map) => {
            for y in 0..h {
                let row = y * w;
                for x in 0..w {
                    let idx = row + x;
                    let luma = map.get(idx).copied().unwrap_or(1.0);
                    let r = (base_radius as f64 * luma).round().max(0.0) as usize;
                    out[idx] = sample_h(ch, w, h, x, y, r, is_min, repeat_edge);
                }
            }
        }
        None => {
            let r = base_radius;
            if r == 0 {
                return ch.to_vec();
            }
            let pad_val = neutral_value(is_min);

            for y in 0..h {
                let row = y * w;
                let pw = w + 2 * r;
                let mut padded = vec![0u8; pw];

                padded[r..r + w].copy_from_slice(&ch[row..row + w]);

                let left_val = if repeat_edge { ch[row] } else { pad_val };
                for item in padded.iter_mut().take(r) {
                    *item = left_val;
                }

                let right_val = if repeat_edge {
                    ch[row + w - 1]
                } else {
                    pad_val
                };
                for item in padded.iter_mut().skip(r + w).take(r) {
                    *item = right_val;
                }

                vanherk_1d(&padded, pw, r, is_min, &mut out[row..row + w], r);
            }
        }
    }
    out
}

fn minimax_1d_v(
    ch: &[u8],
    w: usize,
    h: usize,
    base_radius: usize,
    is_min: bool,
    luma_map: Option<&[f64]>,
    repeat_edge: bool,
) -> Vec<u8> {
    let mut out = vec![0u8; w * h];

    match luma_map {
        Some(map) => {
            for x in 0..w {
                for y in 0..h {
                    let idx = y * w + x;
                    let luma = map.get(idx).copied().unwrap_or(1.0);
                    let r = (base_radius as f64 * luma).round().max(0.0) as usize;
                    out[idx] = sample_v(ch, w, h, x, y, r, is_min, repeat_edge);
                }
            }
        }
        None => {
            let r = base_radius;
            if r == 0 {
                return ch.to_vec();
            }
            let pad_val = neutral_value(is_min);
            let ph = h + 2 * r;
            let mut col_in = vec![0u8; ph];
            let mut col_out = vec![0u8; h];

            for x in 0..w {
                for y in 0..h {
                    col_in[r + y] = ch[y * w + x];
                }

                let top_val = if repeat_edge { ch[x] } else { pad_val };
                for item in col_in.iter_mut().take(r) {
                    *item = top_val;
                }

                let bot_val = if repeat_edge {
                    ch[(h - 1) * w + x]
                } else {
                    pad_val
                };
                for item in col_in.iter_mut().skip(r + h).take(r) {
                    *item = bot_val;
                }

                vanherk_1d(&col_in, ph, r, is_min, &mut col_out, r);

                for y in 0..h {
                    out[y * w + x] = col_out[y];
                }
            }
        }
    }
    out
}

fn vanherk_1d(data: &[u8], len: usize, r: usize, is_min: bool, out: &mut [u8], out_offset: usize) {
    let diam = 2 * r + 1;
    let out_len = out.len();

    let mut prefix = vec![0u8; len];
    let mut suffix = vec![0u8; len];

    let mut block_start = 0;
    while block_start < len {
        let block_end = (block_start + diam).min(len);

        prefix[block_start] = data[block_start];
        for i in (block_start + 1)..block_end {
            prefix[i] = if is_min {
                prefix[i - 1].min(data[i])
            } else {
                prefix[i - 1].max(data[i])
            };
        }

        suffix[block_end - 1] = data[block_end - 1];
        for i in (block_start..block_end.saturating_sub(1)).rev() {
            suffix[i] = if is_min {
                suffix[i + 1].min(data[i])
            } else {
                suffix[i + 1].max(data[i])
            };
        }

        block_start = block_end;
    }

    for (i, item) in out.iter_mut().enumerate().take(out_len) {
        let center = out_offset + i;
        let left = center - r;
        let right = (center + r).min(len - 1);

        *item = if is_min {
            suffix[left].min(prefix[right])
        } else {
            suffix[left].max(prefix[right])
        };
    }
}

#[inline]
fn neutral_value(is_min: bool) -> u8 {
    if is_min {
        255
    } else {
        0
    }
}

#[inline]
fn lerp(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 * (1.0 - t) + b as f32 * t + 0.5).clamp(0.0, 255.0) as u8
}

fn blend_channels(a: &[u8], b: &[u8], t: f32) -> Vec<u8> {
    a.iter().zip(b).map(|(&av, &bv)| lerp(av, bv, t)).collect()
}
