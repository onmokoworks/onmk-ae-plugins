//! Line generation (always on the CPU). Produces stable line records and convex
//! primitives in absolute buffer coordinates.
//!
//! Spaces:
//! * layer px: full-resolution layer pixels, y down (AE layer space).
//! * display: layer px with x multiplied by the pixel aspect ratio (square pixels).
//!   All geometry, angles and widths are defined here.
//! * buffer: `x_buf = X / par * dsx`, `y_buf = Y * dsy`. Downsampling is applied
//!   exactly once, in [`Mapping::to_buffer`].
//!
//! Angles: 0 degrees = up, clockwise positive. Direction `(sin t, -cos t)`.

use crate::hash::{self, unit};
use crate::raster::{Hole, Prim, Scene};
use std::f64::consts::{PI, TAU};

pub const MAX_LINES: f64 = 5000.0;
pub const MAX_SEGMENTS_PER_LINE: usize = 64;
/// Estimated profile error tolerance for taper subdivision, in buffer pixels.
pub const TAPER_TOLERANCE_PX: f64 = 0.02;
const GOLDEN: f64 = 0.618_033_988_749_894_9;
const MAX_FOCUS: f64 = 1.0e6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Evolution {
    Static,
    HoldFrames,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Composite {
    LinesOnly,
    OverSource,
}

/// User-facing values in UI units (%, degrees, px). Sanitized before use.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// Focus in full-resolution layer px. `None` = layer centre.
    pub focus: Option<[f64; 2]>,
    /// Vertical semi-axis of the empty centre, % of H/2.
    pub center_size: f64,
    /// Horizontal / vertical semi-axis ratio.
    pub center_aspect: f64,
    pub center_rotation: f64,
    /// Ellipse centre offset, % of the semi-axes; length clamped to 95%.
    pub center_offset: [f64; 2],
    pub line_count: f64,
    /// Line width in display px (full resolution).
    pub line_width: f64,
    pub width_random: f64,
    /// Tip width, % of the line width.
    pub tip_width: f64,
    /// Round the inward end; default false preserves earlier renders.
    pub round_tip: bool,
    /// Taper length, % of H/2. 0 = constant width.
    pub taper_length: f64,
    pub taper_shape: f64,
    /// Random inner-start push, % of H/2.
    pub length_random: f64,
    pub short_min: f64,
    pub short_max: f64,
    pub short_mix: f64,
    pub seed: u32,
    pub angle_jitter: f64,
    pub clustering: f64,
    pub cluster_count: u32,
    pub cluster_spread: f64,
    pub density_variation: f64,
    pub gap_angle: f64,
    pub gap_width: f64,
    pub gap_softness: f64,
    pub evolution: Evolution,
    pub hold_frames: u32,
    pub change_amount: f64,
    pub stagger: f64,
    pub color: [f32; 3],
    pub intensity: f32,
    /// 0..100 %.
    pub opacity: f32,
    pub composite: Composite,
    pub samples: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            focus: None,
            center_size: 45.0,
            center_aspect: 1.6,
            center_rotation: 0.0,
            center_offset: [0.0, 0.0],
            line_count: 220.0,
            line_width: 9.0,
            width_random: 45.0,
            tip_width: 0.0,
            round_tip: false,
            taper_length: 70.0,
            taper_shape: 1.0,
            length_random: 35.0,
            short_min: 30.0,
            short_max: 120.0,
            short_mix: 0.0,
            seed: 1,
            angle_jitter: 0.6,
            clustering: 15.0,
            cluster_count: 12,
            cluster_spread: 4.0,
            density_variation: 25.0,
            gap_angle: 180.0,
            gap_width: 0.0,
            gap_softness: 20.0,
            evolution: Evolution::Static,
            hold_frames: 2,
            change_amount: 100.0,
            stagger: 0.0,
            color: [0.0, 0.0, 0.0],
            intensity: 1.0,
            opacity: 100.0,
            composite: Composite::LinesOnly,
            samples: 8,
        }
    }
}

fn finite_or(v: f64, fallback: f64) -> f64 {
    if v.is_finite() {
        v
    } else {
        fallback
    }
}

fn clampf(v: f64, lo: f64, hi: f64, fallback: f64) -> f64 {
    finite_or(v, fallback).clamp(lo, hi)
}

