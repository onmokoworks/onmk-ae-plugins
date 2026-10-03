//! AE-facing Rust port of the MIT-licensed blackhole-rt/flaresim renderer.

use crate::lens::{LensSystem, Surface};
use std::ops::{Add, Div, Mul, Neg, Sub};

#[derive(Clone, Copy, Default)]
struct V3 {
    x: f64,
    y: f64,
    z: f64,
}
impl Add for V3 {
    type Output = Self;
    fn add(self, r: Self) -> Self {
        Self {
            x: self.x + r.x,
            y: self.y + r.y,
            z: self.z + r.z,
        }
    }
}
impl Sub for V3 {
    type Output = Self;
    fn sub(self, r: Self) -> Self {
        Self {
            x: self.x - r.x,
            y: self.y - r.y,
            z: self.z - r.z,
        }
    }
}
impl Mul<f64> for V3 {
    type Output = Self;
    fn mul(self, r: f64) -> Self {
        Self {
            x: self.x * r,
            y: self.y * r,
            z: self.z * r,
        }
    }
}
impl Div<f64> for V3 {
    type Output = Self;
    fn div(self, r: f64) -> Self {
        self * (1.0 / r)
    }
}
impl Neg for V3 {
    type Output = Self;
    fn neg(self) -> Self {
        self * -1.0
    }
}
impl V3 {
    fn dot(self, r: Self) -> f64 {
        self.x * r.x + self.y * r.y + self.z * r.z
    }
    fn norm(self) -> Self {
        self / self.dot(self).sqrt().max(1e-15)
    }
}
#[derive(Clone, Copy)]
struct Ray {
    o: V3,
    d: V3,
}
#[derive(Clone, Copy)]
struct Hit {
    p: V3,
    w: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BrightSource {
    pub angle_x: f64,
    pub angle_y: f64,
    pub rgb: [f32; 3],
}

#[derive(Clone, Copy)]
pub struct Config {
    pub ray_grid: usize,
    pub min_ghost: f64,
    pub gain: f32,
    pub normalize: bool,
    pub max_area_boost: f32,
    pub ghost_blur: f32,
    pub ghost_blur_passes: usize,
    /// Reconstruct adjacent pupil samples as projected triangles.  This is
    /// the Hullin-style path: it preserves each ghost's warped aperture
    /// footprint and spatial density instead of reducing it to point splats.
    pub surface_raster: bool,
    pub bloom_strength: f32,
    pub bloom_radius: f32,
    pub bloom_passes: usize,
    pub bloom_octaves: usize,
    pub bloom_chromatic: bool,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            ray_grid: 40,
            min_ghost: 1e-7,
            gain: 3_000.0,
            normalize: true,
            max_area_boost: 8.0,
            ghost_blur: 0.005,
            ghost_blur_passes: 3,
            surface_raster: true,
            bloom_strength: 0.6,
            bloom_radius: 0.018,
            bloom_passes: 3,
            bloom_octaves: 4,
            bloom_chromatic: true,
        }
    }
}

fn ior(s: &Surface, l: f64) -> f64 {
    if s.abbe < 0.1 || s.ior <= 1.0001 {
        return s.ior;
    }
    let (lf, ld, lc) = (486.13_f64, 587.56_f64, 656.27_f64);
    let dn = (s.ior - 1.0) / s.abbe;
    let b = dn / (1.0 / lf.powi(2) - 1.0 / lc.powi(2));
    s.ior - b / ld.powi(2) + b / l.powi(2)
}
fn before(l: &LensSystem, i: usize, w: f64) -> f64 {
    if i == 0 {
        1.0
    } else {
        ior(&l.surfaces[i - 1], w)
    }
}
fn fresnel(c: f64, n1: f64, n2: f64) -> f64 {
    let c = c.abs();
    let e = n1 / n2;
    let st = e * e * (1.0 - c * c);
    if st >= 1.0 {
        return 1.0;
    }
    let ct = (1.0 - st).sqrt();
    let rs = (n1 * c - n2 * ct) / (n1 * c + n2 * ct);
    let rp = (n2 * c - n1 * ct) / (n2 * c + n1 * ct);
    0.5 * (rs * rs + rp * rp)
}
fn reflectance(c: f64, n1: f64, n2: f64, layers: i32, w: f64) -> f64 {
    if layers <= 0 {
        return fresnel(c, n1, n2);
    }
    let nc = 1.38;
    let sc = (n1 / nc).powi(2) * (1.0 - c * c);
    if sc >= 1.0 {
        return fresnel(c, n1, n2);
    }
    let cc = (1.0 - sc).sqrt();
    let d = 550.0 / (4.0 * nc);
    let phase = 2.0 * std::f64::consts::PI * nc * d * cc / w;
    let r01 = (n1 * c - nc * cc) / (n1 * c + nc * cc);
    let s2 = (nc / n2).powi(2) * (1.0 - cc * cc);
    if s2 >= 1.0 {
        return fresnel(c, n1, n2);
    }
    let c2 = (1.0 - s2).sqrt();
    let r12 = (nc * cc - n2 * c2) / (nc * cc + n2 * c2);
    let q = (2.0 * phase).cos();
    let mut r = (r01 * r01 + r12 * r12 + 2.0 * r01 * r12 * q)
        / (1.0 + r01 * r01 * r12 * r12 + 2.0 * r01 * r12 * q);
    for _ in 1..layers {
        r *= 0.25
    }
    r.clamp(0.0, 1.0)
}
fn intersect(ray: Ray, s: &Surface) -> Option<(V3, V3)> {
    let (p, mut n) = if s.radius.abs() < 1e-6 {
        if ray.d.z.abs() < 1e-12 {
            return None;
        }
        let t = (s.z - ray.o.z) / ray.d.z;
        if t < 1e-6 {
            return None;
        }
        (
            ray.o + ray.d * t,
            V3 {
                x: 0.0,
                y: 0.0,
                z: if ray.d.z > 0.0 { -1.0 } else { 1.0 },
            },
        )
    } else {
        let c = V3 {
            x: 0.0,
            y: 0.0,
            z: s.z + s.radius,
        };
        let oc = ray.o - c;
        let a = ray.d.dot(ray.d);
        let b = 2.0 * oc.dot(ray.d);
        let q = oc.dot(oc) - s.radius * s.radius;
        let disc = b * b - 4.0 * a * q;
        if disc < 0.0 {
            return None;
        }
        let root = disc.sqrt();
        let ts = [(-b - root) / (2.0 * a), (-b + root) / (2.0 * a)];
        let mut best = None;
        let mut dz = f64::MAX;
        for t in ts {
            if t > 1e-6 {
                let e = (ray.o.z + t * ray.d.z - s.z).abs();
                if e < dz {
                    best = Some(t);
                    dz = e
                }
            }
        }
        let t = best?;
        let p = ray.o + ray.d * t;
        let mut n = (p - c) / s.radius.abs();
        if n.dot(ray.d) > 0.0 {
            n = -n
        }
        (p, n)
    };
    if p.x * p.x + p.y * p.y > s.semi_aperture * s.semi_aperture {
        return None;
    }
    if n.dot(ray.d) > 0.0 {
        n = -n
    }
    Some((p, n))
}
fn refract(d: V3, n: V3, r: f64) -> Option<V3> {
    let ci = -n.dot(d);
    let st = r * r * (1.0 - ci * ci);
    if st >= 1.0 {
        return None;
    }
    Some((d * r + n * (r * ci - (1.0 - st).sqrt())).norm())
}
fn reflect(d: V3, n: V3) -> V3 {
    (d - n * (2.0 * d.dot(n))).norm()
}
fn trace(mut ray: Ray, l: &LensSystem, a: usize, b: usize, wave: f64) -> Option<Hit> {
    let mut weight = 1.0;
    let mut current = 1.0;
    for i in 0..=b {
        let (h, n) = intersect(ray, &l.surfaces[i])?;
        ray.o = h;
        let n2 = ior(&l.surfaces[i], wave);
        let r = reflectance(n.dot(ray.d).abs(), current, n2, l.surfaces[i].coating, wave);
        if i == b {
            ray.d = reflect(ray.d, n);
            weight *= r
        } else {
            ray.d = refract(ray.d, n, current / n2)?;
            weight *= 1.0 - r;
            current = n2
        }
    }
    for i in (a..b).rev() {
        let (h, n) = intersect(ray, &l.surfaces[i])?;
        ray.o = h;
        let n2 = before(l, i, wave);
        let r = reflectance(n.dot(ray.d).abs(), current, n2, l.surfaces[i].coating, wave);
        if i == a {
            ray.d = reflect(ray.d, n);
            weight *= r;
            current = ior(&l.surfaces[a], wave)
        } else {
            ray.d = refract(ray.d, n, current / n2)?;
            weight *= 1.0 - r;
            current = n2
        }
    }
    for i in (a + 1)..l.surfaces.len() {
        let (h, n) = intersect(ray, &l.surfaces[i])?;
        ray.o = h;
        let n2 = ior(&l.surfaces[i], wave);
        let r = reflectance(n.dot(ray.d).abs(), current, n2, l.surfaces[i].coating, wave);
        ray.d = refract(ray.d, n, current / n2)?;
        weight *= 1.0 - r;
        current = n2
    }
    let t = (l.sensor_z - ray.o.z) / ray.d.z;
    if t < 0.0 {
        return None;
    }
    Some(Hit {
        p: ray.o + ray.d * t,
        w: weight,
    })
}
fn pairs(l: &LensSystem) -> Vec<(usize, usize)> {
    let mut p = Vec::new();
    for a in 0..l.surfaces.len() {
        for b in a + 1..l.surfaces.len() {
            if (before(l, a, 550.0) - ior(&l.surfaces[a], 550.0)).abs() > 0.001
                && (before(l, b, 550.0) - ior(&l.surfaces[b], 550.0)).abs() > 0.001
            {
                p.push((a, b))
            }
        }
    }
    p
}

