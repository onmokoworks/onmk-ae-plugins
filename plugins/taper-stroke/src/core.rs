//! Host-independent outline construction and coverage rasterization.

#[derive(Clone, Copy, Debug)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}
impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
    fn sub(self, p: Self) -> Self {
        Self::new(self.x - p.x, self.y - p.y)
    }
    fn add(self, p: Self) -> Self {
        Self::new(self.x + p.x, self.y + p.y)
    }
    fn mul(self, v: f64) -> Self {
        Self::new(self.x * v, self.y * v)
    }
    fn length(self) -> f64 {
        self.x.hypot(self.y)
    }
}

#[derive(Clone, Copy)]
pub struct Style {
    pub width: f64,
    pub start: f64,
    pub end: f64,
    pub ease: f64,
    pub round: bool,
}
impl Style {
    pub fn radius(self, fraction: f64) -> f64 {
        let t = if self.start >= 1.0 {
            0.0
        } else {
            ((fraction - self.start) / (1.0 - self.start)).clamp(0.0, 1.0)
        };
        // Ease=1 is linear; other values retain an adjustable power curve.
        let t = t.powf(self.ease.clamp(0.1, 10.0));
        self.width.max(0.0) * 0.5 * (1.0 - (1.0 - self.end.clamp(0.0, 1.0)) * t)
    }
}

#[derive(Clone, Copy)]
struct Disc {
    center: Point,
    rx: f64,
    ry: f64,
}

