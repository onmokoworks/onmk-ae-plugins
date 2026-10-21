//! Independent expected-result oracle written directly from docs/design.md §4/§5
//! in f64. It deliberately shares no code with src/: no Ω domain (values are
//! evaluated lazily anywhere), direct 2-D structuring-element enumeration,
//! direct 2-D Gaussian convolution and a general bilinear sampler.
#![allow(dead_code)]

use inner_shadow::engine::{GpuRun, Image, Prepared};
use inner_shadow::spec::{Blend, Plan, Rect64, Scale, ShadowParams};
use std::collections::HashMap;

pub fn no_gpu(_: &Prepared, _: &Plan) -> Result<(Vec<[f32; 4]>, GpuRun), String> {
    Err("CPU-only test: GPU backend intentionally unavailable".to_string())
}

pub fn image(w: usize, h: usize, left: i64, top: i64, f: impl Fn(i64, i64) -> [f32; 4]) -> Image {
    let mut px = Vec::with_capacity(w * h);
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            px.push(f(left + x, top + y));
        }
    }
    Image::new(w, h, left, top, px).unwrap()
}

pub const HIDDEN: [f32; 4] = [0.9, 0.1, 0.7, 0.0];

/// Soft disc with a soft hole plus an explicit partial-alpha ladder row.
pub fn soft_fixture(left: i64, top: i64) -> Image {
    let (w, h) = (36usize, 28usize);
    image(w, h, left, top, |x, y| {
        let (lx, ly) = ((x - left) as f64, (y - top) as f64);
        if ly == 1.0 && (2.0..12.0).contains(&lx) {
            let ladder = [
                0.0,
                1.0 / 255.0,
                0.125,
                0.5,
                1.0,
                1.0 / 32768.0,
                0.25,
                0.75,
                0.999,
                0.001,
            ];
            return [1.0, 0.8666667, 0.2, ladder[(lx as usize) - 2]];
        }
        let (cx, cy) = (18.5, 14.0);
        let d = ((lx - cx).powi(2) + (ly - cy).powi(2)).sqrt();
        let outer = (11.0 - d).clamp(0.0, 1.0);
        let hole = ((lx - 21.0).powi(2) + (ly - 12.0).powi(2)).sqrt();
        let inner = (hole - 3.2).clamp(0.0, 1.0);
        let a = (outer * inner) as f32;
        if a == 0.0 {
            HIDDEN
        } else {
            [1.0, 0.8666667, 0.2, a]
        }
    })
}

fn coverage(a: f32) -> f64 {
    if a.is_nan() {
        0.0
    } else {
        (a as f64).clamp(0.0, 1.0)
    }
}

fn gaussian(sigma: f64) -> Vec<f64> {
    if sigma <= 1e-4 {
        return vec![1.0];
    }
    let r = ((3.0 * sigma - 1e-9).ceil().max(1.0)) as i64;
    let w: Vec<f64> = (-r..=r)
        .map(|i| (-((i * i) as f64) / (2.0 * sigma * sigma)).exp())
        .collect();
    let s: f64 = w.iter().sum();
    w.into_iter().map(|v| v / s).collect()
}

/// (i, j) offsets of the elliptical structuring element for buffer radii.
fn element(rx: f64, ry: f64) -> Vec<(i64, i64)> {
    let mut v = Vec::new();
    let ryi = (ry + 1e-9).floor() as i64;
    for j in -ryi..=ryi {
        let q = if ry > 0.0 {
            (1.0 - (j as f64 / ry).powi(2)).max(0.0)
        } else {
            1.0
        };
        let hw = (rx * q.sqrt() + 1e-6).floor() as i64;
        for i in -hw..=hw {
            v.push((i, j));
        }
    }
    v
}

pub struct Oracle<'a> {
    img: &'a Image,
    p: ShadowParams,
    o: (f64, f64),
    lo: Option<Vec<(i64, i64)>>,
    hi: Option<Vec<(i64, i64)>>,
    mix: f64,
    kx: Vec<f64>,
    ky: Vec<f64>,
    d_memo: HashMap<(i64, i64), f64>,
    b_memo: HashMap<(i64, i64), f64>,
}