#[derive(Clone, Copy)]
struct TraceState {
    ray: Ray,
    weight: f64,
    current: f64,
}

fn trace_first(mut ray: Ray, l: &LensSystem, b: usize, wave: f64) -> Option<TraceState> {
    let mut weight = 1.0;
    let mut current = 1.0;
    for i in 0..=b {
        let (h, n) = intersect(ray, &l.surfaces[i])?;
        ray.o = h;
        let n2 = ior(&l.surfaces[i], wave);
        let r = reflectance(n.dot(ray.d).abs(), current, n2, l.surfaces[i].coating, wave);
        if i == b {
            ray.d = reflect(ray.d, n);
            weight *= r;
        } else {
            ray.d = refract(ray.d, n, current / n2)?;
            weight *= 1.0 - r;
            current = n2;
        }
    }
    Some(TraceState {
        ray,
        weight,
        current,
    })
}

fn trace_remaining(
    state: TraceState,
    l: &LensSystem,
    a: usize,
    b: usize,
    wave: f64,
) -> Option<Hit> {
    let TraceState {
        mut ray,
        mut weight,
        mut current,
    } = state;
    for i in (a..b).rev() {
        let (h, n) = intersect(ray, &l.surfaces[i])?;
        ray.o = h;
        let n2 = before(l, i, wave);
        let r = reflectance(n.dot(ray.d).abs(), current, n2, l.surfaces[i].coating, wave);
        if i == a {
            ray.d = reflect(ray.d, n);
            weight *= r;
            current = ior(&l.surfaces[a], wave);
        } else {
            ray.d = refract(ray.d, n, current / n2)?;
            weight *= 1.0 - r;
            current = n2;
        }
    }
    for i in a + 1..l.surfaces.len() {
        let (h, n) = intersect(ray, &l.surfaces[i])?;
        ray.o = h;
        let n2 = ior(&l.surfaces[i], wave);
        let r = reflectance(n.dot(ray.d).abs(), current, n2, l.surfaces[i].coating, wave);
        ray.d = refract(ray.d, n, current / n2)?;
        weight *= 1.0 - r;
        current = n2;
    }
    let t = (l.sensor_z - ray.o.z) / ray.d.z;
    if t < 0.0 {
        return None;
    }
    Some(Hit {
        p: ray.o + ray.d * t,
        w: weight,
    })
}
fn pair_estimate(l: &LensSystem, a: usize, b: usize) -> f64 {
    [650.0, 550.0, 450.]
        .into_iter()
        .filter_map(|w| {
            trace(
                Ray {
                    o: V3 {
                        x: 0.0,
                        y: 0.0,
                        z: l.surfaces[0].z - 20.0,
                    },
                    d: V3 {
                        x: 0.0,
                        y: 0.0,
                        z: 1.0,
                    },
                },
                l,
                a,
                b,
                w,
            )
        })
        .map(|h| h.w)
        .sum::<f64>()
        / 3.0
}
fn area_boost(l: &LensSystem, a: usize, b: usize, sw: f64, sh: f64, max: f32) -> f32 {
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    for gy in 0..8 {
        for gx in 0..8 {
            let u = (gx as f64 + 0.5) / 4. - 1.0;
            let v = (gy as f64 + 0.5) / 4. - 1.0;
            if u * u + v * v > 1. {
                continue;
            }
            if let Some(h) = trace(
                Ray {
                    o: V3 {
                        x: u * l.surfaces[0].semi_aperture,
                        y: v * l.surfaces[0].semi_aperture,
                        z: l.surfaces[0].z - 20.0,
                    },
                    d: V3 {
                        x: 0.0,
                        y: 0.0,
                        z: 1.0,
                    },
                },
                l,
                a,
                b,
                550.0,
            ) {
                xs.push(h.p.x);
                ys.push(h.p.y)
            }
        }
    }
    if xs.len() < 2 {
        return 1.0;
    }
    let range = |v: &Vec<f64>| {
        v.iter().copied().fold(f64::NEG_INFINITY, f64::max)
            - v.iter().copied().fold(f64::INFINITY, f64::min)
    };
    ((range(&xs) * range(&ys) / (4. * sw * sh)).clamp(1.0, max as f64)) as f32
}
fn tent(buf: &mut [[f32; 3]], w: usize, h: usize, px: f32, py: f32, ch: usize, value: f32, r: f32) {
    tent_rows(buf, w, h, px, py, ch, value, r, 0);
}
fn tent_rows(
    buf: &mut [[f32; 3]],
    w: usize,
    h: usize,
    px: f32,
    py: f32,
    ch: usize,
    value: f32,
    r: f32,
    row_start: usize,
) {
    let r = r.max(1.5);
    let x0 = (px - r).floor().max(0.0) as usize;
    let x1 = (px + r).ceil().min((w - 1) as f32) as usize;
    let y0 = (py - r).floor().max(0.0) as usize;
    let y1 = (py + r).ceil().min((h - 1) as f32) as usize;
    let norm = value / (r * r);
    for y in y0.max(row_start)..=y1.min(row_start + buf.len() / w - 1) {
        let wy = (1. - ((y as f32 + 0.5) - py).abs() / r).max(0.0);
        for x in x0..=x1 {
            let wx = (1. - ((x as f32 + 0.5) - px).abs() / r).max(0.0);
            buf[(y - row_start) * w + x][ch] += norm * wx * wy
        }
    }
}

#[derive(Clone, Copy)]
struct SensorSample {
    x: f32,
    y: f32,
    energy: f32,
}

#[cfg(test)]
fn raster_triangle(
    buf: &mut [[f32; 3]],
    w: usize,
    h: usize,
    ch: usize,
    a: SensorSample,
    b: SensorSample,
    c: SensorSample,
    row_start: usize,
) {
    let signed = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    let area = signed.abs() * 0.5;
    let edge_ab = ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt();
    let edge_bc = ((c.x - b.x).powi(2) + (c.y - b.y).powi(2)).sqrt();
    let edge_ca = ((a.x - c.x).powi(2) + (a.y - c.y).powi(2)).sqrt();
    let longest = edge_ab.max(edge_bc).max(edge_ca);
    let diagonal = ((w * w + h * h) as f32).sqrt();
    let total = (a.energy + b.energy + c.energy) / 6.0;
    // Rays on opposite sides of a caustic or a failed pupil boundary can be
    // neighbours in the input grid while landing arbitrarily far apart on the
    // sensor.  Such discontinuities must not be bridged by a giant triangle.
    if !area.is_finite()
        || !longest.is_finite()
        || longest > diagonal * 0.16
        || area > (w * h) as f32 * 0.018
        || (longest > 8.0 && area / (longest * longest) < 0.0015)
    {
        return;
    }
    // Each quad represents one pupil cell; its two triangles each carry half
    // the interpolated cell energy.  Very small projected triangles are
    // filtered as a sub-pixel tent so they cannot disappear between samples.
    if area < 0.75 {
        tent_rows(
            buf,
            w,
            h,
            (a.x + b.x + c.x) / 3.0,
            (a.y + b.y + c.y) / 3.0,
            ch,
            total,
            1.5,
            row_start,
        );
        return;
    }
    let minx = a.x.min(b.x).min(c.x).floor().max(0.0) as usize;
    let maxx = a.x.max(b.x).max(c.x).ceil().min((w - 1) as f32) as usize;
    let miny = a.y.min(b.y).min(c.y).floor().max(0.0) as usize;
    let maxy = a.y.max(b.y).max(c.y).ceil().min((h - 1) as f32) as usize;
    if minx > maxx || miny > maxy {
        return;
    }
    let edge = |p: (f32, f32), q: (f32, f32), x: f32, y: f32| {
        (x - p.0) * (q.1 - p.1) - (y - p.1) * (q.0 - p.0)
    };
    let inv_area = 1.0 / signed.abs().max(1e-8);
    let density = total / area.max(1.0);
    for y in miny.max(row_start)..=maxy.min(row_start + buf.len() / w - 1) {
        for x in minx..=maxx {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let e0 = edge((a.x, a.y), (b.x, b.y), px, py);
            let e1 = edge((b.x, b.y), (c.x, c.y), px, py);
            let e2 = edge((c.x, c.y), (a.x, a.y), px, py);
            if (e0 >= 0.0 && e1 >= 0.0 && e2 >= 0.0) || (e0 <= 0.0 && e1 <= 0.0 && e2 <= 0.0) {
                // A mild barycentric energy interpolation retains coating and
                // Fresnel gradients without changing the triangle's flux.
                let wa = edge((b.x, b.y), (c.x, c.y), px, py).abs() * inv_area;
                let wb = edge((c.x, c.y), (a.x, a.y), px, py).abs() * inv_area;
                let wc = (1.0 - wa - wb).max(0.0);
                let mean = (a.energy + b.energy + c.energy) / 3.0;
                let local = if mean > 1e-12 {
                    (wa * a.energy + wb * b.energy + wc * c.energy) / mean
                } else {
                    1.0
                };
                buf[(y - row_start) * w + x][ch] += density * local;
            }
        }
    }
}

