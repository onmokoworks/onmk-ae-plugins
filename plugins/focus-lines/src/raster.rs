//! Scene representation, spatial bins and the CPU reference rasterizer.
//!
//! Coverage policy shared with the GPU shader (`shader.wgsl`):
//! S horizontal scanlines per pixel row at `y + (s + 0.5) / S`; per scanline the
//! geometric UNION of all primitive intervals is formed, the hole-ellipse interval
//! is subtracted, and the overlap with each pixel `[x, x + 1)` is integrated
//! analytically. Vertical direction is sampled, so this is NOT an exact area.

pub const TILE: i32 = 16;
pub const MAX_VERTS: usize = 8;
/// Upper bound for tile list entries (u32 each, 128 MiB).
pub const MAX_BIN_ITEMS: usize = 32 << 20;
/// Upper bound for pixels in one coverage request.
pub const MAX_PIXELS: u64 = 1 << 28;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Prim {
    pub n: u8,
    pub v: [[f64; 2]; MAX_VERTS],
}

impl Prim {
    /// Convex polygon in absolute buffer coordinates, 3..=8 finite vertices.
    pub fn from_points(points: &[[f64; 2]]) -> Option<Prim> {
        if points.len() < 3 || points.len() > MAX_VERTS {
            return None;
        }
        if points
            .iter()
            .any(|p| !p[0].is_finite() || !p[1].is_finite())
        {
            return None;
        }
        let mut v = [[0.0; 2]; MAX_VERTS];
        v[..points.len()].copy_from_slice(points);
        Some(Prim {
            n: points.len() as u8,
            v,
        })
    }

    pub fn verts(&self) -> &[[f64; 2]] {
        &self.v[..self.n as usize]
    }

    pub fn y_range(&self) -> (f64, f64) {
        self.verts()
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
                (lo.min(p[1]), hi.max(p[1]))
            })
    }

    /// Interval of the polygon on the horizontal line `y = ys`. Edges use the
    /// half-open rule so a sample on a shared edge belongs to exactly one side.
    pub fn scan(&self, ys: f64) -> Option<(f64, f64)> {
        let v = self.verts();
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        let mut hits = 0;
        for i in 0..v.len() {
            let a = v[i];
            let b = v[(i + 1) % v.len()];
            if (a[1] <= ys && b[1] > ys) || (b[1] <= ys && a[1] > ys) {
                let x = a[0] + (ys - a[1]) * (b[0] - a[0]) / (b[1] - a[1]);
                lo = lo.min(x);
                hi = hi.max(x);
                hits += 1;
            }
        }
        (hits >= 2 && hi > lo).then_some((lo, hi))
    }

    /// X extent of the polygon restricted to `y0 <= y <= y1` (convex).
    pub fn slab_x_range(&self, y0: f64, y1: f64) -> Option<(f64, f64)> {
        let v = self.verts();
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for i in 0..v.len() {
            let a = v[i];
            let b = v[(i + 1) % v.len()];
            if a[1] >= y0 && a[1] <= y1 {
                lo = lo.min(a[0]);
                hi = hi.max(a[0]);
            }
            if a[1] != b[1] {
                for yb in [y0, y1] {
                    if yb >= a[1].min(b[1]) && yb <= a[1].max(b[1]) {
                        let x = a[0] + (yb - a[1]) * (b[0] - a[0]) / (b[1] - a[1]);
                        lo = lo.min(x);
                        hi = hi.max(x);
                    }
                }
            }
        }
        (lo <= hi).then_some((lo, hi))
    }
}

/// Elliptical hole: points with `|M (p - c)| < 1` never receive line coverage.
/// `m` is row-major: `u = m[0][0] dx + m[0][1] dy`, `v = m[1][0] dx + m[1][1] dy`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hole {
    pub c: [f64; 2],
    pub m: [[f64; 2]; 2],
}

impl Hole {
    pub fn contains(&self, p: [f64; 2]) -> bool {
        let dx = p[0] - self.c[0];
        let dy = p[1] - self.c[1];
        let u = self.m[0][0] * dx + self.m[0][1] * dy;
        let v = self.m[1][0] * dx + self.m[1][1] * dy;
        u * u + v * v < 1.0
    }

    /// Hole interval on scanline `ys` (same formula as the shader).
    pub fn scan(&self, ys: f64) -> Option<(f64, f64)> {
        let [[m00, m01], [m10, m11]] = self.m;
        let dy = ys - self.c[1];
        let a = m00 * m00 + m10 * m10;
        let b = 2.0 * (m00 * m01 + m10 * m11) * dy;
        let c = (m01 * m01 + m11 * m11) * dy * dy - 1.0;
        let disc = b * b - 4.0 * a * c;
        if a.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater)
            || disc.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater)
        {
            return None;
        }
        let r = disc.sqrt();
        Some((
            self.c[0] + (-b - r) / (2.0 * a),
            self.c[0] + (-b + r) / (2.0 * a),
        ))
    }
}

#[derive(Clone, Debug, Default)]
pub struct Scene {
    pub prims: Vec<Prim>,
    pub hole: Option<Hole>,
}

