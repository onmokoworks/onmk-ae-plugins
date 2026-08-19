#[derive(Clone, Copy, Debug)]
pub struct EdgeParams {
    pub border: f32,
    pub roughness: f32,
    pub scale: f32,
    pub detail: u32,
    pub balance: f32,
    pub sharpness: f32,
    pub stretch: f32,
    pub angle: f32,
    pub evolution: f32,
    pub seed: u32,
    pub preserve: f32,
    pub mix: f32,
}

impl Default for EdgeParams {
    fn default() -> Self {
        Self {
            border: 0.0,
            roughness: 12.0,
            scale: 48.0,
            detail: 4,
            balance: 0.0,
            sharpness: 70.0,
            stretch: 0.0,
            angle: 0.0,
            evolution: 0.0,
            seed: 0,
            preserve: 50.0,
            mix: 1.0,
        }
    }
}

#[derive(Clone, Copy)]
struct Seed {
    x: i32,
    y: i32,
}

const INF: f32 = 1.0e20;

fn distance_field(
    alpha: &[f32],
    w: usize,
    h: usize,
    feature_inside: bool,
) -> (Vec<f32>, Vec<Seed>) {
    let mut d = vec![INF; w * h];
    let mut seed = vec![Seed { x: -1, y: -1 }; w * h];
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if (alpha[i] >= 0.5) == feature_inside {
                d[i] = 0.0;
                seed[i] = Seed {
                    x: x as i32,
                    y: y as i32,
                };
            }
        }
    }
    let diag = std::f32::consts::SQRT_2;
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            for (dx, dy, c) in [(-1, 0, 1.0), (0, -1, 1.0), (-1, -1, diag), (1, -1, diag)] {
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                if nx >= 0 && ny >= 0 && nx < w as i32 && ny < h as i32 {
                    let n = ny as usize * w + nx as usize;
                    if d[n] + c < d[i] {
                        d[i] = d[n] + c;
                        seed[i] = seed[n];
                    }
                }
            }
        }
    }
    for y in (0..h).rev() {
        for x in (0..w).rev() {
            let i = y * w + x;
            for (dx, dy, c) in [(1, 0, 1.0), (0, 1, 1.0), (1, 1, diag), (-1, 1, diag)] {
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                if nx >= 0 && ny >= 0 && nx < w as i32 && ny < h as i32 {
                    let n = ny as usize * w + nx as usize;
                    if d[n] + c < d[i] {
                        d[i] = d[n] + c;
                        seed[i] = seed[n];
                    }
                }
            }
        }
    }
    (d, seed)
}

fn hash(mut x: u32) -> f32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x as f32 / u32::MAX as f32
}

fn value_noise(x: f32, y: f32, seed: u32) -> f32 {
    let ix = x.floor() as i32;
    let iy = y.floor() as i32;
    let fx = x - ix as f32;
    let fy = y - iy as f32;
    let u = fx * fx * (3.0 - 2.0 * fx);
    let v = fy * fy * (3.0 - 2.0 * fy);
    let h = |dx: i32, dy: i32| hash((ix + dx) as u32 ^ ((iy + dy) as u32).rotate_left(16) ^ seed);
    let a = h(0, 0) + (h(1, 0) - h(0, 0)) * u;
    let b = h(0, 1) + (h(1, 1) - h(0, 1)) * u;
    (a + (b - a) * v) * 2.0 - 1.0
}

fn fbm(x: f32, y: f32, p: &EdgeParams) -> f32 {
    let r = p.angle.to_radians();
    let cs = r.cos();
    let sn = r.sin();
    let mut px = (x * cs - y * sn) / p.scale.max(1.0);
    let mut py = (x * sn + y * cs) / p.scale.max(1.0)
        * (1.0 - p.stretch.clamp(-99.0, 99.0) / 100.0).max(0.01);
    let mut sum = 0.0;
    let mut amp = 0.5;
    let mut norm = 0.0;
    for o in 0..p.detail.clamp(1, 8) {
        sum += value_noise(
            px + p.evolution * 0.01,
            py + p.evolution * 0.006,
            p.seed.wrapping_add(o * 1013),
        ) * amp;
        norm += amp;
        px *= 2.0;
        py *= 2.0;
        amp *= 0.5;
    }
    sum / norm.max(1.0e-6)
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a).max(1.0e-6)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub fn render_rgba(input: &[f32], w: usize, h: usize, p: &EdgeParams) -> Vec<f32> {
    if w == 0 || h == 0 || input.len() < w * h * 4 {
        return Vec::new();
    }
    let alpha: Vec<f32> = (0..w * h)
        .map(|i| input[i * 4 + 3].clamp(0.0, 1.0))
        .collect();
    let (to_inside, inside_seed) = distance_field(&alpha, w, h, true);
    let (to_outside, _) = distance_field(&alpha, w, h, false);
    let mut out = vec![0.0; w * h * 4];
    let softness = (1.0 - p.sharpness.clamp(0.0, 100.0) / 100.0) * 3.5 + 0.35;
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let src = i * 4;
            let signed = if alpha[i] >= 0.5 {
                to_outside[i]
            } else {
                -to_inside[i]
            };
            let n = fbm(x as f32, y as f32, p);
            let biased = if n >= 0.0 {
                n * (1.0 + p.balance / 100.0)
            } else {
                n * (1.0 - p.balance / 100.0)
            };
            let protect = (signed.abs() / (p.preserve * 0.25 + 1.0)).clamp(0.0, 1.0);
            let shaped = signed + p.border + biased * p.roughness * (1.0 - protect * 0.85);
            let edge_alpha = smoothstep(-softness, softness, shaped);
            let final_alpha = alpha[i] + (edge_alpha - alpha[i]) * p.mix;
            let color_i = if alpha[i] > 1.0e-5 {
                i
            } else {
                let s = inside_seed[i];
                if s.x >= 0 {
                    s.y as usize * w + s.x as usize
                } else {
                    i
                }
            } * 4;
            out[src] = input[color_i];
            out[src + 1] = input[color_i + 1];
            out[src + 2] = input[color_i + 2];
            out[src + 3] = final_alpha.clamp(0.0, 1.0);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deterministic() {
        let p = EdgeParams::default();
        assert_eq!(fbm(12.0, 9.0, &p), fbm(12.0, 9.0, &p));
    }
    #[test]
    fn border_expands_alpha() {
        let mut src = vec![0.0; 9 * 9 * 4];
        for y in 3..6 {
            for x in 3..6 {
                src[(y * 9 + x) * 4 + 3] = 1.0;
            }
        }
        let base: f32 = render_rgba(
            &src,
            9,
            9,
            &EdgeParams {
                roughness: 0.0,
                ..Default::default()
            },
        )
        .iter()
        .skip(3)
        .step_by(4)
        .sum();
        let grown: f32 = render_rgba(
            &src,
            9,
            9,
            &EdgeParams {
                border: 2.0,
                roughness: 0.0,
                ..Default::default()
            },
        )
        .iter()
        .skip(3)
        .step_by(4)
        .sum();
        assert!(grown > base);
    }
    #[test]
    fn zero_mix_preserves_alpha() {
        let src = vec![0.25; 4 * 4 * 4];
        let out = render_rgba(
            &src,
            4,
            4,
            &EdgeParams {
                mix: 0.0,
                ..Default::default()
            },
        );
        for i in 0..16 {
            assert_eq!(out[i * 4 + 3], 0.25);
        }
    }
}