pub fn render(
    l: &LensSystem,
    sources: &[BrightSource],
    w: usize,
    h: usize,
    cfg: Config,
) -> Vec<[f32; 3]> {
    let workers = std::thread::available_parallelism().map_or(1, |n| n.get().min(4));
    render_with_workers(l, sources, w, h, cfg, workers)
}

type Triangle = (usize, SensorSample, SensorSample, SensorSample);

// SSE2 is part of the x86-64 baseline. Four independent pixels retain the
// scalar multiply/subtract/divide order (no reciprocal approximation or FMA).
#[cfg(target_arch = "x86_64")]
#[inline(always)]
fn raster_quad(
    a: SensorSample,
    b: SensorSample,
    c: SensorSample,
    x: usize,
    y: usize,
    inv_area: f32,
    density: f32,
    mean: f32,
) -> ([f32; 4], i32) {
    use std::arch::x86_64::*;
    // All operations are on registers except an unaligned store to four f32s.
    unsafe {
        let px = _mm_setr_ps(
            x as f32 + 0.5,
            (x + 1) as f32 + 0.5,
            (x + 2) as f32 + 0.5,
            (x + 3) as f32 + 0.5,
        );
        let py = _mm_set1_ps(y as f32 + 0.5);
        let edge = |p: SensorSample, q: SensorSample| {
            _mm_sub_ps(
                _mm_mul_ps(_mm_sub_ps(px, _mm_set1_ps(p.x)), _mm_set1_ps(q.y - p.y)),
                _mm_mul_ps(_mm_sub_ps(py, _mm_set1_ps(p.y)), _mm_set1_ps(q.x - p.x)),
            )
        };
        let e0 = edge(a, b);
        let e1 = edge(b, c);
        let e2 = edge(c, a);
        let zero = _mm_setzero_ps();
        let pos = _mm_and_ps(
            _mm_and_ps(_mm_cmpge_ps(e0, zero), _mm_cmpge_ps(e1, zero)),
            _mm_cmpge_ps(e2, zero),
        );
        let neg = _mm_and_ps(
            _mm_and_ps(_mm_cmple_ps(e0, zero), _mm_cmple_ps(e1, zero)),
            _mm_cmple_ps(e2, zero),
        );
        let mask = _mm_movemask_ps(_mm_or_ps(pos, neg));
        if mask == 0 {
            return ([0.0; 4], 0);
        }
        let local = if mean > 1e-12 {
            let wa = _mm_mul_ps(_mm_andnot_ps(_mm_set1_ps(-0.0), e1), _mm_set1_ps(inv_area));
            let wb = _mm_mul_ps(_mm_andnot_ps(_mm_set1_ps(-0.0), e2), _mm_set1_ps(inv_area));
            let wc = _mm_max_ps(_mm_sub_ps(_mm_sub_ps(_mm_set1_ps(1.0), wa), wb), zero);
            _mm_div_ps(
                _mm_add_ps(
                    _mm_add_ps(
                        _mm_mul_ps(wa, _mm_set1_ps(a.energy)),
                        _mm_mul_ps(wb, _mm_set1_ps(b.energy)),
                    ),
                    _mm_mul_ps(wc, _mm_set1_ps(c.energy)),
                ),
                _mm_set1_ps(mean),
            )
        } else {
            _mm_set1_ps(1.0)
        };
        let mut values = [0.0; 4];
        _mm_storeu_ps(values.as_mut_ptr(), _mm_mul_ps(_mm_set1_ps(density), local));
        (values, mask)
    }
}

enum PreparedTriangle {
    Tent {
        ch: usize,
        x: f32,
        y: f32,
        total: f32,
    },
    Area {
        ch: usize,
        a: SensorSample,
        b: SensorSample,
        c: SensorSample,
        minx: usize,
        maxx: usize,
        miny: usize,
        maxy: usize,
        inv_area: f32,
        density: f32,
        mean: f32,
    },
}

// Runtime-dispatched AVX, deliberately without AVX2/FMA or approximate divides.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx")]
unsafe fn raster_row_avx(
    row: &mut [[f32; 3]],
    ch: usize,
    a: SensorSample,
    b: SensorSample,
    c: SensorSample,
    mut x: usize,
    maxx: usize,
    y: usize,
    inv_area: f32,
    density: f32,
    mean: f32,
) -> usize {
    use std::arch::x86_64::*;
    let py = _mm256_set1_ps(y as f32 + 0.5);
    while x + 7 <= maxx {
        let px = _mm256_setr_ps(
            x as f32 + 0.5,
            (x + 1) as f32 + 0.5,
            (x + 2) as f32 + 0.5,
            (x + 3) as f32 + 0.5,
            (x + 4) as f32 + 0.5,
            (x + 5) as f32 + 0.5,
            (x + 6) as f32 + 0.5,
            (x + 7) as f32 + 0.5,
        );
        let edge = |p: SensorSample, q: SensorSample| {
            _mm256_sub_ps(
                _mm256_mul_ps(
                    _mm256_sub_ps(px, _mm256_set1_ps(p.x)),
                    _mm256_set1_ps(q.y - p.y),
                ),
                _mm256_mul_ps(
                    _mm256_sub_ps(py, _mm256_set1_ps(p.y)),
                    _mm256_set1_ps(q.x - p.x),
                ),
            )
        };
        let e0 = edge(a, b);
        let e1 = edge(b, c);
        let e2 = edge(c, a);
        let zero = _mm256_setzero_ps();
        let pos = _mm256_and_ps(
            _mm256_and_ps(
                _mm256_cmp_ps::<_CMP_GE_OQ>(e0, zero),
                _mm256_cmp_ps::<_CMP_GE_OQ>(e1, zero),
            ),
            _mm256_cmp_ps::<_CMP_GE_OQ>(e2, zero),
        );
        let neg = _mm256_and_ps(
            _mm256_and_ps(
                _mm256_cmp_ps::<_CMP_LE_OQ>(e0, zero),
                _mm256_cmp_ps::<_CMP_LE_OQ>(e1, zero),
            ),
            _mm256_cmp_ps::<_CMP_LE_OQ>(e2, zero),
        );
        let mask = _mm256_movemask_ps(_mm256_or_ps(pos, neg));
        if mask != 0 {
            let local = if mean > 1e-12 {
                let wa = _mm256_mul_ps(
                    _mm256_andnot_ps(_mm256_set1_ps(-0.0), e1),
                    _mm256_set1_ps(inv_area),
                );
                let wb = _mm256_mul_ps(
                    _mm256_andnot_ps(_mm256_set1_ps(-0.0), e2),
                    _mm256_set1_ps(inv_area),
                );
                let wc = _mm256_max_ps(
                    _mm256_sub_ps(_mm256_sub_ps(_mm256_set1_ps(1.0), wa), wb),
                    zero,
                );
                _mm256_div_ps(
                    _mm256_add_ps(
                        _mm256_add_ps(
                            _mm256_mul_ps(wa, _mm256_set1_ps(a.energy)),
                            _mm256_mul_ps(wb, _mm256_set1_ps(b.energy)),
                        ),
                        _mm256_mul_ps(wc, _mm256_set1_ps(c.energy)),
                    ),
                    _mm256_set1_ps(mean),
                )
            } else {
                _mm256_set1_ps(1.0)
            };
            let mut values = [0.0; 8];
            _mm256_storeu_ps(
                values.as_mut_ptr(),
                _mm256_mul_ps(_mm256_set1_ps(density), local),
            );
            for lane in 0..8 {
                if mask & (1 << lane) != 0 {
                    row[x + lane][ch] += values[lane];
                }
            }
        }
        x += 8;
    }
    x
}