impl Settings {
    /// Replace non-finite values and clamp every field to its valid range.
    pub fn sanitized(&self) -> Settings {
        let d = Settings::default();
        let s = self;
        Settings {
            focus: s.focus.map(|f| {
                [
                    clampf(f[0], -MAX_FOCUS, MAX_FOCUS, 0.0),
                    clampf(f[1], -MAX_FOCUS, MAX_FOCUS, 0.0),
                ]
            }),
            center_size: clampf(s.center_size, 0.0, 1000.0, d.center_size),
            center_aspect: clampf(s.center_aspect, 0.1, 10.0, d.center_aspect),
            center_rotation: finite_or(s.center_rotation, 0.0) % 360.0,
            center_offset: [
                clampf(s.center_offset[0], -100.0, 100.0, 0.0),
                clampf(s.center_offset[1], -100.0, 100.0, 0.0),
            ],
            line_count: clampf(s.line_count, 0.0, MAX_LINES, 0.0),
            line_width: clampf(s.line_width, 0.0, 500.0, 0.0),
            width_random: clampf(s.width_random, 0.0, 100.0, 0.0),
            tip_width: clampf(s.tip_width, 0.0, 100.0, 0.0),
            round_tip: s.round_tip,
            taper_length: clampf(s.taper_length, 0.0, 1000.0, d.taper_length),
            taper_shape: clampf(s.taper_shape, 0.25, 4.0, 1.0),
            length_random: clampf(s.length_random, 0.0, 400.0, 0.0),
            short_min: clampf(s.short_min, 0.0, 10000.0, 30.0),
            short_max: clampf(s.short_max, 0.0, 10000.0, 120.0),
            short_mix: clampf(s.short_mix, 0.0, 100.0, 0.0),
            seed: s.seed,
            angle_jitter: clampf(s.angle_jitter, 0.0, 30.0, 0.0),
            clustering: clampf(s.clustering, 0.0, 100.0, 0.0),
            cluster_count: s.cluster_count.clamp(1, 64),
            cluster_spread: clampf(s.cluster_spread, 0.0, 90.0, 0.0),
            density_variation: clampf(s.density_variation, 0.0, 100.0, 0.0),
            gap_angle: finite_or(s.gap_angle, 0.0) % 360.0,
            gap_width: clampf(s.gap_width, 0.0, 360.0, 0.0),
            gap_softness: clampf(s.gap_softness, 0.0, 180.0, 0.0),
            evolution: s.evolution,
            hold_frames: s.hold_frames.clamp(1, 10_000),
            change_amount: clampf(s.change_amount, 0.0, 100.0, 0.0),
            stagger: clampf(s.stagger, 0.0, 100.0, 0.0),
            color: s.color.map(|c| {
                if c.is_finite() {
                    c.clamp(0.0, 1.0)
                } else {
                    0.0
                }
            }),
            intensity: if s.intensity.is_finite() {
                s.intensity.clamp(0.0, 100.0)
            } else {
                1.0
            },
            opacity: if s.opacity.is_finite() {
                s.opacity.clamp(0.0, 100.0)
            } else {
                0.0
            },
            composite: s.composite,
            samples: if s.samples >= 16 { 16 } else { 8 },
        }
    }
}

/// Layer geometry and resolution mapping, fixed once per render.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mapping {
    /// Full-resolution layer size in layer px.
    pub layer_w: f64,
    pub layer_h: f64,
    /// Pixel aspect ratio (width / height).
    pub par: f64,
    pub dsx: f64,
    pub dsy: f64,
}

impl Mapping {
    pub fn new(layer_w: f64, layer_h: f64, par: f64, dsx: f64, dsy: f64) -> Mapping {
        let positive = |v: f64, fallback: f64| {
            if v.is_finite() && v > 0.0 {
                v
            } else {
                fallback
            }
        };
        Mapping {
            layer_w: if layer_w.is_finite() {
                layer_w.clamp(0.0, 1.0e6)
            } else {
                0.0
            },
            layer_h: if layer_h.is_finite() {
                layer_h.clamp(0.0, 1.0e6)
            } else {
                0.0
            },
            par: positive(par, 1.0).clamp(0.01, 100.0),
            dsx: positive(dsx, 1.0).clamp(1.0e-4, 1.0),
            dsy: positive(dsy, 1.0).clamp(1.0e-4, 1.0),
        }
    }

