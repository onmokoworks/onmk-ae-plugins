//! Shared shadow mathematics (docs/design.md §4). Both backends consume the
//! same [`Plan`]; nothing here depends on After Effects or wgpu.

/// Valid parameter ranges (user pixels, before downsample / PAR).
pub const MAX_DISTANCE: f64 = 1000.0;
pub const MAX_SIZE: f64 = 250.0;
/// Hard guards in buffer pixels; unreachable with valid parameters and the
/// accepted PAR / downsample range, but keep kernels and spans bounded.
pub const MAX_BUFFER_RADIUS: f64 = 8192.0;
/// Largest computation domain (Ω) accepted by either backend.
pub const MAX_PLANE_PIXELS: usize = 1 << 26;

const OFFSET_GRID: f64 = 65536.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blend {
    Normal,
    Multiply,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuMode {
    Auto,
    Gpu,
    Cpu,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowParams {
    pub blend: Blend,
    /// Straight shadow colour in working space (HDR allowed in 32 bpc).
    pub color: [f32; 3],
    /// 0..=1
    pub opacity: f64,
    /// Layer Style convention: direction the light comes from, 0° = right,
    /// counter-clockwise positive, image y axis pointing down.
    pub angle_deg: f64,
    /// User pixels.
    pub distance: f64,
    /// 0..=1 fraction of `size` spent as hard matte choke.
    pub choke: f64,
    /// User pixels; total reach of choke + blur.
    pub size: f64,
}

impl Default for ShadowParams {
    fn default() -> Self {
        Self {
            blend: Blend::Multiply,
            color: [0.0, 0.0, 0.0],
            opacity: 0.75,
            angle_deg: 120.0,
            distance: 5.0,
            choke: 0.0,
            size: 5.0,
        }
    }
}

/// Host resolution factors: buffer px = user px * ds (x additionally / par).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scale {
    pub dsx: f64,
    pub dsy: f64,
    /// Pixel aspect ratio, width over height.
    pub par: f64,
}

impl Scale {
    pub const FULL: Scale = Scale {
        dsx: 1.0,
        dsy: 1.0,
        par: 1.0,
    };

    pub fn new(dsx: f64, dsy: f64, par: f64) -> Result<Self, PlanError> {
        let ds_ok = |v: f64| v.is_finite() && v > 0.0 && v <= 16.0;
        if !ds_ok(dsx) || !ds_ok(dsy) {
            return Err(PlanError::Downsample);
        }
        if !(par.is_finite() && (0.05..=20.0).contains(&par)) {
            return Err(PlanError::PixelAspect);
        }
        Ok(Self { dsx, dsy, par })
    }

    /// Converts a host rational. Only a structurally invalid rational
    /// (zero denominator or non-positive numerator) falls back to 1.
    pub fn ratio(num: i64, den: u64) -> f64 {
        if den == 0 || num <= 0 {
            1.0
        } else {
            num as f64 / den as f64
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanError {
    Downsample,
    PixelAspect,
    RadiusTooLarge,
}

/// Exclusive-right/bottom rectangle in (downsampled) layer coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Rect64 {
    pub left: i64,
    pub top: i64,
    pub right: i64,
    pub bottom: i64,
}

impl Rect64 {
    pub fn new(left: i64, top: i64, right: i64, bottom: i64) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }
    pub fn width(&self) -> i64 {
        (self.right - self.left).max(0)
    }
    pub fn height(&self) -> i64 {
        (self.bottom - self.top).max(0)
    }
    pub fn is_empty(&self) -> bool {
        self.right <= self.left || self.bottom <= self.top
    }
    pub fn intersect(&self, o: &Rect64) -> Rect64 {
        let r = Rect64 {
            left: self.left.max(o.left),
            top: self.top.max(o.top),
            right: self.right.min(o.right),
            bottom: self.bottom.min(o.bottom),
        };
        if r.is_empty() {
            Rect64::default()
        } else {
            r
        }
    }
    pub fn union(&self, o: &Rect64) -> Rect64 {
        if self.is_empty() {
            return *o;
        }
        if o.is_empty() {
            return *self;
        }
        Rect64 {
            left: self.left.min(o.left),
            top: self.top.min(o.top),
            right: self.right.max(o.right),
            bottom: self.bottom.max(o.bottom),
        }
    }
}

/// One row of the elliptical structuring element: |i| <= half_width at
/// row offset dy. `level` = floor(log2(2*half_width+1)) selects the sparse
/// max table used to answer the span in O(1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub dy: i32,
    pub half_width: i32,
    pub level: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Dilation {
    /// Sorted by (level, dy).
    pub spans: Vec<Span>,
    /// Number of sparse-table levels needed (max level + 1).
    pub levels: u32,
    pub reach_x: i64,
    pub reach_y: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    /// True overlays the shadow on the source; false returns straight shadow RGBA.
    pub keep_source: bool,
    /// Opacity 0: bit-exact copy, no mask, no GPU initialisation.
    pub identity: bool,
    pub blend: Blend,
    pub color: [f32; 3],
    pub opacity: f32,
    /// floor of the buffer-pixel shadow offset.
    pub shift_x: i64,
    pub shift_y: i64,
    /// offset - floor, exactly representable (2^-16 grid), in [0,1).
    pub frac_x: f32,
    pub frac_y: f32,
    /// None = radius 0 (D = E).
    pub dilate_lo: Option<Dilation>,
    pub dilate_hi: Option<Dilation>,
    /// D = D_lo + (D_hi - D_lo) * dilate_mix
    pub dilate_mix: f32,
    pub kernel_x: Vec<f32>,
    pub kernel_y: Vec<f32>,
    pub pad_x: i64,
    pub pad_y: i64,
}

pub fn sanitize(p: &ShadowParams) -> ShadowParams {
    let clamp = |v: f64, lo: f64, hi: f64| if v.is_nan() { lo } else { v.clamp(lo, hi) };
    let color = p.color.map(|c| if c.is_finite() { c } else { 0.0 });
    ShadowParams {
        blend: p.blend,
        color,
        opacity: clamp(p.opacity, 0.0, 1.0),
        angle_deg: if p.angle_deg.is_finite() {
            p.angle_deg
        } else {
            0.0
        },
        distance: clamp(p.distance, 0.0, MAX_DISTANCE),
        choke: clamp(p.choke, 0.0, 1.0),
        size: clamp(p.size, 0.0, MAX_SIZE),
    }
}

/// Buffer-pixel offset (ox, oy) of the shadow, snapped to the 2^-16 grid.
pub fn offset(p: &ShadowParams, s: &Scale) -> (f64, f64) {
    let theta = p.angle_deg.to_radians();
    let snap = |v: f64| {
        let r = (v * OFFSET_GRID).round() / OFFSET_GRID;
        if r == 0.0 {
            0.0
        } else {
            r
        }
    };
    (
        snap(p.distance * -theta.cos() * s.dsx / s.par),
        snap(p.distance * theta.sin() * s.dsy),
    )
}

fn level_for(half_width: i32) -> u32 {
    let width = 2 * half_width as u32 + 1;
    31 - width.leading_zeros()
}

/// Half width of the structuring-element row `dy` for buffer radii (rx, ry).
pub fn half_width(rx: f64, ry: f64, dy: i32) -> i32 {
    let q = if ry > 0.0 {
        let t = dy as f64 / ry;
        (1.0 - t * t).max(0.0)
    } else {
        1.0
    };
    (rx * q.sqrt() + 1e-6).floor() as i32
}

/// Structuring element for a user radius `r`; None when it is the identity.
pub fn dilation(r: f64, s: &Scale) -> Result<Option<Dilation>, PlanError> {
    if r <= 0.0 {
        return Ok(None);
    }
    let rx = r * s.dsx / s.par;
    let ry = r * s.dsy;
    if !(rx.is_finite() && ry.is_finite()) || rx > MAX_BUFFER_RADIUS || ry > MAX_BUFFER_RADIUS {
        return Err(PlanError::RadiusTooLarge);
    }
    let ryi = (ry + 1e-9).floor() as i32;
    let mut spans: Vec<Span> = (-ryi..=ryi)
        .map(|dy| {
            let hw = half_width(rx, ry, dy);
            Span {
                dy,
                half_width: hw,
                level: level_for(hw),
            }
        })
        .collect();
    if spans.iter().all(|sp| sp.half_width == 0 && sp.dy == 0) {
        return Ok(None);
    }
    spans.sort_by_key(|sp| (sp.level, sp.dy));
    let levels = spans.iter().map(|sp| sp.level).max().unwrap_or(0) + 1;
    let reach_x = spans
        .iter()
        .map(|sp| sp.half_width as i64)
        .max()
        .unwrap_or(0);
    Ok(Some(Dilation {
        spans,
        levels,
        reach_x,
        reach_y: ryi as i64,
    }))
}

/// Normalised sampled Gaussian, support ceil(3 sigma). sigma <= 1e-4 -> [1].
pub fn kernel(sigma: f64) -> Result<Vec<f32>, PlanError> {
    if !(sigma > 1e-4) {
        return Ok(vec![1.0]);
    }
    let radius = (3.0 * sigma - 1e-9).ceil().max(1.0);
    if !radius.is_finite() || radius > MAX_BUFFER_RADIUS {
        return Err(PlanError::RadiusTooLarge);
    }
    let radius = radius as i64;
    let weights: Vec<f64> = (-radius..=radius)
        .map(|i| (-((i * i) as f64) / (2.0 * sigma * sigma)).exp())
        .collect();
    let sum: f64 = weights.iter().sum();
    Ok(weights.iter().map(|w| (w / sum) as f32).collect())
}

impl Plan {
    pub fn new(params: &ShadowParams, scale: &Scale) -> Result<Plan, PlanError> {
        let p = sanitize(params);
        let (ox, oy) = offset(&p, scale);
        let (fx, fy) = (ox.floor(), oy.floor());
        let r_m = p.choke * p.size;
        let r_b = (p.size - r_m).max(0.0);
        let (dilate_lo, dilate_hi, mix) = if r_m > 0.0 {
            let r0 = r_m.floor();
            let f = r_m - r0;
            let lo = dilation(r0, scale)?;
            let hi = if f > 0.0 {
                dilation(r0 + 1.0, scale)?
            } else {
                None
            };
            (lo, hi, if f > 0.0 { f as f32 } else { 0.0 })
        } else {
            (None, None, 0.0)
        };
        let sigma = r_b / 3.0;
        let kernel_x = kernel(sigma * scale.dsx / scale.par)?;
        let kernel_y = kernel(sigma * scale.dsy)?;
        let reach = |f: fn(&Dilation) -> i64| {
            [&dilate_lo, &dilate_hi]
                .iter()
                .filter_map(|d| d.as_ref().map(f))
                .max()
                .unwrap_or(0)
        };
        let pad_x = (kernel_x.len() / 2) as i64 + reach(|d| d.reach_x);
        let pad_y = (kernel_y.len() / 2) as i64 + reach(|d| d.reach_y);
        Ok(Plan {
            keep_source: true,
            identity: p.opacity == 0.0,
            blend: p.blend,
            color: p.color,
            opacity: p.opacity as f32,
            shift_x: fx as i64,
            shift_y: fy as i64,
            frac_x: (ox - fx) as f32,
            frac_y: (oy - fy) as f32,
            dilate_lo,
            dilate_hi,
            dilate_mix: mix,
            kernel_x,
            kernel_y,
            pad_x,
            pad_y,
        })
    }

    /// Bilinear weights (w(x0), w(x0+1)) shared by every pixel.
    pub fn weights_x(&self) -> (f32, f32) {
        weights(self.frac_x)
    }
    pub fn weights_y(&self) -> (f32, f32) {
        weights(self.frac_y)
    }
    /// x0 = p + base_x for output pixel p (layer coordinates).
    pub fn base_x(&self) -> i64 {
        -self.shift_x - (self.frac_x > 0.0) as i64
    }
    pub fn base_y(&self) -> i64 {
        -self.shift_y - (self.frac_y > 0.0) as i64
    }

    /// Computation domain Ω for an output rectangle. Every value of the blurred
    /// mask sampled by `out` is exact inside Ω (exterior beyond Ω is 1).
    pub fn omega(&self, out: &Rect64) -> Rect64 {
        if out.is_empty() {
            return Rect64::default();
        }
        Rect64 {
            left: out.left + self.base_x() - self.pad_x,
            top: out.top + self.base_y() - self.pad_y,
            right: out.right - self.shift_x + self.pad_x,
            bottom: out.bottom - self.shift_y + self.pad_y,
        }
    }

    /// Input region to request from the host for an output request.
    pub fn input_rect(&self, out: &Rect64) -> Rect64 {
        if self.identity || out.is_empty() {
            *out
        } else {
            out.union(&self.omega(out))
        }
    }
}

fn weights(frac: f32) -> (f32, f32) {
    if frac > 0.0 {
        (frac, 1.0 - frac)
    } else {
        (1.0, 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(f: impl FnOnce(&mut ShadowParams)) -> Plan {
        let mut p = ShadowParams {
            opacity: 1.0,
            ..Default::default()
        };
        f(&mut p);
        Plan::new(&p, &Scale::FULL).unwrap()
    }

    #[test]
    fn angle_convention_light_from_direction() {
        let s = Scale::FULL;
        let o = |a: f64| {
            offset(
                &ShadowParams {
                    angle_deg: a,
                    distance: 4.0,
                    ..Default::default()
                },
                &s,
            )
        };
        assert_eq!(o(0.0), (-4.0, 0.0)); // light from right: exterior moves left
        assert_eq!(o(90.0), (0.0, 4.0)); // light from top: exterior moves down
        assert_eq!(o(180.0), (4.0, 0.0));
        assert_eq!(o(270.0), (0.0, -4.0));
        let (x, y) = o(120.0);
        assert!((x - 2.0).abs() < 1e-4 && (y - 4.0 * 0.75f64.sqrt()).abs() < 1e-4);
    }

    #[test]
    fn fractional_offset_split_is_exact() {
        let p = plan(|p| {
            p.angle_deg = 90.0;
            p.distance = 2.5;
        });
        assert_eq!((p.shift_x, p.frac_x, p.shift_y, p.frac_y), (0, 0.0, 2, 0.5));
        assert_eq!(p.weights_y(), (0.5, 0.5));
        assert_eq!(p.weights_x(), (1.0, 0.0));
        assert_eq!(p.base_y(), -3);
        let p = plan(|p| {
            p.angle_deg = 0.0;
            p.distance = 2.25;
        });
        assert_eq!((p.shift_x, p.frac_x), (-3, 0.75));
    }

    #[test]
    fn isotropic_spans_match_euclidean_disk() {
        for r in 1..=12 {
            let d = dilation(r as f64, &Scale::FULL).unwrap().unwrap();
            for sp in &d.spans {
                let expected = (0..=r)
                    .filter(|i| i * i + sp.dy * sp.dy <= r * r)
                    .max()
                    .unwrap();
                assert_eq!(sp.half_width, expected, "r={r} dy={}", sp.dy);
                assert_eq!(1u32 << sp.level <= (2 * sp.half_width + 1) as u32, true);
                assert!((2 * sp.half_width + 1) < (1 << (sp.level + 1)));
            }
            assert_eq!(d.spans.len(), (2 * r + 1) as usize);
            assert_eq!((d.reach_x, d.reach_y), (r as i64, r as i64));
        }
    }

    #[test]
    fn downsample_and_par_scale_every_length() {
        let p = ShadowParams {
            angle_deg: 0.0,
            distance: 8.0,
            size: 12.0,
            choke: 0.5,
            opacity: 1.0,
            ..Default::default()
        };
        let s = Scale::new(0.5, 0.5, 2.0).unwrap();
        assert_eq!(offset(&p, &s), (-2.0, 0.0));
        let plan = Plan::new(&p, &s).unwrap();
        let d = plan.dilate_lo.as_ref().unwrap(); // r=6 -> rx=1.5, ry=3
        assert_eq!(d.reach_y, 3);
        assert_eq!(d.reach_x, 1);
        // sigma = 6/3 = 2 -> sigma_x = 0.5, sigma_y = 1
        assert_eq!(plan.kernel_x.len(), 2 * 2 + 1);
        assert_eq!(plan.kernel_y.len(), 2 * 3 + 1);
        let s = Scale::new(1.0, 1.0, 0.5).unwrap();
        assert_eq!(offset(&p, &s), (-16.0, 0.0));
    }

    #[test]
    fn choke_fraction_interpolates_neighbouring_radii() {
        let p = plan(|p| {
            p.size = 10.0;
            p.choke = 0.25;
        });
        assert_eq!(p.dilate_lo.as_ref().unwrap().reach_x, 2);
        assert_eq!(p.dilate_hi.as_ref().unwrap().reach_x, 3);
        assert_eq!(p.dilate_mix, 0.5);
        let p = plan(|p| {
            p.size = 0.5;
            p.choke = 1.0;
        });
        assert!(p.dilate_lo.is_none());
        assert_eq!(p.dilate_hi.as_ref().unwrap().reach_x, 1);
        assert_eq!(p.dilate_mix, 0.5);
        assert_eq!(p.kernel_x, vec![1.0]);
    }

    #[test]
    fn kernel_is_normalised_symmetric_and_bounded() {
        for sigma in [0.0, 1e-5, 0.2, 1.0, 3.3, 83.0] {
            let k = kernel(sigma).unwrap();
            let sum: f64 = k.iter().map(|&w| w as f64).sum();
            assert!((sum - 1.0).abs() < 1e-5, "sigma={sigma} sum={sum}");
            let n = k.len();
            assert_eq!(n % 2, 1);
            for i in 0..n {
                assert_eq!(k[i], k[n - 1 - i]);
            }
            if sigma > 1e-4 {
                assert_eq!(n / 2, (3.0 * sigma - 1e-9).ceil().max(1.0) as usize);
            }
        }
        assert!(kernel(1e9).is_err());
    }

    #[test]
    fn omega_covers_shift_pad_and_bilinear_tap() {
        let p = plan(|p| {
            p.angle_deg = 90.0;
            p.distance = 2.5;
            p.size = 3.0;
        });
        let out = Rect64::new(10, 20, 30, 40);
        let o = p.omega(&out);
        // sigma=1 -> R=3; y samples x0 in [20-3, 39-3] and +1 tap.
        assert_eq!(o, Rect64::new(10 - 3, 20 - 3 - 3, 30 + 3, 40 - 2 + 3));
        assert_eq!(p.input_rect(&out), out.union(&o));
        assert_eq!(p.omega(&Rect64::default()), Rect64::default());
    }

    #[test]
    fn invalid_host_scales_are_rejected_or_structurally_defaulted() {
        assert!(Scale::new(0.0, 1.0, 1.0).is_err());
        assert!(Scale::new(1.0, 1.0, f64::NAN).is_err());
        assert!(Scale::new(1.0, 1.0, 100.0).is_err());
        assert_eq!(Scale::ratio(1, 0), 1.0);
        assert_eq!(Scale::ratio(-3, 2), 1.0);
        assert_eq!(Scale::ratio(10, 11), 10.0 / 11.0);
    }

    #[test]
    fn identity_only_when_opacity_is_zero() {
        assert!(plan(|p| p.opacity = 0.0).identity);
        assert!(
            !plan(|p| {
                p.distance = 0.0;
                p.size = 0.0;
            })
            .identity
        );
        let p = sanitize(&ShadowParams {
            opacity: f64::NAN,
            distance: 1e9,
            size: -3.0,
            choke: 7.0,
            ..Default::default()
        });
        assert_eq!(
            (p.opacity, p.distance, p.size, p.choke),
            (0.0, MAX_DISTANCE, 0.0, 1.0)
        );
    }
}
