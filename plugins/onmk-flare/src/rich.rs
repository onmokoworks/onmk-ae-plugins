//! High-quality optical element renderer layered on FlareSim geometry.
//!
//! FlareSim determines the physical axis/lens character. This stage supplies
//! the aperture silhouettes, coating colour, cat-eye deformation and internal
//! reflection texture that a sparse ray splat alone cannot describe.

use crate::{physical::BrightSource, EffectParams};
const PI: f32 = std::f32::consts::PI;

pub fn render(ep: &EffectParams, sources: &[BrightSource], w: usize, h: usize) -> Vec<[f32; 3]> {
    if ep.style_preset >= 6 || sources.is_empty() {
        return vec![[0.0; 3]; w * h];
    }
    render_config(ep.lens_preset, ep.ghost_complexity as f32, sources, w, h)
}

pub fn render_config(
    lens_preset: usize,
    complexity: f32,
    sources: &[BrightSource],
    w: usize,
    h: usize,
) -> Vec<[f32; 3]> {
    let mut out = vec![[0.0; 3]; w * h];
    let mut selected = sources.to_vec();
    selected.sort_by(|a, b| {
        energy(b)
            .partial_cmp(&energy(a))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    selected.truncate(4);
    let fov_h = 50_f64.to_radians();
    let fov_v = 2.0 * ((h as f64 / w as f64) * (fov_h * 0.5).tan()).atan();
    let min_dim = w.min(h) as f32;
    let count = (10.0 + complexity * 18.0) as usize;
    for (si, source) in selected.iter().enumerate() {
        let sx = ((0.5 + source.angle_x / fov_h) * w as f64) as f32;
        let sy = ((0.5 + source.angle_y / fov_v) * h as f64) as f32;
        let axis_x = w as f32 * 0.5 - sx;
        let axis_y = h as f32 * 0.5 - sy;
        let src_e = energy(source).max(0.05);
        for k in 0..count {
            let seed = (lens_preset as u32 + 1).wrapping_mul(0x9e37_79b9)
                ^ (k as u32).wrapping_mul(0x85eb_ca6b)
                ^ si as u32;
            let j0 = hash(seed);
            let j1 = hash(seed ^ 0xa511_e9b3);
            let j2 = hash(seed ^ 0x63d8_3595);
            let t = -0.32 + 2.18 * (k as f32 + 0.22 + j0 * 0.56) / count as f32;
            let cross = (j1 - 0.5) * min_dim * 0.018;
            let alen = (axis_x * axis_x + axis_y * axis_y).sqrt().max(1.0);
            let gx = sx + axis_x * t - axis_y / alen * cross;
            let gy = sy + axis_y * t + axis_x / alen * cross;
            let featured = k % 7 == 2 || k % 11 == 4;
            let radius = min_dim
                * if featured {
                    0.055 + 0.075 * j1
                } else {
                    0.006 + 0.031 * j1 * j1
                };
            let edge = ((gx / w as f32 - 0.5).powi(2) + (gy / h as f32 - 0.5).powi(2)).sqrt();
            let aspect = (0.72 + 0.58 * j2) * (1.0 - 0.48 * edge).max(0.46);
            let rotation = j0 * PI + (axis_y).atan2(axis_x) * 0.16;
            let blades = 5 + (seed % 4) as usize;
            let kind = (seed % 6) as usize;
            let base = (0.025 + 0.16 * j2.powi(2)) * if featured { 1.7 } else { 1.0 };
            let color = coating_color(lens_preset, k, j0);
            draw_element(
                &mut out,
                w,
                h,
                Element {
                    gx,
                    gy,
                    radius,
                    aspect,
                    rotation,
                    blades,
                    kind,
                    intensity: base * src_e * (0.45 + complexity),
                    color,
                    seed,
                },
            );
        }
        draw_beads(&mut out, w, h, sx, sy, axis_x, axis_y, src_e, si as u32);
    }
    out
}

struct Element {
    gx: f32,
    gy: f32,
    radius: f32,
    aspect: f32,
    rotation: f32,
    blades: usize,
    kind: usize,
    intensity: f32,
    color: [f32; 3],
    seed: u32,
}

fn draw_element(out: &mut [[f32; 3]], w: usize, h: usize, e: Element) {
    let bound = e.radius * 1.55;
    let x0 = (e.gx - bound).floor().max(0.0) as usize;
    let x1 = (e.gx + bound).ceil().min((w - 1) as f32) as usize;
    let y0 = (e.gy - bound).floor().max(0.0) as usize;
    let y1 = (e.gy + bound).ceil().min((h - 1) as f32) as usize;
    let (cs, sn) = (e.rotation.cos(), e.rotation.sin());
    for y in y0..=y1 {
        for x in x0..=x1 {
            let dx = x as f32 + 0.5 - e.gx;
            let dy = y as f32 + 0.5 - e.gy;
            let rx = (dx * cs + dy * sn) / e.aspect.max(0.2);
            let ry = -dx * sn + dy * cs;
            let a = ry.atan2(rx);
            let sector = 2.0 * PI / e.blades as f32;
            let local = (a + sector * 0.5).rem_euclid(sector) - sector * 0.5;
            let poly = (PI / e.blades as f32).cos() / local.cos().max(0.2);
            let q = (rx * rx + ry * ry).sqrt() / (e.radius * poly).max(0.5);
            let fill = (-0.75 * q.powi(6)).exp();
            let rim = (-0.5 * ((q - 0.78) / 0.14).powi(2)).exp();
            let inner = (-0.5 * ((q - 0.48) / 0.22).powi(2)).exp();
            let crescent = (fill
                - (-0.72
                    * (((rx + e.radius * 0.34).powi(2) + ry.powi(2)).sqrt() / e.radius).powi(6))
                .exp())
            .max(0.0);
            let spectral = (-0.5 * ((q - 0.68) / 0.18).powi(2)).exp();
            let shape = match e.kind {
                0 => fill,
                1 => 0.18 * fill + 0.52 * rim,
                2 => 0.56 * fill + 0.46 * rim,
                3 => crescent,
                4 => 0.16 * fill + 0.58 * spectral,
                _ => 0.58 * fill + 0.42 * inner,
            };
            let texture = 0.64
                + 0.24 * (rx * 0.071 + hash(e.seed) * 9.0).sin() * (ry * 0.053 + 1.7).cos()
                + 0.12 * hash2(x as u32, y as u32, e.seed);
            let v = shape * texture.max(0.18) * e.intensity;
            if v > 1e-6 {
                let p = &mut out[y * w + x];
                for c in 0..3 {
                    p[c] += v * e.color[c];
                }
            }
        }
    }
}

fn draw_beads(
    out: &mut [[f32; 3]],
    w: usize,
    h: usize,
    sx: f32,
    sy: f32,
    ax: f32,
    ay: f32,
    e: f32,
    seed: u32,
) {
    for i in 0..9 {
        let t = 0.58 + i as f32 * 0.055;
        let gx = sx + ax * t;
        let gy = sy + ay * t;
        let r = 1.2 + (i % 3) as f32 * 1.1;
        let c = coating_color(seed as usize, i, hash(seed + i as u32));
        let el = Element {
            gx,
            gy,
            radius: r,
            aspect: 1.0,
            rotation: 0.0,
            blades: 7,
            kind: 0,
            intensity: e * (0.13 - 0.009 * i as f32),
            color: c,
            seed: seed + i as u32,
        };
        draw_element(out, w, h, el);
    }
}

fn coating_color(lens: usize, k: usize, j: f32) -> [f32; 3] {
    let palettes = [
        [
            [0.20, 0.84, 1.0],
            [1.0, 0.22, 0.52],
            [0.35, 1.0, 0.55],
            [0.72, 0.34, 1.0],
        ],
        [
            [0.12, 0.78, 1.0],
            [1.0, 0.28, 0.68],
            [0.40, 1.0, 0.72],
            [0.62, 0.38, 1.0],
        ],
        [
            [1.0, 0.48, 0.18],
            [0.22, 1.0, 0.68],
            [0.48, 0.34, 1.0],
            [1.0, 0.82, 0.42],
        ],
        [
            [0.22, 1.0, 0.48],
            [0.18, 0.56, 1.0],
            [1.0, 0.24, 0.58],
            [0.68, 0.30, 1.0],
        ],
    ];
    let mut c = palettes[lens.min(3)][(k + (j * 4.0) as usize) % 4];
    let desat = 0.74 + 0.2 * j;
    for v in &mut c {
        *v = 0.5 + (*v - 0.5) * desat;
    }
    c
}
fn energy(s: &BrightSource) -> f32 {
    0.2126 * s.rgb[0] + 0.7152 * s.rgb[1] + 0.0722 * s.rgb[2]
}
fn hash(mut x: u32) -> f32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x as f64 as f32 / u32::MAX as f32
}
fn hash2(x: u32, y: u32, s: u32) -> f32 {
    hash(x.wrapping_mul(374761393) ^ y.wrapping_mul(668265263) ^ s)
}