    pub fn to_buffer(&self, p: [f64; 2]) -> [f64; 2] {
        [p[0] / self.par * self.dsx, p[1] * self.dsy]
    }

    /// Layer extent in buffer pixels (continuous, not rounded).
    pub fn buffer_extent(&self) -> [f64; 2] {
        [self.layer_w * self.dsx, self.layer_h * self.dsy]
    }

    /// Largest display -> buffer scale factor.
    fn max_scale(&self) -> f64 {
        (self.dsx / self.par).max(self.dsy)
    }
}

/// Frame index from AE rational time: `floor(current_time / time_step)`,
/// floor also for negative times. Returns 0 if the step is not positive.
pub fn frame_index(current_time: i64, time_step: i64) -> i64 {
    if time_step <= 0 {
        0
    } else {
        current_time.div_euclid(time_step)
    }
}

/// Normalized ellipse offset, radially clamped to length 0.95.
pub fn clamp_offset(ox: f64, oy: f64) -> (f64, f64) {
    let len = (ox * ox + oy * oy).sqrt();
    if len > 0.95 {
        (ox * 0.95 / len, oy * 0.95 / len)
    } else {
        (ox, oy)
    }
}

/// Gap visibility factor in [0, 1] for angular distance `delta` (degrees).
pub fn gap_factor(delta: f64, gap_width: f64, softness: f64) -> f64 {
    if gap_width.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
        return 1.0;
    }
    if gap_width >= 360.0 {
        return 0.0;
    }
    let half = gap_width * 0.5;
    if softness.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
        return if delta >= half { 1.0 } else { 0.0 };
    }
    let t = ((delta - half) / softness).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn wrap_pi(a: f64) -> f64 {
    let r = (a + PI).rem_euclid(TAU) - PI;
    if r.is_finite() {
        r
    } else {
        0.0
    }
}

fn rot(v: [f64; 2], c: f64, s: f64) -> [f64; 2] {
    [c * v[0] - s * v[1], s * v[0] + c * v[1]]
}

/// One concentration line with a stable identity `id`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineRec {
    pub id: u32,
    /// Angle in radians, [0, 2 pi), 0 = up, clockwise.
    pub theta: f64,
    /// Inner / outer distance from the focus in display px.
    pub r0: f64,
    pub r1: f64,
    /// Full width at the end of the taper, display px.
    pub width: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Ellipse {
    center: [f64; 2],
    a: f64,
    b: f64,
    cos: f64,
    sin: f64,
}

impl Ellipse {
    /// Exit distance of the ray `focus + t dir` (focus is inside by construction).
    fn exit(&self, focus: [f64; 2], dir: [f64; 2]) -> f64 {
        let p = rot(
            [focus[0] - self.center[0], focus[1] - self.center[1]],
            self.cos,
            -self.sin,
        );
        let v = rot(dir, self.cos, -self.sin);
        let p = [p[0] / self.a, p[1] / self.b];
        let v = [v[0] / self.a, v[1] / self.b];
        let qa = v[0] * v[0] + v[1] * v[1];
        let qb = 2.0 * (p[0] * v[0] + p[1] * v[1]);
        let qc = p[0] * p[0] + p[1] * p[1] - 1.0;
        let disc = qb * qb - 4.0 * qa * qc;
        if qa.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater)
            || !matches!(
                disc.partial_cmp(&0.0),
                Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)
            )
        {
            return 0.0;
        }
        ((-qb + disc.sqrt()) / (2.0 * qa)).max(0.0)
    }
}

struct Frame {
    s: Settings,
    focus: [f64; 2],
    refr: f64,
    disp: [f64; 2],
    ellipse: Option<Ellipse>,
}

fn prepare(settings: &Settings, m: &Mapping) -> Frame {
    let s = settings.sanitized();
    let refr = m.layer_h * 0.5;
    let focus_layer = s.focus.unwrap_or([m.layer_w * 0.5, m.layer_h * 0.5]);
    let focus = [focus_layer[0] * m.par, focus_layer[1]];
    let b = s.center_size / 100.0 * refr;
    let a = b * s.center_aspect;
    let ellipse = (a > 1e-9 && b > 1e-9).then(|| {
        let (cos, sin) = (
            s.center_rotation.to_radians().cos(),
            s.center_rotation.to_radians().sin(),
        );
        let (ox, oy) = clamp_offset(s.center_offset[0] / 100.0, s.center_offset[1] / 100.0);
        let off = rot([a * ox, b * oy], cos, sin);
        Ellipse {
            center: [focus[0] + off[0], focus[1] + off[1]],
            a,
            b,
            cos,
            sin,
        }
    });
    Frame {
        s,
        focus,
        refr,
        disp: [m.layer_w * m.par, m.layer_h],
        ellipse,
    }
}