fn avx_available() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        std::is_x86_feature_detected!("avx")
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

impl PreparedTriangle {
    fn new((ch, a, b, c): Triangle, w: usize, h: usize) -> Option<Self> {
        let signed = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
        let area = signed.abs() * 0.5;
        let edge_ab = ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt();
        let edge_bc = ((c.x - b.x).powi(2) + (c.y - b.y).powi(2)).sqrt();
        let edge_ca = ((a.x - c.x).powi(2) + (a.y - c.y).powi(2)).sqrt();
        let longest = edge_ab.max(edge_bc).max(edge_ca);
        let diagonal = ((w * w + h * h) as f32).sqrt();
        let total = (a.energy + b.energy + c.energy) / 6.0;
        if !area.is_finite()
            || !longest.is_finite()
            || longest > diagonal * 0.16
            || area > (w * h) as f32 * 0.018
            || (longest > 8.0 && area / (longest * longest) < 0.0015)
        {
            return None;
        }
        if area < 0.75 {
            return Some(Self::Tent {
                ch,
                x: (a.x + b.x + c.x) / 3.0,
                y: (a.y + b.y + c.y) / 3.0,
                total,
            });
        }
        let minx = a.x.min(b.x).min(c.x).floor().max(0.0) as usize;
        let maxx = a.x.max(b.x).max(c.x).ceil().min((w - 1) as f32) as usize;
        let miny = a.y.min(b.y).min(c.y).floor().max(0.0) as usize;
        let maxy = a.y.max(b.y).max(c.y).ceil().min((h - 1) as f32) as usize;
        if minx > maxx || miny > maxy {
            return None;
        }
        Some(Self::Area {
            ch,
            a,
            b,
            c,
            minx,
            maxx,
            miny,
            maxy,
            inv_area: 1.0 / signed.abs().max(1e-8),
            density: total / area.max(1.0),
            mean: (a.energy + b.energy + c.energy) / 3.0,
        })
    }

    fn draw(&self, buf: &mut [[f32; 3]], w: usize, h: usize, row_start: usize, use_avx: bool) {
        match *self {
            Self::Tent { ch, x, y, total } => tent_rows(buf, w, h, x, y, ch, total, 1.5, row_start),
            Self::Area {
                ch,
                a,
                b,
                c,
                minx,
                maxx,
                miny,
                maxy,
                inv_area,
                density,
                mean,
            } => {
                let edge = |p: (f32, f32), q: (f32, f32), x: f32, y: f32| {
                    (x - p.0) * (q.1 - p.1) - (y - p.1) * (q.0 - p.0)
                };
                for y in miny.max(row_start)..=maxy.min(row_start + buf.len() / w - 1) {
                    let mut x = minx;
                    #[cfg(target_arch = "x86_64")]
                    if use_avx && x + 7 <= maxx {
                        // Caller gates this path with runtime AVX detection.
                        x = unsafe {
                            raster_row_avx(
                                &mut buf[(y - row_start) * w..(y - row_start + 1) * w],
                                ch,
                                a,
                                b,
                                c,
                                x,
                                maxx,
                                y,
                                inv_area,
                                density,
                                mean,
                            )
                        };
                    }
                    #[cfg(target_arch = "x86_64")]
                    while x + 3 <= maxx {
                        let (values, mask) = raster_quad(a, b, c, x, y, inv_area, density, mean);
                        for lane in 0..4 {
                            if mask & (1 << lane) != 0 {
                                buf[(y - row_start) * w + x + lane][ch] += values[lane];
                            }
                        }
                        x += 4;
                    }
                    for x in x..=maxx {
                        let px = x as f32 + 0.5;
                        let py = y as f32 + 0.5;
                        let e0 = edge((a.x, a.y), (b.x, b.y), px, py);
                        let e1 = edge((b.x, b.y), (c.x, c.y), px, py);
                        let e2 = edge((c.x, c.y), (a.x, a.y), px, py);
                        if (e0 >= 0.0 && e1 >= 0.0 && e2 >= 0.0)
                            || (e0 <= 0.0 && e1 <= 0.0 && e2 <= 0.0)
                        {
                            let wa = e1.abs() * inv_area;
                            let wb = e2.abs() * inv_area;
                            let wc = (1.0 - wa - wb).max(0.0);
                            let local = if mean > 1e-12 {
                                (wa * a.energy + wb * b.energy + wc * c.energy) / mean
                            } else {
                                1.0
                            };
                            buf[(y - row_start) * w + x][ch] += density * local;
                        }
                    }
                }
            }
        }
    }
}

fn draw_triangles(
    out: &mut [[f32; 3]],
    triangles: &[Triangle],
    w: usize,
    h: usize,
    workers: usize,
) {
    if triangles.is_empty() {
        return;
    }
    let prepared: Vec<_> = triangles
        .iter()
        .copied()
        .filter_map(|t| PreparedTriangle::new(t, w, h))
        .collect();
    let use_avx = avx_available();
    let draw = |buf: &mut [[f32; 3]], row_start| {
        for triangle in &prepared {
            triangle.draw(buf, w, h, row_start, use_avx);
        }
    };
    if workers > 1 && w * h >= 262144 {
        // Distribute short horizontal strips round-robin: a bright centre should
        // not leave the top/bottom workers idle while the middle workers finish.
        let worker_count = workers.min(4);
        let rows = 32;
        let mut strips: Vec<Vec<_>> = (0..worker_count).map(|_| Vec::new()).collect();
        for (index, chunk) in out.chunks_mut(rows * w).enumerate() {
            strips[index % worker_count].push((index * rows, chunk));
        }
        std::thread::scope(|scope| {
            for owned in strips {
                let draw = &draw;
                scope.spawn(move || {
                    for (start, chunk) in owned {
                        draw(chunk, start);
                    }
                });
            }
        });
    } else {
        draw(out, 0);
    }
}