#[derive(Default)]
pub struct Outline {
    polygons: Vec<Vec<Point>>,
    y_bounds: Vec<(f64, f64)>,
    discs: Vec<Disc>,
}
impl Outline {
    pub fn new(points: &[Point], style: Style, sx: f64, sy: f64) -> Self {
        Self::new_trimmed(points, style, sx, sy, 0.0, 1.0)
    }
    /// Radius is anchored to the original path, never renormalized by trimming.
    pub fn new_trimmed(
        points: &[Point],
        style: Style,
        sx: f64,
        sy: f64,
        start: f64,
        end: f64,
    ) -> Self {
        let mut result = Self::default();
        if !start.is_finite() || !end.is_finite() {
            return result;
        }
        let start = start.clamp(0.0, 1.0);
        let end = end.clamp(0.0, 1.0);
        if start >= end {
            return result;
        }
        let mut path: Vec<Point> = Vec::new();
        for &p in points {
            if !p.x.is_finite() || !p.y.is_finite() {
                return result;
            }
            if path.last().is_none_or(|q| p.sub(*q).length() > 1e-8) {
                path.push(p);
            }
        }
        let path = trim_path(&path, start, end);
        if path.len() < 2 || style.width <= 0.0 {
            return result;
        }
        let mut distances = vec![0.0];
        for pair in path.windows(2) {
            distances.push(distances.last().unwrap() + pair[1].sub(pair[0]).length());
        }
        let total = *distances.last().unwrap();
        if !total.is_finite() || total <= 1e-8 {
            return result;
        }
        let radii: Vec<f64> = distances
            .iter()
            .map(|s| style.radius(start + (end - start) * s / total))
            .collect();
        let (path, radii) = simplify(
            &path,
            &radii,
            &distances,
            0.03 / sx.abs().max(sy.abs()).max(1.0),
        );
        if style.round {
            // Convex hull of each pair of discs is the exact envelope for a
            // linearly changing radius. Tangent rails meet circles with C1 continuity.
            for i in 0..path.len() - 1 {
                if let Some(quad) = tangent_quad(path[i], radii[i], path[i + 1], radii[i + 1]) {
                    result.polygons.push(quad.to_vec());
                }
            }
            for (&p, &r) in path.iter().zip(&radii) {
                result.disc(p, r);
            }
        } else {
            let normals: Vec<Point> = path
                .windows(2)
                .map(|p| {
                    let d = p[1].sub(p[0]);
                    Point::new(-d.y, d.x).mul(1.0 / d.length())
                })
                .collect();
            let mut offsets = Vec::with_capacity(path.len());
            offsets.push(normals[0].mul(radii[0]));
            for i in 1..path.len() - 1 {
                let sum = normals[i - 1].add(normals[i]);
                let denom = sum.x * normals[i].x + sum.y * normals[i].y;
                if denom > 0.5 {
                    offsets.push(sum.mul(radii[i] / denom));
                } else {
                    // Bounded join at a sharp corner or cusp, avoiding an unbounded miter.
                    offsets.push(normals[i].mul(radii[i]));
                    result.disc(path[i], radii[i]);
                }
            }
            offsets.push(normals[normals.len() - 1].mul(*radii.last().unwrap()));
            for i in 0..path.len() - 1 {
                result.polygons.push(vec![
                    path[i].add(offsets[i]),
                    path[i + 1].add(offsets[i + 1]),
                    path[i + 1].sub(offsets[i + 1]),
                    path[i].sub(offsets[i]),
                ]);
            }
        }
        for disc in &mut result.discs {
            disc.center.x *= sx;
            disc.center.y *= sy;
            disc.rx *= sx.abs();
            disc.ry *= sy.abs();
        }
        for poly in &mut result.polygons {
            for p in poly {
                p.x *= sx;
                p.y *= sy;
            }
        }
        result.y_bounds = result
            .polygons
            .iter()
            .map(|poly| {
                poly.iter()
                    .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
                        (lo.min(p.y), hi.max(p.y))
                    })
            })
            .collect();
        result
    }
    fn disc(&mut self, p: Point, r: f64) {
        if r <= 1e-8 {
            return;
        }
        self.discs.push(Disc {
            center: p,
            rx: r,
            ry: r,
        });
    }
    /// Union spans before accumulating coverage: overlaps never darken or cancel out.
    /// Four vertical samples with exact horizontal coverage avoids a full supersample buffer.
    pub fn row(&self, y: f64, x_origin: f64, alpha: &mut [f32]) {
        alpha.fill(0.0);
        let mut spans = Vec::new();
        let mut hits = Vec::new();
        for sample in 0..4 {
            let yy = y + (sample as f64 + 0.5) / 4.0;
            spans.clear();
            for (poly, &(lo, hi)) in self.polygons.iter().zip(&self.y_bounds) {
                if yy < lo || yy >= hi {
                    continue;
                }
                hits.clear();
                for i in 0..poly.len() {
                    let a = poly[i];
                    let b = poly[(i + 1) % poly.len()];
                    if (a.y <= yy && b.y > yy) || (b.y <= yy && a.y > yy) {
                        hits.push(a.x + (yy - a.y) * (b.x - a.x) / (b.y - a.y));
                    }
                }
                hits.sort_by(f64::total_cmp);
                for pair in hits.chunks_exact(2) {
                    spans.push((pair[0], pair[1]));
                }
            }
            // Analytic ellipse spans: no polygon facets on highly magnified caps.
            for disc in &self.discs {
                if disc.rx <= 0.0 || disc.ry <= 0.0 {
                    continue;
                }
                let dy = (yy - disc.center.y) / disc.ry;
                if dy.abs() >= 1.0 {
                    continue;
                }
                let dx = disc.rx * (1.0 - dy * dy).sqrt();
                spans.push((disc.center.x - dx, disc.center.x + dx));
            }
            spans.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut merged: Option<(f64, f64)> = None;
            for &(l, r) in &spans {
                match merged {
                    Some((a, b)) if l <= b => merged = Some((a, b.max(r))),
                    Some((a, b)) => {
                        add_span(alpha, a - x_origin, b - x_origin);
                        merged = Some((l, r));
                    }
                    None => merged = Some((l, r)),
                }
            }
            if let Some((a, b)) = merged {
                add_span(alpha, a - x_origin, b - x_origin);
            }
        }
    }
}
/// External common tangent contacts. If one disc contains the other, their
/// union already is the hull: emitting a quad would fold or produce NaNs.
fn tangent_quad(a: Point, ra: f64, b: Point, rb: f64) -> Option<[Point; 4]> {
    let delta = b.sub(a);
    let distance = delta.length();
    if distance <= 1e-8 || (ra - rb).abs() >= distance {
        return None;
    }
    let u = delta.mul(1.0 / distance);
    let n = Point::new(-u.y, u.x);
    let k = (ra - rb) / distance;
    let h = (1.0 - k * k).max(0.0).sqrt();
    let top = u.mul(k).add(n.mul(h));
    let bottom = u.mul(k).sub(n.mul(h));
    Some([
        a.add(top.mul(ra)),
        b.add(top.mul(rb)),
        b.add(bottom.mul(rb)),
        a.add(bottom.mul(ra)),
    ])
}