/// Output rectangle in absolute buffer pixels: x in [x0, x0 + w), y in [y0, y0 + h).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelRect {
    pub x0: i32,
    pub y0: i32,
    pub w: u32,
    pub h: u32,
}

impl PixelRect {
    pub fn pixels(&self) -> u64 {
        u64::from(self.w) * u64::from(self.h)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RasterError {
    TooLarge(&'static str),
}

impl std::fmt::Display for RasterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RasterError::TooLarge(what) => write!(f, "render too large: {what}"),
        }
    }
}

/// Candidate lists on an absolute 16 px tile grid covering one output rect.
/// Lists are in ascending primitive order (deterministic).
#[derive(Clone, Debug, Default)]
pub struct Bins {
    pub tx0: i32,
    pub ty0: i32,
    pub tiles_x: u32,
    pub tiles_y: u32,
    /// `tiles_x * tiles_y + 1` prefix offsets into `tile_items`.
    pub tile_offsets: Vec<u32>,
    pub tile_items: Vec<u32>,
    /// `tiles_y + 1` prefix offsets into `band_items` (one band = one tile row).
    pub band_offsets: Vec<u32>,
    pub band_items: Vec<u32>,
}

pub fn bin(scene: &Scene, rect: PixelRect) -> Result<Bins, RasterError> {
    if rect.pixels() > MAX_PIXELS {
        return Err(RasterError::TooLarge("pixels"));
    }
    if rect.w == 0 || rect.h == 0 {
        return Ok(Bins {
            tile_offsets: vec![0],
            band_offsets: vec![0],
            ..Bins::default()
        });
    }
    if scene.prims.len() > u32::MAX as usize {
        return Err(RasterError::TooLarge("primitives"));
    }
    let x0 = i64::from(rect.x0);
    let y0 = i64::from(rect.y0);
    let x1 = x0 + i64::from(rect.w);
    let y1 = y0 + i64::from(rect.h);
    let t = i64::from(TILE);
    let tx0 = x0.div_euclid(t);
    let ty0 = y0.div_euclid(t);
    let tiles_x = ((x1 - 1).div_euclid(t) - tx0 + 1) as usize;
    let tiles_y = ((y1 - 1).div_euclid(t) - ty0 + 1) as usize;
    let mut tiles: Vec<Vec<u32>> = vec![Vec::new(); tiles_x * tiles_y];
    let mut bands: Vec<Vec<u32>> = vec![Vec::new(); tiles_y];
    let mut total = 0usize;
    for (index, prim) in scene.prims.iter().enumerate() {
        let (ymin, ymax) = prim.y_range();
        let ylo = ymin.max(y0 as f64);
        let yhi = ymax.min(y1 as f64);
        if yhi.partial_cmp(&ylo) != Some(std::cmp::Ordering::Greater) {
            continue;
        }
        let ra = ((ylo.floor() as i64).div_euclid(t) - ty0).max(0);
        let rb = ((yhi.floor() as i64).div_euclid(t) - ty0).min(tiles_y as i64 - 1);
        for row in ra..=rb {
            let sy0 = (((row + ty0) * t) as f64).max(ylo);
            let sy1 = (((row + ty0 + 1) * t) as f64).min(yhi);
            if sy1 < sy0 {
                continue;
            }
            let Some((xa, xb)) = prim.slab_x_range(sy0, sy1) else {
                continue;
            };
            let xa = xa.max(x0 as f64);
            let xb = xb.min(x1 as f64);
            if xb.partial_cmp(&xa) != Some(std::cmp::Ordering::Greater) {
                continue;
            }
            let ca = ((xa.floor() as i64).div_euclid(t) - tx0).max(0);
            let cb = ((xb.floor() as i64).div_euclid(t) - tx0).min(tiles_x as i64 - 1);
            if cb < ca {
                continue;
            }
            bands[row as usize].push(index as u32);
            for col in ca..=cb {
                tiles[row as usize * tiles_x + col as usize].push(index as u32);
            }
            total += (cb - ca + 1) as usize;
            if total > MAX_BIN_ITEMS {
                return Err(RasterError::TooLarge("tile list entries"));
            }
        }
    }
    let flatten = |lists: Vec<Vec<u32>>| {
        let mut offsets = Vec::with_capacity(lists.len() + 1);
        let mut items = Vec::new();
        offsets.push(0u32);
        for list in lists {
            items.extend_from_slice(&list);
            offsets.push(items.len() as u32);
        }
        (offsets, items)
    };
    let (tile_offsets, tile_items) = flatten(tiles);
    let (band_offsets, band_items) = flatten(bands);
    Ok(Bins {
        tx0: tx0 as i32,
        ty0: ty0 as i32,
        tiles_x: tiles_x as u32,
        tiles_y: tiles_y as u32,
        tile_offsets,
        tile_items,
        band_offsets,
        band_items,
    })
}

/// Clamp the sample count to the two supported quality levels.
pub fn samples_for(quality_high: bool) -> u32 {
    if quality_high {
        16
    } else {
        8
    }
}