fn render_with_workers(
    l: &LensSystem,
    sources: &[BrightSource],
    w: usize,
    h: usize,
    cfg: Config,
    workers: usize,
) -> Vec<[f32; 3]> {
    let mut out = vec![[0.0; 3]; w * h];
    if sources.is_empty() {
        return out;
    }
    let fov_h = 50_f64.to_radians();
    let fov_v = 2.0 * ((h as f64 / w as f64) * (fov_h * 0.5).tan()).atan();
    let sw = l.focal_length * (fov_h * 0.5).tan();
    let sh = l.focal_length * (fov_v * 0.5).tan();
    let n = cfg.ray_grid.clamp(4, 128);
    let mut grid = vec![None; n * n];
    let mut valid_samples = 0usize;
    for gy in 0..n {
        for gx in 0..n {
            // Stable sub-cell jitter avoids visible ray-grid banding while keeping
            // identical frames bit-reproducible for AE's threaded renderer.
            let hash = |x: usize, y: usize, salt: u64| {
                let mut z = (x as u64).wrapping_mul(0x9E3779B185EBCA87)
                    ^ (y as u64).wrapping_mul(0xC2B2AE3D27D4EB4F)
                    ^ salt;
                z ^= z >> 30;
                z = z.wrapping_mul(0xBF58476D1CE4E5B9);
                z ^= z >> 27;
                z = z.wrapping_mul(0x94D049BB133111EB);
                ((z ^ (z >> 31)) as f64 / u64::MAX as f64) - 0.5
            };
            // A coherent pupil grid is required for surface reconstruction.
            // Jitter is retained only by the legacy point-splat path.
            let jitter = if cfg.surface_raster { 0.0 } else { 0.7 };
            let u = ((gx as f64 + 0.5 + hash(gx, gy, 17) * jitter) / n as f64) * 2. - 1.0;
            let v = ((gy as f64 + 0.5 + hash(gx, gy, 43) * jitter) / n as f64) * 2. - 1.0;
            if u * u + v * v <= 1. {
                grid[gy * n + gx] = Some((u, v));
                valid_samples += 1;
            }
        }
    }
    let rw = 1. / valid_samples.max(1) as f32;
    // Frame-local only: reuse the first reflection across different second
    // surfaces, including failed rays. Bound cached pupil states to 16 MiB.
    let mut first_legs: std::collections::HashMap<(usize, usize), Vec<[Option<TraceState>; 3]>> =
        std::collections::HashMap::new();
    let entry_bytes = n * n * std::mem::size_of::<[Option<TraceState>; 3]>();
    let mut triangles = Vec::new();
    for (a, b) in pairs(l) {
        if pair_estimate(l, a, b) < cfg.min_ghost {
            continue;
        }
        let boost = if cfg.normalize {
            area_boost(l, a, b, sw, sh, cfg.max_area_boost)
        } else {
            1.0
        };
        for (source_index, src) in sources.iter().enumerate() {
            let d = V3 {
                x: src.angle_x.tan(),
                y: src.angle_y.tan(),
                z: 1.0,
            }
            .norm();
            let cached = if b >= 2
                && (first_legs.contains_key(&(source_index, b))
                    || (first_legs.len() + 1) * entry_bytes <= 16 * 1024 * 1024)
            {
                Some(first_legs.entry((source_index, b)).or_insert_with(|| {
                    grid.iter()
                        .map(|pupil| {
                            let Some((u, v)) = *pupil else {
                                return [None; 3];
                            };
                            let ray = Ray {
                                o: V3 {
                                    x: u * l.surfaces[0].semi_aperture,
                                    y: v * l.surfaces[0].semi_aperture,
                                    z: l.surfaces[0].z - 20.0,
                                },
                                d,
                            };
                            [650.0, 550.0, 450.0].map(|wave| trace_first(ray, l, b, wave))
                        })
                        .collect()
                }))
            } else {
                None
            };
            let mut samples = vec![[None; 3]; n * n];
            let mut point_hits: Vec<(f32, f32, usize, f32)> = Vec::new();
            let mut minx = f32::MAX;
            let mut maxx = f32::MIN;
            let mut miny = f32::MAX;
            let mut maxy = f32::MIN;
            let mut gh = 0;
            for (grid_index, (sample, pupil)) in samples.iter_mut().zip(&grid).enumerate() {
                let Some((u, v)) = *pupil else { continue };
                let ray = Ray {
                    o: V3 {
                        x: u * l.surfaces[0].semi_aperture,
                        y: v * l.surfaces[0].semi_aperture,
                        z: l.surfaces[0].z - 20.0,
                    },
                    d,
                };
                for (ch, wave) in [650.0, 550.0, 450.].into_iter().enumerate() {
                    let hit = if let Some(states) = &cached {
                        states[grid_index][ch]
                            .and_then(|state| trace_remaining(state, l, a, b, wave))
                    } else {
                        trace(ray, l, a, b, wave)
                    };
                    if let Some(hit) = hit {
                        let px = ((hit.p.x / (2. * sw) + 0.5) * w as f64) as f32;
                        let py = ((hit.p.y / (2. * sh) + 0.5) * h as f64) as f32;
                        let val = src.rgb[ch] * hit.w as f32 * rw * cfg.gain * boost;
                        if val > 1e-12 {
                            sample[ch] = Some(SensorSample {
                                x: px,
                                y: py,
                                energy: val,
                            });
                        }
                    }
                }
            }
            if !cfg.surface_raster {
                for sample in &samples {
                    for (ch, hit) in sample.iter().enumerate() {
                        if let Some(hit) = hit {
                            point_hits.push((hit.x, hit.y, ch, hit.energy));
                            if ch == 1 {
                                minx = minx.min(hit.x);
                                maxx = maxx.max(hit.x);
                                miny = miny.min(hit.y);
                                maxy = maxy.max(hit.y);
                                gh += 1;
                            }
                        }
                    }
                }
            }
            if cfg.surface_raster {
                for gy in 0..n - 1 {
                    for gx in 0..n - 1 {
                        let i00 = gy * n + gx;
                        let i10 = i00 + 1;
                        let i01 = i00 + n;
                        let i11 = i01 + 1;
                        for ch in 0..3 {
                            if let (Some(a), Some(b), Some(c)) =
                                (samples[i00][ch], samples[i10][ch], samples[i11][ch])
                            {
                                triangles.push((ch, a, b, c));
                            }
                            if let (Some(a), Some(b), Some(c)) =
                                (samples[i00][ch], samples[i11][ch], samples[i01][ch])
                            {
                                triangles.push((ch, a, b, c));
                            }
                        }
                    }
                }
                // Batch ordered triangles instead of starting workers per ghost.
                // At most one pupil mesh is added beyond this bounded batch.
                if triangles.len() >= 32768 || workers == 1 {
                    draw_triangles(&mut out, &triangles, w, h, workers);
                    triangles.clear();
                }
            } else {
                let radius = if gh >= 4 {
                    ((maxx - minx).max(maxy - miny).max(1.0) / (gh as f32).sqrt() * 1.2)
                        .clamp(1.5, 80.0)
                } else {
                    1.5
                };
                for (px, py, ch, val) in point_hits {
                    tent(&mut out, w, h, px, py, ch, val, radius)
                }
            }
        }
    }
    draw_triangles(&mut out, &triangles, w, h, workers);
    if cfg.ghost_blur > 0. {
        let r = (cfg.ghost_blur * ((w * w + h * h) as f32).sqrt()) as usize;
        blur(&mut out, w, h, r, cfg.ghost_blur_passes)
    }
    out
}

fn blur(img: &mut Vec<[f32; 3]>, w: usize, h: usize, r: usize, passes: usize) {
    if r == 0 {
        return;
    }
    let mut tmp = vec![[0.0; 3]; img.len()];
    let mut row_prefix = vec![[0.0; 3]; w + 1];
    // Process adjacent columns together so the vertical pass touches nearby
    // cache lines. Each column retains precisely the original addition order.
    const TILE: usize = 32;
    let mut column_prefix = vec![[0.0; 3]; (h + 1) * TILE];
    for _ in 0..passes.max(1) {
        for y in 0..h {
            let prefix = &mut row_prefix;
            let mut sum = [0.0; 3];
            for (dst, pixel) in prefix[1..].iter_mut().zip(&img[y * w..(y + 1) * w]) {
                for c in 0..3 {
                    sum[c] += pixel[c];
                }
                *dst = sum;
            }
            let row = &mut tmp[y * w..(y + 1) * w];
            // Interior windows have constant width and contiguous prefix inputs.
            // Flatten RGB so LLVM can vectorize subtraction and exact division.
            let first = r.min(w);
            let last = w.saturating_sub(r).max(first);
            if first < last {
                let upper = prefix[first + r + 1..last + r + 1].as_flattened();
                let lower = prefix[first - r..last - r].as_flattened();
                let divisor = (2 * r + 1) as f32;
                for ((dst, a), b) in row[first..last]
                    .as_flattened_mut()
                    .iter_mut()
                    .zip(upper)
                    .zip(lower)
                {
                    *dst = (*a - *b) / divisor;
                }
            }
            for x in (0..first).chain(last..w) {
                let lo = x.saturating_sub(r);
                let hi = (x + r + 1).min(w);
                for c in 0..3 {
                    row[x][c] = (prefix[hi][c] - prefix[lo][c]) / (hi - lo) as f32;
                }
            }
        }
        for start in (0..w).step_by(TILE) {
            let count = (w - start).min(TILE);
            let prefix = &mut column_prefix;
            for y in 0..h {
                let (previous, next) = prefix.split_at_mut((y + 1) * TILE);
                let previous = previous[y * TILE..y * TILE + count].as_flattened();
                let next = next[..count].as_flattened_mut();
                let source = tmp[y * w + start..y * w + start + count].as_flattened();
                for ((dst, a), b) in next.iter_mut().zip(previous).zip(source) {
                    *dst = *a + *b;
                }
            }
            for y in 0..h {
                let lo = y.saturating_sub(r);
                let hi = (y + r + 1).min(h);
                let upper = prefix[hi * TILE..hi * TILE + count].as_flattened();
                let lower = prefix[lo * TILE..lo * TILE + count].as_flattened();
                let output = img[y * w + start..y * w + start + count].as_flattened_mut();
                let divisor = (hi - lo) as f32;
                for ((dst, a), b) in output.iter_mut().zip(upper).zip(lower) {
                    *dst = (*a - *b) / divisor;
                }
            }
        }
    }
}

// Only omit provably zero support. Keep full-image edge normalization and the
// original prefix addition order; never crop a nonzero halo by a threshold.
type Support = Option<(usize, usize, usize, usize)>;

