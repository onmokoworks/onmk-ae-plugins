use crate::SlitScanParams;

pub const MODE_HORIZONTAL: i32 = 1;
pub const MODE_VERTICAL: i32 = 2;
pub const MODE_RADIAL: i32 = 3;
pub const MODE_MAP_LAYER: i32 = 4;

const INTERP_NEAREST: i32 = 1;

pub fn generate_gradient_map(
    mode: i32,
    w: usize,
    h: usize,
    phase: f64,
    center: (f64, f64),
    invert: bool,
) -> Vec<f64> {
    let mut map = vec![0.0f64; w * h];
    let wf = (w as f64 - 1.0).max(1.0);
    let hf = (h as f64 - 1.0).max(1.0);

    let cx = center.0 * w as f64;
    let cy = center.1 * h as f64;
    let max_dist = if mode == MODE_RADIAL {
        let d00 = (cx * cx + cy * cy).sqrt();
        let d10 = ((w as f64 - cx).powi(2) + cy.powi(2)).sqrt();
        let d01 = (cx.powi(2) + (h as f64 - cy).powi(2)).sqrt();
        let d11 = ((w as f64 - cx).powi(2) + (h as f64 - cy).powi(2)).sqrt();
        d00.max(d10).max(d01).max(d11).max(1.0)
    } else {
        1.0
    };

    for y in 0..h {
        for x in 0..w {
            let raw = match mode {
                MODE_VERTICAL => y as f64 / hf,
                MODE_RADIAL => {
                    let dx = x as f64 - cx;
                    let dy = y as f64 - cy;
                    ((dx * dx + dy * dy).sqrt() / max_dist).min(1.0)
                }
                _ => x as f64 / wf, // MODE_HORIZONTAL and fallback
            };

            let val = (raw + phase).clamp(0.0, 1.0);
            map[y * w + x] = if invert { 1.0 - val } else { val };
        }
    }

    map
}

pub fn render(
    params: &SlitScanParams,
    frames: &[Vec<u8>],
    w: usize,
    h: usize,
    map: &[f64],
    original: &[u8],
) -> Vec<u8> {
    let num_frames = frames.len();
    let npx = w * h;

    if num_frames < 2 {
        return original.to_vec();
    }

    let mut out = vec![0u8; npx * 4];
    let max_idx = (num_frames - 1) as f64;

    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            let map_val = map[idx].clamp(0.0, 1.0);
            let frame_f = map_val * max_idx;
            let off = idx * 4;

            if params.interpolation == INTERP_NEAREST {
                let fi = (frame_f.round() as usize).min(num_frames - 1);
                out[off] = frames[fi][off];
                out[off + 1] = frames[fi][off + 1];
                out[off + 2] = frames[fi][off + 2];
                out[off + 3] = frames[fi][off + 3];
            } else {
                let lo = (frame_f.floor() as usize).min(num_frames - 1);
                let hi = (lo + 1).min(num_frames - 1);
                let t = (frame_f - lo as f64) as f32;
                let inv_t = 1.0 - t;
                out[off] =
                    (frames[lo][off] as f32 * inv_t + frames[hi][off] as f32 * t).round() as u8;
                out[off + 1] = (frames[lo][off + 1] as f32 * inv_t + frames[hi][off + 1] as f32 * t)
                    .round() as u8;
                out[off + 2] = (frames[lo][off + 2] as f32 * inv_t + frames[hi][off + 2] as f32 * t)
                    .round() as u8;
                out[off + 3] = (frames[lo][off + 3] as f32 * inv_t + frames[hi][off + 3] as f32 * t)
                    .round() as u8;
            }
        }
    }

    if params.mix < 1.0 {
        let mix = params.mix;
        let inv = 1.0 - mix;
        for i in 0..npx {
            let off = i * 4;
            for c in 0..4 {
                let o = original[off + c] as f32;
                let s = out[off + c] as f32;
                out[off + c] = (o * inv + s * mix).clamp(0.0, 255.0).round() as u8;
            }
        }
    }

    out
}