impl<'a> Oracle<'a> {
    /// `p` must already be within the valid parameter ranges.
    pub fn new(img: &'a Image, p: ShadowParams, s: Scale) -> Self {
        let th = p.angle_deg.to_radians();
        let snap = |v: f64| (v * 65536.0).round() / 65536.0;
        let o = (
            snap(p.distance * -th.cos() * s.dsx / s.par),
            snap(p.distance * th.sin() * s.dsy),
        );
        let r_m = p.choke * p.size;
        let r_b = p.size - r_m;
        let el = |r: f64| {
            if r <= 0.0 {
                None
            } else {
                Some(element(r * s.dsx / s.par, r * s.dsy))
            }
        };
        let (lo, hi, mix) = if r_m > 0.0 {
            let r0 = r_m.floor();
            let f = r_m - r0;
            (el(r0), if f > 0.0 { el(r0 + 1.0) } else { None }, f)
        } else {
            (None, None, 0.0)
        };
        let sigma = r_b / 3.0;
        Oracle {
            img,
            p,
            o,
            lo,
            hi,
            mix,
            kx: gaussian(sigma * s.dsx / s.par),
            ky: gaussian(sigma * s.dsy),
            d_memo: HashMap::new(),
            b_memo: HashMap::new(),
        }
    }

    fn ext(&self, x: i64, y: i64) -> f64 {
        1.0 - self.img.get(x, y).map(|p| coverage(p[3])).unwrap_or(0.0)
    }

    fn dil(&self, el: &Option<Vec<(i64, i64)>>, x: i64, y: i64) -> f64 {
        match el {
            None => self.ext(x, y),
            Some(v) => v
                .iter()
                .map(|&(i, j)| self.ext(x + i, y + j))
                .fold(0.0, f64::max),
        }
    }

    fn d(&mut self, x: i64, y: i64) -> f64 {
        if let Some(v) = self.d_memo.get(&(x, y)) {
            return *v;
        }
        let lo = self.dil(&self.lo, x, y);
        let hi = self.dil(&self.hi, x, y);
        let v = lo + (hi - lo) * self.mix;
        self.d_memo.insert((x, y), v);
        v
    }

    fn b(&mut self, x: i64, y: i64) -> f64 {
        if let Some(v) = self.b_memo.get(&(x, y)) {
            return *v;
        }
        let (rx, ry) = ((self.kx.len() / 2) as i64, (self.ky.len() / 2) as i64);
        let (kx, ky) = (self.kx.clone(), self.ky.clone());
        let mut sum = 0.0;
        for (j, wy) in ky.iter().enumerate() {
            for (i, wx) in kx.iter().enumerate() {
                sum += wx * wy * self.d(x + i as i64 - rx, y + j as i64 - ry);
            }
        }
        self.b_memo.insert((x, y), sum);
        sum
    }

    /// Shadow amount t at output pixel p.
    pub fn t(&mut self, x: i64, y: i64) -> f64 {
        let (sx, sy) = (x as f64 - self.o.0, y as f64 - self.o.1);
        let (x0, y0) = (sx.floor(), sy.floor());
        let (fx, fy) = (sx - x0, sy - y0);
        let (x0, y0) = (x0 as i64, y0 as i64);
        let m = (1.0 - fx) * (1.0 - fy) * self.b(x0, y0)
            + fx * (1.0 - fy) * self.b(x0 + 1, y0)
            + (1.0 - fx) * fy * self.b(x0, y0 + 1)
            + fx * fy * self.b(x0 + 1, y0 + 1);
        self.p.opacity * m.clamp(0.0, 1.0)
    }