fn bloom_support(img: &[[f32; 3]], w: usize, h: usize) -> Support {
    if w == 0 || h == 0 {
        return Some((0, 0, 0, 0));
    }
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    let safe_max = f32::MAX / (2.0 * img.len().max(1) as f32);
    for (i, p) in img.iter().enumerate() {
        if p.iter()
            .any(|v| !v.is_finite() || v.is_sign_negative() || *v > safe_max)
        {
            return None;
        }
        if p.iter().any(|v| *v != 0.0) {
            x0 = x0.min(i % w);
            x1 = x1.max(i % w + 1);
            y0 = y0.min(i / w);
            y1 = y1.max(i / w + 1);
        }
    }
    Some((x0, y0, x1, y1))
}

#[cfg(test)]
fn sparse_bloom_blur(img: &mut Vec<[f32; 3]>, w: usize, h: usize, r: usize, passes: usize) {
    let support = bloom_support(img, w, h);
    sparse_bloom_blur_known(img, w, h, r, passes, support);
}

fn sparse_bloom_blur_known(
    img: &mut Vec<[f32; 3]>,
    w: usize,
    h: usize,
    r: usize,
    passes: usize,
    support: Support,
) -> Support {
    if r == 0 || w == 0 || h == 0 {
        return support;
    }
    let Some((mut x0, mut y0, mut x1, mut y1)) = support else {
        blur(img, w, h, r, passes);
        return None;
    };
    if x1 == 0 {
        img.fill([0.0; 3]);
        return Some((0, 0, 0, 0));
    }
    let mut tmp = vec![[0.0; 3]; img.len()];
    let mut row_prefix = vec![[0.0; 3]; w + 1];
    const TILE: usize = 32;
    let mut prefix = vec![[0.0; 3]; (h + 1) * TILE];
    for _ in 0..passes.max(1) {
        let nx0 = x0.saturating_sub(r);
        let nx1 = x1.saturating_add(r).min(w);
        let ny0 = y0.saturating_sub(r);
        let ny1 = y1.saturating_add(r).min(h);
        for y in y0..y1 {
            row_prefix[x0] = [0.0; 3];
            for x in x0..x1 {
                for c in 0..3 {
                    row_prefix[x + 1][c] = row_prefix[x][c] + img[y * w + x][c];
                }
            }
            for x in nx0..nx1 {
                let lo = x.saturating_sub(r);
                let hi = x.saturating_add(r).saturating_add(1).min(w);
                for c in 0..3 {
                    tmp[y * w + x][c] =
                        (row_prefix[hi.min(x1)][c] - row_prefix[lo.max(x0)][c]) / (hi - lo) as f32;
                }
            }
        }
        for start in (nx0..nx1).step_by(TILE) {
            let count = (nx1 - start).min(TILE);
            prefix[y0 * TILE..y0 * TILE + count].fill([0.0; 3]);
            for y in y0..y1 {
                let (prev, next) = prefix.split_at_mut((y + 1) * TILE);
                for ((dst, a), b) in next[..count]
                    .as_flattened_mut()
                    .iter_mut()
                    .zip(prev[y * TILE..y * TILE + count].as_flattened())
                    .zip(tmp[y * w + start..y * w + start + count].as_flattened())
                {
                    *dst = *a + *b;
                }
            }
            for y in ny0..ny1 {
                let lo = y.saturating_sub(r);
                let hi = y.saturating_add(r).saturating_add(1).min(h);
                let upper = hi.min(y1) * TILE;
                let lower = lo.max(y0) * TILE;
                for ((dst, a), b) in img[y * w + start..y * w + start + count]
                    .as_flattened_mut()
                    .iter_mut()
                    .zip(prefix[upper..upper + count].as_flattened())
                    .zip(prefix[lower..lower + count].as_flattened())
                {
                    *dst = (*a - *b) / (hi - lo) as f32;
                }
            }
        }
        (x0, y0, x1, y1) = (nx0, ny0, nx1, ny1);
    }
    Some((x0, y0, x1, y1))
}

pub fn source_layer(sources: &[BrightSource], w: usize, h: usize) -> Vec<[f32; 3]> {
    let mut out = vec![[0.0; 3]; w * h];
    let fov_h = 50_f64.to_radians();
    let fov_v = 2.0 * ((h as f64 / w as f64) * (fov_h * 0.5).tan()).atan();
    for source in sources {
        let x = ((source.angle_x / fov_h + 0.5) * w as f64).round() as isize;
        let y = ((source.angle_y / fov_v + 0.5) * h as f64).round() as isize;
        if x >= 0 && y >= 0 && x < w as isize && y < h as isize {
            let pixel = &mut out[y as usize * w + x as usize];
            for c in 0..3 {
                pixel[c] += source.rgb[c];
            }
        }
    }
    out
}

pub fn bloom(input: &[[f32; 3]], w: usize, h: usize, cfg: Config) -> Vec<[f32; 3]> {
    let workers = std::thread::available_parallelism().map_or(1, |n| n.get().min(2));
    // Bound extra full-frame scratch space; large frames retain serial execution.
    let workers = if w.saturating_mul(h) <= 2_097_152 {
        workers
    } else {
        1
    };
    bloom_with_workers(input, w, h, cfg, workers)
}

fn bloom_with_workers(
    input: &[[f32; 3]],
    w: usize,
    h: usize,
    cfg: Config,
    workers: usize,
) -> Vec<[f32; 3]> {
    let mut out = vec![[0.0; 3]; w * h];
    if cfg.bloom_strength <= 0. {
        return out;
    }
    let diag = ((w * w + h * h) as f32).sqrt();
    let chroma = [
        [1.0, 1.0, 1.],
        [1.0, 0.88, 0.60],
        [1.0, 0.62, 0.28],
        [1.0, 0.35, 0.10],
        [0.9, 0.16, 0.04],
        [0.7, 0.06, 0.01],
    ];
    let support = bloom_support(input, w, h);
    let make_layer = |o: usize| {
        let mut layer = input.to_vec();
        let radius = (cfg.bloom_radius * diag * 2_f32.powi(o as i32)).max(1.0) as usize;
        let final_support =
            sparse_bloom_blur_known(&mut layer, w, h, radius, cfg.bloom_passes, support);
        (layer, final_support)
    };
    let mut accumulate = |o: usize, (layer, final_support): (Vec<[f32; 3]>, Support)| {
        let wt = 1. / 2_f32.powi(o as i32);
        let tint = if cfg.bloom_chromatic {
            chroma[o.min(5)]
        } else {
            [1.0; 3]
        };
        let (x0, y0, x1, y1) = if cfg.bloom_strength.is_finite() {
            final_support.unwrap_or((0, 0, w, h))
        } else {
            (0, 0, w, h)
        };
        for y in y0..y1 {
            for i in y * w + x0..y * w + x1 {
                for c in 0..3 {
                    out[i][c] += layer[i][c] * cfg.bloom_strength * wt * tint[c]
                }
            }
        }
    };
    let octaves = cfg.bloom_octaves.clamp(1, 6);
    let workers = workers.clamp(1, 4);
    for first in (0..octaves).step_by(workers) {
        if workers > 1 && first + 1 < octaves {
            std::thread::scope(|scope| {
                let jobs: Vec<_> = (first + 1..(first + workers).min(octaves))
                    .map(|o| {
                        let make_layer = &make_layer;
                        scope.spawn(move || make_layer(o))
                    })
                    .collect();
                accumulate(first, make_layer(first));
                // Keep the original octave accumulation order, including rounding.
                for (index, job) in jobs.into_iter().enumerate() {
                    accumulate(first + index + 1, job.join().expect("bloom octave worker"));
                }
            });
        } else {
            accumulate(first, make_layer(first));
        }
    }
    out
}

pub fn extract_sources(
    src: &[u8],
    w: usize,
    h: usize,
    threshold: f32,
    downsample: usize,
) -> Vec<BrightSource> {
    let ds = downsample.max(1);
    let fov_h = 50_f64.to_radians();
    let fov_v = 2.0 * ((h as f64 / w as f64) * (fov_h * 0.5).tan()).atan();
    let mut out = Vec::new();
    for y in (0..h).step_by(ds) {
        for x in (0..w).step_by(ds) {
            let mut rgb = [0.0; 3];
            let mut n = 0.0;
            for yy in y..(y + ds).min(h) {
                for xx in x..(x + ds).min(w) {
                    let o = (yy * w + xx) * 4;
                    for c in 0..3 {
                        rgb[c] += src[o + 1 + c] as f32 / 255.0
                    }
                    n += 1.0
                }
            }
            for c in 0..3 {
                rgb[c] /= n
            }
            let lum = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
            if lum > threshold {
                let excess = (lum - threshold) / lum.max(1e-6);
                for c in 0..3 {
                    rgb[c] *= excess
                }
                out.push(BrightSource {
                    angle_x: ((x as f64 + 0.5) / w as f64 - 0.5) * fov_h,
                    angle_y: ((y as f64 + 0.5) / h as f64 - 0.5) * fov_v,
                    rgb,
                })
            }
        }
    }
    out
}