/// Ray / rectangle `[0,w] x [0,h]` intersection for t >= 0.
fn ray_rect(p: [f64; 2], d: [f64; 2], size: [f64; 2]) -> Option<(f64, f64)> {
    let mut t0 = 0.0f64;
    let mut t1 = f64::INFINITY;
    for axis in 0..2 {
        if d[axis].abs() < 1e-15 {
            if p[axis] < 0.0 || p[axis] > size[axis] {
                return None;
            }
        } else {
            let a = (0.0 - p[axis]) / d[axis];
            let b = (size[axis] - p[axis]) / d[axis];
            t0 = t0.max(a.min(b));
            t1 = t1.min(a.max(b));
        }
    }
    (t1 > t0).then_some((t0, t1))
}

/// Stable line records for one frame (`frame` only matters for Hold Frames).
pub fn layout_lines(settings: &Settings, m: &Mapping, frame: i64) -> Vec<LineRec> {
    let f = prepare(settings, m);
    let s = &f.s;
    let mut out = Vec::new();
    if f.refr.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater)
        || f.disp[0].partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater)
        || s.line_width <= 0.0
        || s.opacity <= 0.0
    {
        return out;
    }
    let seed = s.seed;
    let total = s.line_count.ceil() as u32;
    let full = s.line_count.floor();
    let offset = unit(seed, u32::MAX, hash::CH_ANGLE_OFFSET, 0);
    let cluster_offset = unit(seed, u32::MAX, hash::CH_CLUSTER_OFFSET, 0);
    let phases = [1u32, 2, 3].map(|h| unit(seed, u32::MAX - h, hash::CH_DENSITY, 0) * TAU);
    let clusters: Vec<f64> = (0..s.cluster_count)
        .map(|j| TAU * (f64::from(j) * GOLDEN + cluster_offset).fract())
        .collect();
    let jitter = s.angle_jitter.to_radians();
    let spread = s.cluster_spread.to_radians();
    let clump = s.clustering / 100.0;
    let change = s.change_amount / 100.0;
    let gap_center = s.gap_angle.to_radians();
    for k in 0..total {
        let fade = if f64::from(k) < full {
            1.0
        } else {
            s.line_count - full
        };
        if fade.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
            continue;
        }
        let epoch = match s.evolution {
            Evolution::Static => 0,
            Evolution::HoldFrames => {
                let hold = i64::from(s.hold_frames);
                let shift = (s.stagger / 100.0 * unit(seed, k, hash::CH_STAGGER, 0) * hold as f64)
                    .floor() as i64;
                frame.saturating_add(shift).div_euclid(hold)
            }
        };
        let var = |ch: u32| {
            let a = unit(seed, k, ch, 0);
            if epoch == 0 {
                a
            } else {
                a + (unit(seed, k, ch, epoch) - a) * change
            }
        };
        let mut theta = TAU * (f64::from(k) * GOLDEN + offset).fract();
        theta += (2.0 * var(hash::CH_JITTER) - 1.0) * jitter;
        if clump > 0.0 {
            let mut best = clusters[0];
            let mut best_d = f64::INFINITY;
            for &c in &clusters {
                let d = wrap_pi(c - theta).abs();
                if d < best_d {
                    best_d = d;
                    best = c;
                }
            }
            let target = best + (2.0 * var(hash::CH_SPREAD) - 1.0) * spread;
            theta += clump * wrap_pi(target - theta);
        }
        let theta = theta.rem_euclid(TAU);
        let delta = wrap_pi(theta - gap_center).abs().to_degrees();
        let g = gap_factor(delta, s.gap_width, s.gap_softness);
        if g.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
            continue;
        }
        let dir = [theta.sin(), -theta.cos()];
        let Some((_, r1)) = ray_rect(f.focus, dir, f.disp) else {
            continue;
        };
        let re = f.ellipse.map_or(0.0, |e| e.exit(f.focus, dir));
        let density = ((theta + phases[0]).cos()
            + (2.0 * theta + phases[1]).cos()
            + (3.0 * theta + phases[2]).cos())
            / 3.0;
        let mut r0 = re
            + s.length_random / 100.0 * var(hash::CH_LENGTH) * f.refr
            + s.density_variation / 100.0 * density.max(0.0) * f.refr;
        if unit(seed, k, hash::CH_SHORT_GROUP, 0) < s.short_mix / 100.0 {
            let lo = s.short_min.min(s.short_max);
            let hi = s.short_min.max(s.short_max);
            let length = lo + (hi - lo) * var(hash::CH_SHORT_LENGTH);
            // Preserve the empty centre even when the requested length exceeds it.
            r0 = (r1 - length).max(re);
        }
        r0 += (1.0 - g) * (r1 - r0).max(0.0);
        if r1.partial_cmp(&r0) != Some(std::cmp::Ordering::Greater) {
            continue;
        }
        let width = s.line_width * (1.0 - s.width_random / 100.0 * var(hash::CH_WIDTH)) * fade * g;
        if width.partial_cmp(&1e-9) != Some(std::cmp::Ordering::Greater) {
            continue;
        }
        out.push(LineRec {
            id: k,
            theta,
            r0,
            r1,
            width,
        });
    }
    out
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub lines: usize,
    pub prims: usize,
    /// Lines whose taper subdivision hit [`MAX_SEGMENTS_PER_LINE`].
    pub capped_lines: usize,
}