/// Clip by original arc length, interpolating the two boundary points exactly.
/// Callers retain the original start/end fractions when evaluating width.
pub fn trim_path(points: &[Point], start: f64, end: f64) -> Vec<Point> {
    if points.len() < 2 || !start.is_finite() || !end.is_finite() {
        return Vec::new();
    }
    let start = start.clamp(0.0, 1.0);
    let end = end.clamp(0.0, 1.0);
    if start >= end {
        return Vec::new();
    }
    if start == 0.0 && end == 1.0 {
        return points.to_vec();
    }
    let total: f64 = points.windows(2).map(|p| p[1].sub(p[0]).length()).sum();
    let (from, to) = (start * total, end * total);
    let mut out = Vec::new();
    let mut at = 0.0;
    for p in points.windows(2) {
        let len = p[1].sub(p[0]).length();
        let next = at + len;
        if len > 1e-8 && next > from && at < to {
            let a = p[0].add(p[1].sub(p[0]).mul(((from - at) / len).clamp(0.0, 1.0)));
            let b = p[0].add(p[1].sub(p[0]).mul(((to - at) / len).clamp(0.0, 1.0)));
            if out.is_empty() {
                out.push(a);
            }
            out.push(b);
        }
        at = next;
        if at >= to {
            break;
        }
    }
    out
}

// Retain geometry AND width within a subpixel tolerance. A straight linear taper
// reduces to one quad rather than thousands, without changing its silhouette.
fn simplify(
    path: &[Point],
    radii: &[f64],
    distances: &[f64],
    tolerance: f64,
) -> (Vec<Point>, Vec<f64>) {
    let mut keep = vec![false; path.len()];
    keep[0] = true;
    keep[path.len() - 1] = true;
    let mut stack = vec![(0, path.len() - 1)];
    while let Some((a, b)) = stack.pop() {
        let mut worst = tolerance;
        let mut split = None;
        for i in a + 1..b {
            let t = (distances[i] - distances[a]) / (distances[b] - distances[a]);
            let predicted = path[a].add(path[b].sub(path[a]).mul(t));
            let error = path[i].sub(predicted).length()
                + (radii[i] - (radii[a] + (radii[b] - radii[a]) * t)).abs();
            if error > worst {
                worst = error;
                split = Some(i);
            }
        }
        if let Some(i) = split {
            keep[i] = true;
            stack.push((a, i));
            stack.push((i, b));
        }
    }
    (
        path.iter()
            .zip(&keep)
            .filter_map(|(&p, &k)| k.then_some(p))
            .collect(),
        radii
            .iter()
            .zip(&keep)
            .filter_map(|(&r, &k)| k.then_some(r))
            .collect(),
    )
}

