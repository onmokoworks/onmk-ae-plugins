//! CPU backend. Mirrors `shader.wgsl` pass by pass (docs/design.md §7.1).

use crate::spec::{Blend, Dilation, Plan, Span};

pub(crate) fn alloc(len: usize) -> Option<Vec<f32>> {
    let mut v = Vec::new();
    v.try_reserve_exact(len).ok()?;
    v.resize(len, 0.0);
    Some(v)
}

/// Plane sample with the exterior boundary condition (outside = 1).
#[inline]
fn at(p: &[f32], w: usize, h: usize, x: i64, y: i64) -> f32 {
    if x < 0 || y < 0 || x >= w as i64 || y >= h as i64 {
        1.0
    } else {
        p[y as usize * w + x as usize]
    }
}

/// Sparse-table step: dst(x) = max(src(x), src(x + half)).
fn level_step(src: &[f32], dst: &mut [f32], w: usize, h: usize, half: i64) {
    for y in 0..h {
        for x in 0..w {
            let (xi, yi) = (x as i64, y as i64);
            dst[y * w + x] = at(src, w, h, xi, yi).max(at(src, w, h, xi + half, yi));
        }
    }
}

/// out = max(out, span maxima for every span of one level). `pow` = 2^level.
fn accumulate(src: &[f32], out: &mut [f32], w: usize, h: usize, pow: i64, spans: &[Span]) {
    for y in 0..h {
        for x in 0..w {
            let (xi, yi) = (x as i64, y as i64);
            let mut m = out[y * w + x];
            for s in spans {
                let yy = yi + s.dy as i64;
                let hw = s.half_width as i64;
                m = m
                    .max(at(src, w, h, xi - hw, yy))
                    .max(at(src, w, h, xi + hw - pow + 1, yy));
            }
            out[y * w + x] = m;
        }
    }
}

#[derive(Clone, Copy)]
enum Cur {
    E,
    A,
    B,
}

/// Flat elliptical grey-scale dilation of `e` into `out`.
pub(crate) fn dilate(
    e: &[f32],
    w: usize,
    h: usize,
    d: &Dilation,
    la: &mut [f32],
    lb: &mut [f32],
    out: &mut [f32],
) {
    out.fill(0.0);
    let mut cur = Cur::E;
    let mut start = 0usize;
    for level in 0..d.levels {
        if level > 0 {
            let half = 1i64 << (level - 1);
            cur = match cur {
                Cur::E => {
                    level_step(e, &mut *la, w, h, half);
                    Cur::A
                }
                Cur::A => {
                    level_step(&*la, &mut *lb, w, h, half);
                    Cur::B
                }
                Cur::B => {
                    level_step(&*lb, &mut *la, w, h, half);
                    Cur::A
                }
            };
        }
        let end = start
            + d.spans[start..]
                .iter()
                .take_while(|s| s.level == level)
                .count();
        if end > start {
            let src: &[f32] = match cur {
                Cur::E => e,
                Cur::A => &*la,
                Cur::B => &*lb,
            };
            accumulate(src, out, w, h, 1i64 << level, &d.spans[start..end]);
        }
        start = end;
    }
}

#[inline]
fn mixed(lo: &[f32], hi: &[f32], mix: f32, w: usize, h: usize, x: i64, y: i64) -> f32 {
    if x < 0 || y < 0 || x >= w as i64 || y >= h as i64 {
        return 1.0;
    }
    let i = y as usize * w + x as usize;
    lo[i] + (hi[i] - lo[i]) * mix
}

pub(crate) fn blur_h(
    lo: &[f32],
    hi: &[f32],
    mix: f32,
    w: usize,
    h: usize,
    k: &[f32],
    dst: &mut [f32],
) {
    let r = (k.len() / 2) as i64;
    for y in 0..h {
        for x in 0..w {
            let mut sum = 0.0f32;
            for (i, wt) in k.iter().enumerate() {
                sum += wt * mixed(lo, hi, mix, w, h, x as i64 + i as i64 - r, y as i64);
            }
            dst[y * w + x] = sum;
        }
    }
}

pub(crate) fn blur_v(src: &[f32], w: usize, h: usize, k: &[f32], dst: &mut [f32]) {
    let r = (k.len() / 2) as i64;
    for y in 0..h {
        for x in 0..w {
            let mut sum = 0.0f32;
            for (i, wt) in k.iter().enumerate() {
                sum += wt * at(src, w, h, x as i64, y as i64 + i as i64 - r);
            }
            dst[y * w + x] = sum;
        }
    }
}

/// Straight-RGB inset shadow for one pixel. Returns (r', g', b', t).
#[inline]
pub fn composite_pixel(src: [f32; 4], m: f32, plan: &Plan) -> [f32; 4] {
    let t = plan.opacity * m.clamp(0.0, 1.0);
    let k = plan.color;
    let c = |c: f32, k: f32| match plan.blend {
        Blend::Normal => c + t * (k - c),
        Blend::Multiply => c * (1.0 - t * (1.0 - k)),
    };
    [c(src[0], k[0]), c(src[1], k[1]), c(src[2], k[2]), t]
}