/// Half width at distance `d` from the inner end.
fn half_width(line: &LineRec, s: &Settings, taper_len: f64, d: f64) -> f64 {
    let tip = s.tip_width / 100.0;
    if taper_len <= 1e-9 || tip >= 1.0 {
        return line.width * 0.5;
    }
    let t = (d / taper_len).clamp(0.0, 1.0);
    let e = if s.taper_shape == 1.0 {
        t
    } else {
        t.powf(s.taper_shape)
    };
    line.width * 0.5 * (tip + (1.0 - tip) * e)
}

/// Split [0, len] into segments with linear width (error-adaptive in the taper).
fn segments(line: &LineRec, s: &Settings, taper_len: f64, tol: f64) -> (Vec<(f64, f64)>, bool) {
    let len = line.r1 - line.r0;
    let tip = s.tip_width / 100.0;
    if taper_len <= 1e-9 || tip >= 1.0 {
        return (vec![(0.0, len)], false);
    }
    let end = taper_len.min(len);
    let budget = MAX_SEGMENTS_PER_LINE - usize::from(len > end);
    let mut out = Vec::new();
    let mut capped = false;
    // LIFO stack, right half pushed first so output is in ascending order.
    let mut stack = vec![(0.0, end)];
    while let Some((a, b)) = stack.pop() {
        let mid = 0.5 * (a + b);
        let ha = half_width(line, s, taper_len, a);
        let hb = half_width(line, s, taper_len, b);
        let err = (half_width(line, s, taper_len, mid) - 0.5 * (ha + hb)).abs();
        if err <= tol || b - a <= 1e-9 {
            out.push((a, b));
        } else if out.len() + stack.len() + 2 > budget {
            capped = true;
            out.push((a, b));
        } else {
            stack.push((mid, b));
            stack.push((a, mid));
        }
    }
    if len > end {
        out.push((end, len));
    }
    (out, capped)
}

/// Sutherland-Hodgman clip against `[0, w] x [0, h]`.
fn clip_rect(poly: &[[f64; 2]], w: f64, h: f64) -> Vec<[f64; 2]> {
    let mut pts = poly.to_vec();
    let planes: [(usize, f64, bool); 4] =
        [(0, 0.0, true), (0, w, false), (1, 0.0, true), (1, h, false)];
    for (axis, bound, keep_greater) in planes {
        if pts.is_empty() {
            break;
        }
        let inside = |p: &[f64; 2]| {
            if keep_greater {
                p[axis] >= bound
            } else {
                p[axis] <= bound
            }
        };
        let mut next = Vec::with_capacity(pts.len() + 2);
        for i in 0..pts.len() {
            let a = pts[i];
            let b = pts[(i + 1) % pts.len()];
            let (ia, ib) = (inside(&a), inside(&b));
            if ia {
                next.push(a);
            }
            if ia != ib {
                let t = (bound - a[axis]) / (b[axis] - a[axis]);
                let mut p = [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])];
                p[axis] = bound;
                next.push(p);
            }
        }
        pts = next;
    }
    pts
}