fn add_span(row: &mut [f32], a: f64, b: f64) {
    let a = a.clamp(0.0, row.len() as f64);
    let b = b.clamp(0.0, row.len() as f64);
    for (x, value) in row
        .iter_mut()
        .enumerate()
        .take(b.ceil() as usize)
        .skip(a.floor() as usize)
    {
        *value += ((b.min(x as f64 + 1.0) - a.max(x as f64)).max(0.0) * 0.25) as f32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_ease_is_linear() {
        let s = Style {
            width: 100.0,
            start: 0.0,
            end: 0.2,
            ease: 1.0,
            round: false,
        };
        for (t, w) in [
            (0.0, 50.0),
            (0.25, 40.0),
            (0.5, 30.0),
            (0.75, 20.0),
            (1.0, 10.0),
        ] {
            assert!((s.radius(t) - w).abs() < 1e-10);
        }
    }
    #[test]
    fn trim_interpolates_by_distance_across_segments() {
        let p = [
            Point::new(0.0, 0.0),
            Point::new(80.0, 0.0),
            Point::new(80.0, 20.0),
        ];
        let clipped = trim_path(&p, 0.25, 0.90);
        assert_eq!(clipped.len(), 3);
        assert!((clipped[0].x - 25.0).abs() < 1e-10);
        assert!((clipped[2].y - 10.0).abs() < 1e-10);
        assert!(trim_path(&p, 0.5, 0.5).is_empty());
        assert!(trim_path(&p, 0.9, 0.1).is_empty());
        assert_eq!(trim_path(&p, 0.0, 1.0).len(), p.len());
    }
    #[test]
    fn trim_preserves_original_width_at_every_retained_position() {
        let p = [Point::new(8.0, 32.0), Point::new(120.0, 32.0)];
        let s = Style {
            start: 0.0,
            end: 0.2,
            ..style()
        };
        let full = Outline::new(&p, s, 1.0, 1.0);
        for (start, end) in [(0.0, 0.5), (0.25, 0.75), (0.75, 1.0), (0.99, 1.0)] {
            let trim = Outline::new_trimmed(&p, s, 1.0, 1.0, start, end);
            let mut a = [0.0; 128];
            let mut b = [0.0; 128];
            for y in 0..64 {
                full.row(y as f64, 0.0, &mut a);
                trim.row(y as f64, 0.0, &mut b);
                let first = (8.0 + 112.0 * start).ceil() as usize;
                let last = (8.0 + 112.0 * end).floor() as usize;
                for x in first..last {
                    assert!(
                        (a[x] - b[x]).abs() < 1e-5,
                        "trim={start}..{end} pixel={x},{y}"
                    );
                }
            }
        }
        assert!(Outline::new_trimmed(&p, s, 1.0, 1.0, 0.5, 0.5)
            .polygons
            .is_empty());
    }
    #[test]
    fn round_contacts_are_tangent_even_for_steep_taper() {
        let a = Point::new(10.0, 25.0);
        let b = Point::new(110.0, 25.0);
        let q = tangent_quad(a, 80.0, b, 20.0).unwrap();
        let rail = q[1].sub(q[0]);
        for (p, c, r) in [(q[0], a, 80.0), (q[1], b, 20.0)] {
            let normal = p.sub(c);
            assert!((normal.length() - r).abs() < 1e-10);
            assert!((normal.x * rail.x + normal.y * rail.y).abs() < 1e-9);
        }
        assert!(tangent_quad(a, 120.0, b, 20.0).is_none());
        assert!(tangent_quad(a, 140.0, b, 20.0).is_none());
        assert!(tangent_quad(a, 0.0, b, 0.0)
            .unwrap()
            .iter()
            .all(|p| p.x.is_finite()));
    }
    // Independent oracle: minimize squared distance minus squared radius over
    // a continuous linear family of discs. Does not use tangent contacts.
    fn disc_family_contains(p: Point, a: Point, b: Point, r0: f64, r1: f64) -> bool {
        let d = b.sub(a);
        let v = p.sub(a);
        let dr = r1 - r0;
        let aa = d.x * d.x + d.y * d.y - dr * dr;
        let bb = -2.0 * (v.x * d.x + v.y * d.y + r0 * dr);
        let cc = v.x * v.x + v.y * v.y - r0 * r0;
        let mut value = cc.min(aa + bb + cc);
        if aa > 0.0 {
            let t = (-bb / (2.0 * aa)).clamp(0.0, 1.0);
            value = value.min(aa * t * t + bb * t + cc);
        }
        value <= 0.0
    }
    #[test]
    fn round_raster_matches_continuous_disc_oracle() {
        for (a, b, r0, r1) in [
            (Point::new(22.0, 32.0), Point::new(65.0, 32.0), 20.0, 4.0),
            (Point::new(32.0, 32.0), Point::new(34.0, 32.0), 20.0, 4.0),
            (Point::new(22.0, 32.0), Point::new(65.0, 32.0), 20.0, 0.0),
        ] {
            let o = Outline::new(
                &[a, b],
                Style {
                    width: r0 * 2.0,
                    start: 0.0,
                    end: r1 / r0,
                    ease: 1.0,
                    round: true,
                },
                1.0,
                1.0,
            );
            let mut row = [0.0; 96];
            for y in 8..56 {
                o.row(y as f64, 0.0, &mut row);
                for (x, &actual) in row.iter().enumerate() {
                    let mut count = 0;
                    for sy in 0..4 {
                        for sx in 0..128 {
                            let p = Point::new(
                                x as f64 + (sx as f64 + 0.5) / 128.0,
                                y as f64 + (sy as f64 + 0.5) / 4.0,
                            );
                            count += disc_family_contains(p, a, b, r0, r1) as u32;
                        }
                    }
                    assert!(
                        (actual - count as f32 / 512.0).abs() < 0.009,
                        "pixel={x},{y} actual={actual}"
                    );
                }
            }
        }
    }
    #[test]
    fn negligible_trim_is_empty() {
        let p = [Point::new(0.0, 0.0), Point::new(100.0, 0.0)];
        for round in [false, true] {
            let o =
                Outline::new_trimmed(&p, Style { round, ..style() }, 1.0, 1.0, 0.5, 0.5 + 1e-12);
            assert!(o.polygons.is_empty() && o.discs.is_empty());
        }
    }
    #[test]
    fn curved_round_matches_unsimplified_disc_oracle() {
        let sine: Vec<_> = (0..=32)
            .map(|i| {
                let t = i as f64 / 32.0;
                Point::new(
                    12.0 + 40.0 * t,
                    32.0 + 10.0 * (t * std::f64::consts::TAU).sin(),
                )
            })
            .collect();
        let tight: Vec<_> = (0..=32)
            .map(|i| {
                let t = i as f64 / 32.0 * std::f64::consts::PI;
                Point::new(32.0 + 6.0 * t.cos(), 32.0 + 6.0 * t.sin())
            })
            .collect();
        let hairpin = vec![
            Point::new(12.0, 30.0),
            Point::new(52.0, 30.0),
            Point::new(12.0, 34.0),
        ];
        let s = Style {
            width: 20.0,
            start: 0.0,
            end: 0.2,
            ease: 1.0,
            round: true,
        };
        for path in [sine, tight, hairpin] {
            let mut lengths = vec![0.0];
            for pair in path.windows(2) {
                lengths.push(lengths.last().unwrap() + pair[1].sub(pair[0]).length());
            }
            let total = *lengths.last().unwrap();
            let radii: Vec<_> = lengths.iter().map(|d| s.radius(d / total)).collect();
            let full = Outline::new(&path, s, 1.0, 1.0);
            let trimmed = Outline::new_trimmed(&path, s, 1.0, 1.0, 0.9, 1.0);
            let mut area = 0.0_f64;
            let mut expected_area = 0.0_f64;
            for y in 0..64 {
                let mut row = [0.0; 64];
                let mut trim_row = [0.0; 64];
                full.row(y as f64, 0.0, &mut row);
                trimmed.row(y as f64, 0.0, &mut trim_row);
                for (x, &actual) in row.iter().enumerate() {
                    let mut count = 0;
                    for sy in 0..4 {
                        for sx in 0..64 {
                            let p = Point::new(
                                x as f64 + (sx as f64 + 0.5) / 64.0,
                                y as f64 + (sy as f64 + 0.5) / 4.0,
                            );
                            count += path.windows(2).enumerate().any(|(i, q)| {
                                disc_family_contains(p, q[0], q[1], radii[i], radii[i + 1])
                            }) as u32;
                        }
                    }
                    let expected = count as f32 / 256.0;
                    assert!(
                        (actual - expected).abs() < 0.1,
                        "pixel={x},{y}: {actual} vs {expected}"
                    );
                    assert!(
                        trim_row[x] <= actual + 0.02,
                        "trim exceeds original at {x},{y}"
                    );
                    area += actual as f64;
                    expected_area += expected as f64;
                }
            }
            assert!((area - expected_area).abs() / expected_area < 0.005);
        }
    }
    #[test]
    fn dense_straight_line_reduces_to_one_quad() {
        let p: Vec<_> = (0..4000)
            .map(|i| Point::new(i as f64 / 2.0, 10.0))
            .collect();
        let o = Outline::new(
            &p,
            Style {
                start: 0.0,
                ..style()
            },
            1.0,
            1.0,
        );
        assert_eq!(o.polygons.len(), 1);
    }
    fn style() -> Style {
        Style {
            width: 20.0,
            start: 0.25,
            end: 0.0,
            ease: 1.0,
            round: false,
        }
    }
    fn image(points: &[Point], s: Style) -> Vec<f32> {
        let outline = Outline::new(points, s, 1.0, 1.0);
        let mut pixels = vec![0.0; 128 * 64];
        for y in 0..64 {
            outline.row(y as f64, 0.0, &mut pixels[y * 128..(y + 1) * 128]);
        }
        pixels
    }
    #[test]
    fn straight_tapers_to_point_and_has_transparent_background() {
        let p = image(&[Point::new(16.0, 32.0), Point::new(112.0, 32.0)], style());
        assert_eq!(p[32 * 128 + 16], 1.0);
        assert_eq!(p[5 * 128 + 50], 0.0);
        assert!(p[25 * 128 + 25] > 0.9);
        assert_eq!(p[25 * 128 + 110], 0.0);
        assert_eq!(p[32 * 128 + 112], 0.0);
    }
    #[test]
    fn flat_and_round_caps_differ_at_both_ends() {
        let mut s = style();
        s.end = 1.0;
        let path = [Point::new(16.0, 32.0), Point::new(112.0, 32.0)];
        let flat = image(&path, s);
        s.round = true;
        let round = image(&path, s);
        for x in [10, 117] {
            assert_eq!(flat[32 * 128 + x], 0.0);
            assert!(round[32 * 128 + x] > 0.9);
        }
    }
    #[test]
    fn duplicated_points_and_self_crossings_are_finite_union() {
        let path = [
            Point::new(16.0, 16.0),
            Point::new(16.0, 16.0),
            Point::new(100.0, 50.0),
            Point::new(100.0, 16.0),
            Point::new(16.0, 50.0),
        ];
        let p = image(&path, style());
        assert!(p
            .iter()
            .all(|v| v.is_finite() && *v >= 0.0 && *v <= 1.00001));
        assert!(image(&[path[0], path[0]], style())
            .iter()
            .all(|v| *v == 0.0));
    }
    #[test]
    fn width_profile_is_monotone_and_honors_endpoints() {
        for ease in [0.1, 1.0, 10.0] {
            let s = Style { ease, ..style() };
            let mut prev = 10.0;
            for i in 0..1001 {
                let r = s.radius(i as f64 / 1000.0);
                assert!(r <= prev + 1e-10);
                prev = r;
            }
            assert_eq!(s.radius(0.25), 10.0);
            assert_eq!(s.radius(1.0), 0.0);
        }
        assert_eq!(
            Style {
                start: 1.0,
                ..style()
            }
            .radius(1.0),
            10.0
        );
    }
    #[test]
    fn tiled_rows_equal_full_frame() {
        let outline = Outline::new(
            &[Point::new(10.0, 10.0), Point::new(110.0, 50.0)],
            style(),
            0.5,
            0.25,
        );
        let mut full = [0.0; 128];
        let mut tile = [0.0; 20];
        for y in 0..64 {
            outline.row(y as f64, 0.0, &mut full);
            outline.row(y as f64, 23.0, &mut tile);
            assert_eq!(&full[23..43], &tile);
        }
    }
    #[test]
    fn taper_uses_distance_not_vertex_index() {
        let uniform: Vec<_> = (0..=192)
            .map(|i| Point::new(16.0 + i as f64 * 0.5, 32.0))
            .collect();
        let clustered: Vec<_> = (0..=1000)
            .map(|i| Point::new(16.0 + 96.0 * (i as f64 / 1000.0).powi(2), 32.0))
            .collect();
        let a = image(&uniform, style());
        let b = image(&clustered, style());
        assert!(a.iter().zip(b).all(|(a, b)| (a - b).abs() < 0.01));
        assert_eq!(a[23 * 128 + 35], 1.0); // Before the 25% taper start, retain full width.
    }
    #[test]
    fn curved_path_updates_without_stale_pixels() {
        let path: Vec<_> = (0..=400)
            .map(|i| {
                let t = i as f64 / 400.0;
                Point::new(
                    16.0 + 96.0 * t,
                    32.0 + 12.0 * (t * std::f64::consts::TAU).sin(),
                )
            })
            .collect();
        let normal = image(&path, style());
        let wide = image(
            &path,
            Style {
                width: 32.0,
                ..style()
            },
        );
        assert!(wide.iter().sum::<f32>() > normal.iter().sum::<f32>());
        assert_eq!(image(&path, style()), normal);
        assert!(image(
            &path,
            Style {
                width: 0.0,
                ..style()
            }
        )
        .iter()
        .all(|v| *v == 0.0));
        assert!(normal.iter().any(|v| *v > 0.0 && *v < 1.0));
    }
}