/// Keep only strong, spatially distinct highlights. Without this reduction a
/// bright plate can create tens of thousands of physical light sources.
pub fn prune_sources(mut sources: Vec<BrightSource>, max_sources: usize) -> Vec<BrightSource> {
    sources.sort_by(|a, b| {
        let ea = 0.2126 * a.rgb[0] + 0.7152 * a.rgb[1] + 0.0722 * a.rgb[2];
        let eb = 0.2126 * b.rgb[0] + 0.7152 * b.rgb[1] + 0.0722 * b.rgb[2];
        eb.total_cmp(&ea)
    });
    let mut selected: Vec<BrightSource> = Vec::with_capacity(max_sources);
    for source in sources {
        let distinct = selected.iter().all(|kept| {
            let dx = source.angle_x - kept.angle_x;
            let dy = source.angle_y - kept.angle_y;
            dx * dx + dy * dy > 0.035_f64.powi(2)
        });
        if distinct {
            selected.push(source);
            if selected.len() >= max_sources {
                break;
            }
        }
    }
    selected
}

pub fn bundled_lens(index: usize) -> LensSystem {
    let text = match index {
        1 => include_str!("../lenses/doublegauss.lens"),
        2 => include_str!("../lenses/arri-zeiss-master-prime-t1.3-50mm.lens"),
        3 => include_str!("../lenses/canon-ef-200-400-f4.lens"),
        _ => include_str!("../lenses/cooketriplet.lens"),
    };
    LensSystem::parse(text).expect("bundled lens prescription must be valid")
}

#[cfg(test)]
mod tests {
    use super::*;
    // Reference implementation retained to detect changes to summation,
    // clipping at image edges, and multi-pass rounding.
    fn reference_blur(img: &mut Vec<[f32; 3]>, w: usize, h: usize, r: usize, passes: usize) {
        if r == 0 {
            return;
        }
        let mut tmp = img.clone();
        for _ in 0..passes.max(1) {
            for y in 0..h {
                let mut prefix = vec![[0.0; 3]; w + 1];
                for x in 0..w {
                    for c in 0..3 {
                        prefix[x + 1][c] = prefix[x][c] + img[y * w + x][c];
                    }
                }
                for x in 0..w {
                    let lo = x.saturating_sub(r);
                    let hi = (x + r + 1).min(w);
                    for c in 0..3 {
                        tmp[y * w + x][c] = (prefix[hi][c] - prefix[lo][c]) / (hi - lo) as f32;
                    }
                }
            }
            for x in 0..w {
                let mut prefix = vec![[0.0; 3]; h + 1];
                for y in 0..h {
                    for c in 0..3 {
                        prefix[y + 1][c] = prefix[y][c] + tmp[y * w + x][c];
                    }
                }
                for y in 0..h {
                    let lo = y.saturating_sub(r);
                    let hi = (y + r + 1).min(h);
                    for c in 0..3 {
                        img[y * w + x][c] = (prefix[hi][c] - prefix[lo][c]) / (hi - lo) as f32;
                    }
                }
            }
        }
    }

    #[test]
    fn tiled_blur_is_bit_exact() {
        for (w, h) in [
            (1, 1),
            (1, 37),
            (37, 1),
            (31, 17),
            (32, 18),
            (33, 19),
            (97, 61),
        ] {
            let input: Vec<_> = (0..w * h)
                .map(|i| {
                    [
                        (i % 17) as f32 * 0.13,
                        if i % 23 == 0 { 1000.0 } else { 0.0001 },
                        (i % 11) as f32 * -0.07,
                    ]
                })
                .collect();
            for radius in [0, 1, 5, 128] {
                for passes in [0, 1, 3] {
                    let mut expected = input.clone();
                    let mut actual = input.clone();
                    reference_blur(&mut expected, w, h, radius, passes);
                    blur(&mut actual, w, h, radius, passes);
                    assert!(
                        actual
                            .iter()
                            .flatten()
                            .zip(expected.iter().flatten())
                            .all(|(a, b)| a.to_bits() == b.to_bits()),
                        "{w}x{h}, r={radius}, passes={passes}"
                    );
                }
            }
        }
    }

    #[test]
    #[ignore = "manual release performance measurement"]
    fn benchmark_blur() {
        let (w, h) = (1920, 1080);
        let input: Vec<_> = (0..w * h).map(|i| [(i % 17) as f32 * 0.13; 3]).collect();
        for run in 0..4 {
            let mut reference = input.clone();
            let start = std::time::Instant::now();
            reference_blur(&mut reference, w, h, 40, 3);
            let old = start.elapsed();
            let mut optimized = input.clone();
            let start = std::time::Instant::now();
            blur(&mut optimized, w, h, 40, 3);
            let new = start.elapsed();
            assert_eq!(reference, optimized);
            eprintln!("blur run {run}: old={old:?}, new={new:?}");
        }
    }

    #[test]
    #[ignore = "1080p dense versus sparse blur benchmark; writes black test input"]
    fn benchmark_sparse_1080p() {
        let (w, h) = (1920, 1080);
        image::RgbImage::new(w as u32, h as u32)
            .save("target/aexcompat-regression/pure-black-1920x1080.png")
            .unwrap();
        let mut input = vec![[0.0; 3]; w * h];
        input[465 * w + 690] = [1.0; 3];
        for run in 0..3 {
            let mut dense_time = std::time::Duration::ZERO;
            let mut sparse_time = std::time::Duration::ZERO;
            for r in [39, 79, 158, 317] {
                let mut dense = input.clone();
                let mut sparse = input.clone();
                let start = std::time::Instant::now();
                blur(&mut dense, w, h, r, 3);
                dense_time += start.elapsed();
                let start = std::time::Instant::now();
                sparse_bloom_blur(&mut sparse, w, h, r, 3);
                sparse_time += start.elapsed();
                assert!(dense
                    .iter()
                    .flatten()
                    .zip(sparse.iter().flatten())
                    .all(|(a, b)| a.to_bits() == b.to_bits()));
            }
            eprintln!("1080p four blur layers {run}: dense={dense_time:?}, sparse={sparse_time:?}");
        }
    }