/// Mask pipeline + composite. `e` is Ω (w x h); `src` is output-aligned
/// (out_w x out_h); x0 of output pixel (x, y) in Ω is x + base_x.
pub(crate) fn shade(
    e: &[f32],
    w: usize,
    h: usize,
    src: &[[f32; 4]],
    out_w: usize,
    out_h: usize,
    base: (i64, i64),
    plan: &Plan,
) -> Option<Vec<[f32; 4]>> {
    let n = w.checked_mul(h)?;
    let mut la = alloc(n)?;
    let mut lb = alloc(n)?;
    let lo = match &plan.dilate_lo {
        Some(d) => {
            let mut out = alloc(n)?;
            dilate(e, w, h, d, &mut la, &mut lb, &mut out);
            Some(out)
        }
        None => None,
    };
    let hi = match &plan.dilate_hi {
        Some(d) => {
            let mut out = alloc(n)?;
            dilate(e, w, h, d, &mut la, &mut lb, &mut out);
            Some(out)
        }
        None => None,
    };
    let lo_ref: &[f32] = lo.as_deref().unwrap_or(e);
    let hi_ref: &[f32] = hi.as_deref().unwrap_or(e);
    blur_h(
        lo_ref,
        hi_ref,
        plan.dilate_mix,
        w,
        h,
        &plan.kernel_x,
        &mut la,
    );
    blur_v(&la, w, h, &plan.kernel_y, &mut lb);
    let b = lb;
    let (wx0, wx1) = plan.weights_x();
    let (wy0, wy1) = plan.weights_y();
    let mut out = Vec::new();
    out.try_reserve_exact(src.len()).ok()?;
    for y in 0..out_h {
        for x in 0..out_w {
            let x0 = x as i64 + base.0;
            let y0 = y as i64 + base.1;
            let m = (at(&b, w, h, x0, y0) * wx0 + at(&b, w, h, x0 + 1, y0) * wx1) * wy0
                + (at(&b, w, h, x0, y0 + 1) * wx0 + at(&b, w, h, x0 + 1, y0 + 1) * wx1) * wy1;
            out.push(composite_pixel(src[y * out_w + x], m, plan));
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{dilation, Scale};

    fn naive_dilate(e: &[f32], w: usize, h: usize, d: &Dilation) -> Vec<f32> {
        let mut out = vec![0.0; w * h];
        for y in 0..h {
            for x in 0..w {
                let mut m = 0.0f32;
                for s in &d.spans {
                    for i in -s.half_width..=s.half_width {
                        m = m.max(at(e, w, h, x as i64 + i as i64, y as i64 + s.dy as i64));
                    }
                }
                out[y * w + x] = m;
            }
        }
        out
    }

    #[test]
    fn sparse_table_dilation_equals_direct_span_enumeration() {
        let (w, h) = (23, 17);
        let mut seed = 7u32;
        let e: Vec<f32> = (0..w * h)
            .map(|_| {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                if seed % 7 == 0 {
                    (seed >> 8) as f32 / 16777216.0
                } else {
                    0.0
                }
            })
            .collect();
        for (r, s) in [
            (1.0, Scale::FULL),
            (3.0, Scale::FULL),
            (6.0, Scale::FULL),
            (
                5.0,
                Scale {
                    dsx: 1.0,
                    dsy: 0.5,
                    par: 0.5,
                },
            ),
        ] {
            let d = dilation(r, &s).unwrap().unwrap();
            let (mut la, mut lb, mut out) = (vec![0.0; w * h], vec![0.0; w * h], vec![0.0; w * h]);
            dilate(&e, w, h, &d, &mut la, &mut lb, &mut out);
            assert_eq!(out, naive_dilate(&e, w, h, &d), "r={r}");
        }
    }

    #[test]
    fn composite_modes_preserve_hdr_and_sign() {
        let mut p = crate::spec::Plan::new(
            &crate::spec::ShadowParams {
                opacity: 1.0,
                ..Default::default()
            },
            &Scale::FULL,
        )
        .unwrap();
        p.color = [1.0, 0.0, 0.5];
        p.blend = Blend::Multiply;
        let o = composite_pixel([4.0, -0.5, 0.25, 1.0], 0.5, &p);
        assert_eq!(o, [4.0, -0.25, 0.25 * 0.75, 0.5]);
        p.blend = Blend::Normal;
        let o = composite_pixel([4.0, -0.5, 0.25, 1.0], 0.5, &p);
        assert_eq!(o, [2.5, -0.25, 0.375, 0.5]);
        let o = composite_pixel([4.0, -0.5, 0.25, 1.0], 0.0, &p);
        assert_eq!(o, [4.0, -0.5, 0.25, 0.0]);
    }
}