    pub fn pixel(&mut self, x: i64, y: i64) -> [f64; 4] {
        let src = self.img.get(x, y).unwrap_or([0.0; 4]);
        let s = src.map(|c| c as f64);
        if !(src[3] > 0.0) {
            return s;
        }
        let t = self.t(x, y);
        if t == 0.0 {
            return s;
        }
        let mut out = s;
        for c in 0..3 {
            if !src[c].is_finite() {
                continue;
            }
            let k = self.p.color[c] as f64;
            out[c] = match self.p.blend {
                Blend::Normal => s[c] + t * (k - s[c]),
                Blend::Multiply => s[c] * (1.0 - t * (1.0 - k)),
            };
        }
        out
    }

    pub fn render(&mut self, out: &Rect64) -> Vec<[f64; 4]> {
        let mut v = Vec::new();
        for y in out.top..out.bottom {
            for x in out.left..out.right {
                v.push(self.pixel(x, y));
            }
        }
        v
    }
}

/// Acceptance check against an independent expectation. Refuses fixtures
/// without partial alpha (opaque-only data is not alpha evidence).
pub fn check(actual: &[[f32; 4]], expected: &[[f64; 4]], tol: f64) -> Result<f64, String> {
    if actual.len() != expected.len() {
        return Err(format!("length {} != {}", actual.len(), expected.len()));
    }
    if !expected.iter().any(|p| p[3] > 0.01 && p[3] < 0.99) {
        return Err("fixture must contain partial alpha".into());
    }
    let mut max = 0.0f64;
    for (i, (a, e)) in actual.iter().zip(expected).enumerate() {
        for c in 0..4 {
            let (av, ev) = (a[c] as f64, e[c]);
            if !ev.is_finite() {
                if av.to_bits() != ev.to_bits() && !(av.is_nan() && ev.is_nan()) {
                    return Err(format!("pixel {i} channel {c}: {av} vs non-finite {ev}"));
                }
                continue;
            }
            let d = (av - ev).abs();
            max = max.max(d);
            if !(d <= tol) {
                return Err(format!(
                    "pixel {i} channel {c}: actual {av} expected {ev} (|d|={d:e} > {tol:e})"
                ));
            }
        }
    }
    Ok(max)
}

/// A deliberately wrong (premultiplied) variant of a straight result.
pub fn premultiplied(px: &[[f32; 4]]) -> Vec<[f32; 4]> {
    px.iter()
        .map(|p| [p[0] * p[3], p[1] * p[3], p[2] * p[3], p[3]])
        .collect()
}

pub fn valid_cases() -> Vec<ShadowParams> {
    let base = ShadowParams::default();
    vec![
        base,
        ShadowParams {
            blend: Blend::Normal,
            color: [1.0, 0.1, 0.0],
            opacity: 0.9,
            angle_deg: 37.0,
            distance: 3.3,
            choke: 0.3,
            size: 4.0,
        },
        ShadowParams {
            blend: Blend::Multiply,
            color: [0.0, 0.2, 1.0],
            opacity: 1.0,
            angle_deg: 225.0,
            distance: 0.75,
            choke: 0.8,
            size: 2.5,
        },
        ShadowParams {
            blend: Blend::Normal,
            color: [0.0, 0.0, 0.0],
            opacity: 0.6,
            angle_deg: 300.0,
            distance: 7.0,
            choke: 0.0,
            size: 0.0,
        },
        ShadowParams {
            blend: Blend::Normal,
            color: [0.0, 0.0, 0.0],
            opacity: 1.0,
            angle_deg: 90.0,
            distance: 0.0,
            choke: 0.0,
            size: 0.0,
        },
        ShadowParams {
            blend: Blend::Multiply,
            color: [0.3, 0.0, 0.5],
            opacity: 0.5,
            angle_deg: -10.0,
            distance: 12.0,
            choke: 0.5,
            size: 6.0,
        },
    ]
}

pub fn scales() -> Vec<Scale> {
    vec![
        Scale::FULL,
        Scale::new(0.5, 0.5, 1.0).unwrap(),
        Scale::new(1.0, 1.0, 2.0).unwrap(),
        Scale::new(1.0, 1.0, 0.9).unwrap(),
        Scale::new(0.25, 0.5, 1.5).unwrap(),
    ]
}