/// Build the full scene (primitives + hole) in absolute buffer coordinates.
fn round_cap(prims: &mut Vec<Prim>, center: [f64; 2], dir: [f64; 2], radius: f64, m: &Mapping) {
    if radius <= 1e-9 {
        return;
    }
    let nrm = [-dir[1], dir[0]];
    // Sagitta <=0.02 output pixels, including non-square PAR / downsampling.
    let r = radius * m.max_scale();
    let step = 2.0 * (1.0 - (TAPER_TOLERANCE_PX / r).min(1.0)).acos();
    let steps = (PI / step).ceil().clamp(8.0, 512.0) as usize;
    let [w, h] = m.buffer_extent();
    let at = |angle: f64| {
        m.to_buffer([
            center[0] + radius * (nrm[0] * angle.cos() - dir[0] * angle.sin()),
            center[1] + radius * (nrm[1] * angle.cos() - dir[1] * angle.sin()),
        ])
    };
    for j in 0..steps {
        let triangle = [
            m.to_buffer(center),
            at(PI * j as f64 / steps as f64),
            at(PI * (j + 1) as f64 / steps as f64),
        ];
        if let Some(p) = Prim::from_points(&clip_rect(&triangle, w, h)) {
            prims.push(p);
        }
    }
}

pub fn build_scene(settings: &Settings, m: &Mapping, frame: i64) -> (Scene, Stats) {
    let f = prepare(settings, m);
    let lines = layout_lines(settings, m, frame);
    let s = &f.s;
    let mut stats = Stats {
        lines: lines.len(),
        ..Stats::default()
    };
    let taper_len = s.taper_length / 100.0 * f.refr;
    let tol = TAPER_TOLERANCE_PX / m.max_scale();
    let [bw, bh] = m.buffer_extent();
    let mut prims = Vec::new();
    for line in &lines {
        let mut capped_line = *line;
        if s.round_tip {
            // Keep the inwardmost cap at the old tip position; the diameter
            // joins the body one radius farther out, preserving the empty area.
            let radius = half_width(line, s, taper_len, 0.0);
            capped_line.r0 += radius;
            if capped_line.r0 >= capped_line.r1 {
                continue;
            }
            let dir = [line.theta.sin(), -line.theta.cos()];
            let center = [
                f.focus[0] + dir[0] * capped_line.r0,
                f.focus[1] + dir[1] * capped_line.r0,
            ];
            round_cap(&mut prims, center, dir, radius, m);
        }
        // Put the outer cap beyond every canvas point's projection onto the ray.
        // Stopping at the centerline/edge intersection cuts wide diagonal rays
        // inside the image and exposes triangular wedges of background.
        let outward = [line.theta.sin(), -line.theta.cos()];
        for corner in [[0.0, 0.0], [f.disp[0], 0.0], [0.0, f.disp[1]], f.disp] {
            let projection =
                (corner[0] - f.focus[0]) * outward[0] + (corner[1] - f.focus[1]) * outward[1];
            capped_line.r1 = capped_line.r1.max(projection + 1.0);
        }
        let line = &capped_line;
        let dir = [line.theta.sin(), -line.theta.cos()];
        let nrm = [line.theta.cos(), line.theta.sin()];
        let (segs, capped) = segments(line, s, taper_len, tol);
        if capped {
            stats.capped_lines += 1;
        }
        for (a, b) in segs {
            let ha = half_width(line, s, taper_len, a);
            let hb = half_width(line, s, taper_len, b);
            let pa = [
                f.focus[0] + dir[0] * (line.r0 + a),
                f.focus[1] + dir[1] * (line.r0 + a),
            ];
            let pb = [
                f.focus[0] + dir[0] * (line.r0 + b),
                f.focus[1] + dir[1] * (line.r0 + b),
            ];
            let quad = [
                [pa[0] + nrm[0] * ha, pa[1] + nrm[1] * ha],
                [pb[0] + nrm[0] * hb, pb[1] + nrm[1] * hb],
                [pb[0] - nrm[0] * hb, pb[1] - nrm[1] * hb],
                [pa[0] - nrm[0] * ha, pa[1] - nrm[1] * ha],
            ]
            .map(|p| m.to_buffer(p));
            let clipped = clip_rect(&quad, bw, bh);
            if let Some(prim) = Prim::from_points(&clipped) {
                prims.push(prim);
            }
        }
    }
    stats.prims = prims.len();
    let hole = f.ellipse.map(|e| {
        // u = diag(1/a, 1/b) R(-rot) diag(par/dsx, 1/dsy) (p_buf - c_buf)
        let sx = m.par / m.dsx;
        let sy = 1.0 / m.dsy;
        Hole {
            c: m.to_buffer(e.center),
            m: [
                [e.cos * sx / e.a, e.sin * sy / e.a],
                [-e.sin * sx / e.b, e.cos * sy / e.b],
            ],
        }
    });
    (Scene { prims, hole }, stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mapping() -> Mapping {
        Mapping::new(1920.0, 1080.0, 1.0, 1.0, 1.0)
    }

    #[test]
    fn round_cap_matches_semicircle_area_and_has_no_square_corners() {
        let m = Mapping::new(64.0, 64.0, 1.0, 1.0, 1.0);
        let mut prims = Vec::new();
        round_cap(&mut prims, [32.0, 32.0], [0.0, -1.0], 12.0, &m);
        let scene = Scene { prims, hole: None };
        let c = crate::raster::cpu_render(
            &scene,
            crate::raster::PixelRect {
                x0: 0,
                y0: 0,
                w: 64,
                h: 64,
            },
            16,
        )
        .unwrap();
        let area: f64 = c.iter().map(|&a| f64::from(a)).sum();
        assert!((area - PI * 72.0).abs() < 1.0, "{area}");
        assert!(c[42 * 64 + 32] > 0.99);
        assert_eq!(c[42 * 64 + 42], 0.0);
        assert_eq!(c[30 * 64 + 32], 0.0);
    }

    #[test]
    fn adding_lines_keeps_existing_identities() {
        let mut s = Settings::default();
        let a = layout_lines(&s, &mapping(), 0);
        s.line_count += 1.0;
        let b = layout_lines(&s, &mapping(), 0);
        for line in &a {
            let other = b.iter().find(|l| l.id == line.id).expect("line kept");
            assert_eq!(line, other);
        }
        assert!(b.len() >= a.len());
    }

    #[test]
    fn offset_is_clamped_radially() {
        let (x, y) = clamp_offset(0.95, 0.95);
        assert!(((x * x + y * y).sqrt() - 0.95).abs() < 1e-12);
        assert_eq!(clamp_offset(0.3, -0.4), (0.3, -0.4));
    }

    #[test]
    fn gap_edge_cases() {
        assert_eq!(gap_factor(0.0, 0.0, 20.0), 1.0);
        assert_eq!(gap_factor(179.0, 360.0, 0.0), 0.0);
        assert_eq!(gap_factor(10.0, 40.0, 0.0), 0.0);
        assert_eq!(gap_factor(20.0, 40.0, 0.0), 1.0);
        assert!(gap_factor(25.0, 40.0, 10.0) > 0.0 && gap_factor(25.0, 40.0, 10.0) < 1.0);
    }

    #[test]
    fn frame_index_floors_negative_time() {
        assert_eq!(frame_index(0, 1001), 0);
        assert_eq!(frame_index(1000, 1001), 0);
        assert_eq!(frame_index(1001, 1001), 1);
        assert_eq!(frame_index(-1, 1001), -1);
        assert_eq!(frame_index(-1001, 1001), -1);
        assert_eq!(frame_index(-1002, 1001), -2);
        assert_eq!(frame_index(5, 0), 0);
    }

    #[test]
    fn segments_split_taper_and_constant_part() {
        let s = Settings::default().sanitized();
        let line = LineRec {
            id: 0,
            theta: 0.0,
            r0: 0.0,
            r1: 1000.0,
            width: 10.0,
        };
        let (segs, capped) = segments(&line, &s, 378.0, 0.02);
        assert!(!capped);
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[1], (378.0, 1000.0));
        let (segs, _) = segments(&line, &s, 0.0, 0.02);
        assert_eq!(segs, vec![(0.0, 1000.0)]);
    }
}