/// CPU reference coverage for `rect`, row-major `w * h`, values in [0, 1].
pub fn cpu_coverage(scene: &Scene, rect: PixelRect, samples: u32, bins: &Bins) -> Vec<f32> {
    let w = rect.w as usize;
    let h = rect.h as usize;
    let mut out = vec![0.0f32; w * h];
    if w == 0 || h == 0 || scene.prims.is_empty() {
        return out;
    }
    let samples = samples.clamp(1, 64);
    let weight = 1.0 / f64::from(samples);
    let xl = f64::from(rect.x0);
    let xr = xl + w as f64;
    let mut row = vec![0.0f64; w];
    let mut intervals: Vec<(f64, f64)> = Vec::new();
    for j in 0..h {
        let ay = i64::from(rect.y0) + j as i64;
        let band = (ay.div_euclid(i64::from(TILE)) - i64::from(bins.ty0)) as usize;
        let candidates = &bins.band_items
            [bins.band_offsets[band] as usize..bins.band_offsets[band + 1] as usize];
        if candidates.is_empty() {
            continue;
        }
        row.fill(0.0);
        for s in 0..samples {
            let ys = ay as f64 + (f64::from(s) + 0.5) / f64::from(samples);
            intervals.clear();
            for &p in candidates {
                if let Some((a, b)) = scene.prims[p as usize].scan(ys) {
                    let a = a.max(xl);
                    let b = b.min(xr);
                    if b > a {
                        intervals.push((a, b));
                    }
                }
            }
            if intervals.is_empty() {
                continue;
            }
            intervals.sort_by(|p, q| p.0.total_cmp(&q.0).then(p.1.total_cmp(&q.1)));
            let hole = scene.hole.and_then(|hole| hole.scan(ys));
            let mut current = intervals[0];
            for &(a, b) in &intervals[1..] {
                if a <= current.1 {
                    current.1 = current.1.max(b);
                } else {
                    emit(&mut row, current, hole, xl, weight);
                    current = (a, b);
                }
            }
            emit(&mut row, current, hole, xl, weight);
        }
        for (dst, &src) in out[j * w..(j + 1) * w].iter_mut().zip(&row) {
            *dst = src.clamp(0.0, 1.0) as f32;
        }
    }
    out
}

fn emit(row: &mut [f64], (a, b): (f64, f64), hole: Option<(f64, f64)>, xl: f64, weight: f64) {
    match hole {
        Some((hl, hr)) if hr > a && hl < b => {
            if hl > a {
                add_span(row, a, hl, xl, weight);
            }
            if hr < b {
                add_span(row, hr, b, xl, weight);
            }
        }
        _ => add_span(row, a, b, xl, weight),
    }
}

/// Add `[a, b)` (absolute x) to the row starting at absolute `xl`. Overlaps are
/// computed in absolute coordinates so results do not depend on the ROI origin.
fn add_span(row: &mut [f64], a: f64, b: f64, xl: f64, weight: f64) {
    let a = a.max(xl);
    let b = b.min(xl + row.len() as f64);
    if b.partial_cmp(&a) != Some(std::cmp::Ordering::Greater) {
        return;
    }
    let first = ((a - xl).floor() as i64 - 1).max(0) as usize;
    let last = (((b - xl).ceil() as i64 + 1).max(0) as usize).min(row.len());
    for (x, value) in row.iter_mut().enumerate().take(last).skip(first) {
        let px = xl + x as f64;
        let overlap = b.min(px + 1.0) - a.max(px);
        if overlap > 0.0 {
            *value += overlap * weight;
        }
    }
}

/// Coverage for a whole rect using the CPU reference (bins computed here).
pub fn cpu_render(scene: &Scene, rect: PixelRect, samples: u32) -> Result<Vec<f32>, RasterError> {
    let bins = bin(scene, rect)?;
    Ok(cpu_coverage(scene, rect, samples, &bins))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect_prim(x0: f64, y0: f64, x1: f64, y1: f64) -> Prim {
        Prim::from_points(&[[x0, y0], [x1, y0], [x1, y1], [x0, y1]]).unwrap()
    }

    #[test]
    fn scan_uses_half_open_edges() {
        let p = rect_prim(0.0, 0.0, 4.0, 2.0);
        assert!(p.scan(0.0).is_some());
        assert!(p.scan(2.0).is_none());
        assert!(p.scan(-0.1).is_none());
    }

    #[test]
    fn bins_include_long_diagonal_only_along_its_path() {
        let p =
            Prim::from_points(&[[0.0, 0.0], [1.0, 0.0], [256.0, 255.0], [255.0, 256.0]]).unwrap();
        let scene = Scene {
            prims: vec![p],
            hole: None,
        };
        let bins = bin(
            &scene,
            PixelRect {
                x0: 0,
                y0: 0,
                w: 256,
                h: 256,
            },
        )
        .unwrap();
        assert_eq!(bins.tiles_x, 16);
        // A diagonal touches about 2-3 tiles per row, far fewer than the 256 of its bbox.
        assert!(bins.tile_items.len() < 16 * 4, "{}", bins.tile_items.len());
    }
}
