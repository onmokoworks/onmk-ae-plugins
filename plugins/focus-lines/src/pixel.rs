//! Coverage -> straight (unassociated) ARGB, shared by the CPU and GPU backends.
//! AE boundary: straight RGB. 8 bit white = 255, 16 bit white = 32768,
//! 32 bit float RGB is not clamped (HDR / negative values survive).

/// Normalized straight pixel `[a, r, g, b]`.
pub type Argb = [f32; 4];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shading {
    /// Line colour times intensity (may exceed 1 for 32 bpc HDR).
    pub rgb: [f32; 3],
    /// 0..1.
    pub opacity: f32,
}

impl Shading {
    pub fn new(color: [f32; 3], intensity: f32, opacity_percent: f32) -> Shading {
        let k = if intensity.is_finite() {
            intensity.max(0.0)
        } else {
            0.0
        };
        Shading {
            rgb: color.map(|c| if c.is_finite() { c * k } else { 0.0 }),
            opacity: if opacity_percent.is_finite() {
                (opacity_percent / 100.0).clamp(0.0, 1.0)
            } else {
                0.0
            },
        }
    }
}

pub fn line_alpha(coverage: f32, shading: &Shading) -> f32 {
    let c = if coverage.is_finite() {
        coverage.clamp(0.0, 1.0)
    } else {
        0.0
    };
    (c * shading.opacity).clamp(0.0, 1.0)
}

/// Lines Only: constant straight RGB everywhere (also at alpha 0).
pub fn lines_only(coverage: f32, shading: &Shading) -> Argb {
    let [r, g, b] = shading.rgb;
    [line_alpha(coverage, shading), r, g, b]
}

/// Over Source with straight input. `None` means: line alpha is zero, copy the
/// source pixel bit-exactly (keeps HDR, negatives and colour under alpha 0).
pub fn over_source(coverage: f32, shading: &Shading, src: Argb) -> Option<Argb> {
    let a = line_alpha(coverage, shading);
    if a <= 0.0 {
        return None;
    }
    let sa = if src[0].is_finite() {
        src[0].clamp(0.0, 1.0)
    } else {
        0.0
    };
    let ao = a + sa * (1.0 - a);
    let mut out = [ao, 0.0, 0.0, 0.0];
    for c in 0..3 {
        let s = if src[c + 1].is_finite() {
            src[c + 1]
        } else {
            0.0
        };
        let premult = shading.rgb[c] * a + s * sa * (1.0 - a);
        out[c + 1] = premult / ao;
    }
    Some(out)
}

fn unit_clamp(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

pub fn encode8(p: Argb) -> [u8; 4] {
    p.map(|v| (unit_clamp(v) * 255.0).round() as u8)
}

pub fn encode16(p: Argb) -> [u16; 4] {
    p.map(|v| (unit_clamp(v) * 32768.0).round() as u16)
}

/// Float: alpha clamped to [0, 1], RGB unclamped (non-finite -> 0).
pub fn encode32(p: Argb) -> [f32; 4] {
    let a = if p[0].is_finite() {
        p[0].clamp(0.0, 1.0)
    } else {
        0.0
    };
    let f = |v: f32| if v.is_finite() { v } else { 0.0 };
    [a, f(p[1]), f(p[2]), f(p[3])]
}

pub fn decode8(p: [u8; 4]) -> Argb {
    p.map(|v| f32::from(v) / 255.0)
}

pub fn decode16(p: [u16; 4]) -> Argb {
    p.map(|v| f32::from(v.min(32768)) / 32768.0)
}

/// Acceptance check for a constant-colour generator: every pixel must carry the
/// straight colour regardless of alpha. Requires at least one partially
/// transparent pixel so the check cannot pass vacuously.
pub fn check_straight_constant(
    pixels: &[Argb],
    rgb: [f32; 3],
    tolerance: f32,
) -> Result<(), String> {
    let partial = pixels.iter().filter(|p| p[0] > 0.0 && p[0] < 1.0).count();
    if partial == 0 {
        return Err("no partially transparent pixels: check would be vacuous".into());
    }
    for (i, p) in pixels.iter().enumerate() {
        for c in 0..3 {
            let e = (p[c + 1] - rgb[c]).abs();
            if !matches!(
                e.partial_cmp(&tolerance),
                Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
            ) {
                return Err(format!(
                    "pixel {i}: channel {c} = {} expected {} (alpha {})",
                    p[c + 1],
                    rgb[c],
                    p[0]
                ));
            }
        }
    }
    Ok(())
}

/// Straight "over" onto an opaque background, used by previews and tests.
pub fn composite_on(p: Argb, bg: [f32; 3]) -> [f32; 3] {
    [0, 1, 2].map(|c| p[c + 1] * p[0] + bg[c] * (1.0 - p[0]))
}