    #[test]
    fn bloom_support_preserves_exceptional_values() {
        let (w, h) = (37, 19);
        for value in [0.0, -0.0, 1.0, -1.0, f32::MAX, f32::INFINITY, f32::NAN] {
            for strength in [0.6, f32::INFINITY, f32::NAN] {
                let mut input = vec![[0.0; 3]; w * h];
                input[9 * w + 17] = [value; 3];
                let cfg = Config {
                    bloom_strength: strength,
                    bloom_radius: 0.03,
                    bloom_octaves: 3,
                    bloom_chromatic: false,
                    ..Config::default()
                };
                let mut expected = vec![[0.0; 3]; w * h];
                for o in 0..3 {
                    let mut layer = input.clone();
                    let radius =
                        (cfg.bloom_radius * ((w * w + h * h) as f32).sqrt() * 2_f32.powi(o))
                            .max(1.0) as usize;
                    reference_blur(&mut layer, w, h, radius, cfg.bloom_passes);
                    for i in 0..layer.len() {
                        for c in 0..3 {
                            expected[i][c] += layer[i][c] * strength * (1.0 / 2_f32.powi(o)) * 1.0;
                        }
                    }
                }
                let actual = bloom(&input, w, h, cfg);
                assert!(
                    expected
                        .iter()
                        .flatten()
                        .zip(actual.iter().flatten())
                        .all(|(a, b)| a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())),
                    "value={value} strength={strength}"
                );
            }
        }
    }

    #[test]
    fn shared_first_reflection_matches_trace() {
        for preset in 0..4 {
            let l = bundled_lens(preset);
            for (a, b) in pairs(&l) {
                for wave in [450.0, 550.0, 650.0] {
                    for gx in -3..=3 {
                        let ray = Ray {
                            o: V3 {
                                x: gx as f64 / 4.0 * l.surfaces[0].semi_aperture,
                                y: 0.0,
                                z: l.surfaces[0].z - 20.0,
                            },
                            d: V3 {
                                x: 0.13,
                                y: -0.04,
                                z: 1.0,
                            }
                            .norm(),
                        };
                        let original = trace(ray, &l, a, b, wave);
                        let shared = trace_first(ray, &l, b, wave)
                            .and_then(|s| trace_remaining(s, &l, a, b, wave));
                        match (original, shared) {
                            (None, None) => {}
                            (Some(a), Some(b)) => assert_eq!(
                                [
                                    a.p.x.to_bits(),
                                    a.p.y.to_bits(),
                                    a.p.z.to_bits(),
                                    a.w.to_bits()
                                ],
                                [
                                    b.p.x.to_bits(),
                                    b.p.y.to_bits(),
                                    b.p.z.to_bits(),
                                    b.w.to_bits()
                                ]
                            ),
                            _ => panic!("trace mismatch lens{preset} pair{a},{b}"),
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn prepared_triangles_match_original() {
        let (w, h) = (321, 205);
        let mut original = vec![[0.0; 3]; w * h];
        let mut prepared = original.clone();
        let mut sse = original.clone();
        for i in 0..600 {
            let x = (i * 47 % 401) as f32 - 40.0;
            let y = (i * 31 % 285) as f32 - 40.0;
            let size = if i % 3 == 0 { 0.2 } else { (i % 45) as f32 };
            let a = SensorSample { x, y, energy: 0.7 };
            let b = SensorSample {
                x: x + size,
                y: y + size * 0.12,
                energy: 1.3,
            };
            let c = SensorSample {
                x: x + size * 0.2,
                y: y + size * 0.8,
                energy: 0.002,
            };
            let (b, c) = if i % 2 == 0 { (c, b) } else { (b, c) };
            let ch = i % 3;
            raster_triangle(&mut original, w, h, ch, a, b, c, 0);
            if let Some(t) = PreparedTriangle::new((ch, a, b, c), w, h) {
                for (row, chunk) in sse.chunks_mut(w * 32).enumerate() {
                    t.draw(chunk, w, h, row * 32, false);
                }
                for (row, chunk) in prepared.chunks_mut(w * 32).enumerate() {
                    t.draw(chunk, w, h, row * 32, avx_available());
                }
            }
        }
        assert!(original
            .iter()
            .flatten()
            .zip(prepared.iter().flatten())
            .all(|(a, b)| a.to_bits() == b.to_bits()));
        assert!(original
            .iter()
            .flatten()
            .zip(sse.iter().flatten())
            .all(|(a, b)| a.to_bits() == b.to_bits()));
    }

    #[test]
    fn sparse_bloom_blur_matches_reference() {
        for (w, h) in [(1, 1), (1, 37), (37, 1), (97, 61), (641, 411)] {
            for radius in [1, 7, 40, 999] {
                for passes in [0, 1, 3] {
                    let mut input = vec![[0.0; 3]; w * h];
                    for i in [0, w * h / 3, w * h / 2, w * h - 1] {
                        input[i] = [1.0, 0.007, 23.1];
                    }
                    let mut expected = input.clone();
                    reference_blur(&mut expected, w, h, radius, passes);
                    sparse_bloom_blur(&mut input, w, h, radius, passes);
                    assert!(
                        expected
                            .iter()
                            .flatten()
                            .zip(input.iter().flatten())
                            .all(|(a, b)| a.to_bits() == b.to_bits()),
                        "{w}x{h} r{radius} p{passes}"
                    );
                    let mut centred = vec![[0.0; 3]; w * h];
                    centred[w * h / 2] = [1.0, 0.007, 23.1];
                    let mut expected = centred.clone();
                    reference_blur(&mut expected, w, h, radius, passes);
                    sparse_bloom_blur(&mut centred, w, h, radius, passes);
                    assert_eq!(expected, centred);
                }
            }
        }
    }

    #[test]
    fn parallel_raster_rows_are_bit_exact() {
        let sources = [
            BrightSource {
                angle_x: -0.13,
                angle_y: 0.04,
                rgb: [1.0, 0.8, 0.6],
            },
            BrightSource {
                angle_x: 0.07,
                angle_y: -0.03,
                rgb: [0.2, 0.5, 1.0],
            },
        ];
        for preset in 0..4 {
            let lens = bundled_lens(preset);
            for surface_raster in [false, true] {
                let cfg = Config {
                    ray_grid: 17,
                    surface_raster,
                    ..Config::default()
                };
                let serial = render_with_workers(&lens, &sources, 641, 411, cfg, 1);
                let parallel = render_with_workers(&lens, &sources, 641, 411, cfg, 4);
                assert!(
                    serial
                        .iter()
                        .flatten()
                        .zip(parallel.iter().flatten())
                        .all(|(a, b)| a.to_bits() == b.to_bits()),
                    "lens {preset}, surface {surface_raster}"
                );
            }
        }
    }

    #[test]
    fn parallel_bloom_is_bit_exact() {
        for (w, h) in [(1, 1), (1, 37), (37, 1), (97, 61)] {
            let input: Vec<_> = (0..w * h)
                .map(|i| [(i % 19) as f32 * 0.13 - 0.7, (i % 7) as f32 * 32.1, 0.01])
                .collect();
            for octaves in 1..=6 {
                for chromatic in [false, true] {
                    for passes in [0, 1, 3] {
                        let cfg = Config {
                            bloom_octaves: octaves,
                            bloom_chromatic: chromatic,
                            bloom_passes: passes,
                            ..Config::default()
                        };
                        let reference = bloom_with_workers(&input, w, h, cfg, 1);
                        let actual = bloom_with_workers(&input, w, h, cfg, 2);
                        assert!(reference
                            .iter()
                            .flatten()
                            .zip(actual.iter().flatten())
                            .all(|(a, b)| a.to_bits() == b.to_bits()));
                    }
                }
            }
        }
    }

    #[test]
    fn renders_nonempty_cooke_ghosts() {
        let l = LensSystem::parse(include_str!("../lenses/cooketriplet.lens")).unwrap();
        let c = Config {
            ray_grid: 8,
            gain: 1000.0,
            ghost_blur: 0.0,
            bloom_strength: 0.0,
            ..Config::default()
        };
        let img = render(
            &l,
            &[BrightSource {
                angle_x: 0.1,
                angle_y: 0.0,
                rgb: [1.0; 3],
            }],
            96,
            54,
            c,
        );
        assert!(img.iter().any(|p| p[0] + p[1] + p[2] > 0.0));
    }

    #[test]
    fn extracts_sources_and_bloom_spreads_energy() {
        let mut src = vec![0_u8; 32 * 18 * 4];
        let o = (9 * 32 + 12) * 4;
        src[o] = 255;
        src[o + 1] = 255;
        src[o + 2] = 240;
        src[o + 3] = 200;
        let sources = extract_sources(&src, 32, 18, 0.5, 1);
        assert_eq!(sources.len(), 1);
        let input = source_layer(&sources, 32, 18);
        let output = bloom(
            &input,
            32,
            18,
            Config {
                bloom_radius: 0.04,
                ..Config::default()
            },
        );
        assert!(output.iter().filter(|p| p[0] > 0.0).count() > 1);
    }

    #[test]
    #[ignore = "writes the user-facing four-lens comparison render"]
    fn write_comparison_preview() {
        let (pw, ph) = (320, 180);
        let mut canvas = vec![[0.0_f32; 3]; pw * 2 * ph * 2];
        for preset in 0..4 {
            let cfg = Config {
                ray_grid: 32,
                gain: 120_000.0,
                ghost_blur: 0.005,
                bloom_strength: 2.5,
                bloom_radius: 0.012,
                ..Config::default()
            };
            let sources = [BrightSource {
                angle_x: -0.16,
                angle_y: -0.07,
                rgb: [1.0, 0.92, 0.78],
            }];
            let ghosts = render(&bundled_lens(preset), &sources, pw, ph, cfg);
            let bloom_layer = bloom(&source_layer(&sources, pw, ph), pw, ph, cfg);
            let ox = (preset % 2) * pw;
            let oy = (preset / 2) * ph;
            for y in 0..ph {
                for x in 0..pw {
                    let i = y * pw + x;
                    let d = (y + oy) * pw * 2 + x + ox;
                    for c in 0..3 {
                        canvas[d][c] = ghosts[i][c] + bloom_layer[i][c];
                    }
                }
            }
        }
        let mut image =
            image::ImageBuffer::<image::Rgb<u8>, Vec<u8>>::new((pw * 2) as u32, (ph * 2) as u32);
        for (i, p) in canvas.into_iter().enumerate() {
            let mut rgb = [0u8; 3];
            for c in 0..3 {
                let mapped = (p[c].max(0.0) / (1.0 + p[c].max(0.0))).powf(1.0 / 2.2);
                rgb[c] = (mapped * 255.0).clamp(0.0, 255.0) as u8;
            }
            image.put_pixel(
                (i % (pw * 2)) as u32,
                (i / (pw * 2)) as u32,
                image::Rgb(rgb),
            );
        }
        image
            .save("onmkFlare_hullin_surface_comparison.png")
            .unwrap();
    }
}
