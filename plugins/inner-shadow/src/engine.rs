//! Host-independent rendering entry point: domain (Ω) construction, backend
//! selection policy and the common straight-alpha finalisation rules.

use crate::spec::{GpuMode, Plan, Rect64, MAX_PLANE_PIXELS};

/// Straight RGBA, AE-normalised (8-bit /255, 16-bit /32768, float as-is),
/// positioned at (left, top) in downsampled layer coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    pub width: usize,
    pub height: usize,
    pub left: i64,
    pub top: i64,
    pub pixels: Vec<[f32; 4]>,
}

impl Image {
    pub fn new(
        width: usize,
        height: usize,
        left: i64,
        top: i64,
        pixels: Vec<[f32; 4]>,
    ) -> Result<Self, RenderError> {
        if width.checked_mul(height) != Some(pixels.len()) {
            return Err(RenderError::Invalid(
                "image size does not match pixel count",
            ));
        }
        Ok(Self {
            width,
            height,
            left,
            top,
            pixels,
        })
    }

    pub fn rect(&self) -> Rect64 {
        Rect64::new(
            self.left,
            self.top,
            self.left + self.width as i64,
            self.top + self.height as i64,
        )
    }

    /// Pixel at layer coordinates, None outside the world.
    #[inline]
    pub fn get(&self, x: i64, y: i64) -> Option<[f32; 4]> {
        let (lx, ly) = (x - self.left, y - self.top);
        if lx < 0 || ly < 0 || lx >= self.width as i64 || ly >= self.height as i64 {
            None
        } else {
            Some(self.pixels[ly as usize * self.width + lx as usize])
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuRun {
    pub adapter: String,
    pub backend: String,
    pub device_type: String,
    pub driver: String,
    pub dispatches: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Route {
    /// Opacity 0 or empty output: copy only, no mask, no GPU initialisation.
    Identity,
    Cpu,
    Gpu(GpuRun),
    /// Auto mode: GPU unavailable/unsupported/failed, CPU used. Reason kept.
    CpuFallback(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RenderError {
    Invalid(&'static str),
    OutOfMemory,
    /// GPU mode was forced and the GPU path failed; never silently replaced.
    GpuRequired(String),
}

/// Prepared mask domain shared by both backends.
pub struct Prepared {
    pub omega: Rect64,
    pub omega_w: usize,
    pub omega_h: usize,
    /// E = 1 - clamp(alpha) on Ω, exterior 1.
    pub e: Vec<f32>,
    pub out_w: usize,
    pub out_h: usize,
    /// Input pixels aligned to the output rectangle; transparent zero outside.
    pub src: Vec<[f32; 4]>,
    /// Output pixel (x, y) samples Ω at (x + base.0, y + base.1).
    pub base: (i64, i64),
}

#[inline]
pub fn coverage(alpha: f32) -> f32 {
    if alpha.is_nan() {
        0.0
    } else {
        alpha.clamp(0.0, 1.0)
    }
}

fn dims(r: &Rect64) -> Result<(usize, usize), RenderError> {
    let w = usize::try_from(r.width()).map_err(|_| RenderError::Invalid("rect width"))?;
    let h = usize::try_from(r.height()).map_err(|_| RenderError::Invalid("rect height"))?;
    Ok((w, h))
}

fn copy_region(input: &Image, out: &Rect64) -> Result<Vec<[f32; 4]>, RenderError> {
    let (w, h) = dims(out)?;
    let n = w.checked_mul(h).ok_or(RenderError::OutOfMemory)?;
    let mut v = Vec::new();
    v.try_reserve_exact(n)
        .map_err(|_| RenderError::OutOfMemory)?;
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            v.push(input.get(out.left + x, out.top + y).unwrap_or([0.0; 4]));
        }
    }
    Ok(v)
}

pub fn prepare(input: &Image, out: &Rect64, plan: &Plan) -> Result<Prepared, RenderError> {
    let omega = plan.omega(out);
    let (omega_w, omega_h) = dims(&omega)?;
    let n = omega_w
        .checked_mul(omega_h)
        .ok_or(RenderError::OutOfMemory)?;
    if n > MAX_PLANE_PIXELS {
        return Err(RenderError::OutOfMemory);
    }
    let mut e = Vec::new();
    e.try_reserve_exact(n)
        .map_err(|_| RenderError::OutOfMemory)?;
    for y in 0..omega_h as i64 {
        for x in 0..omega_w as i64 {
            let a = input
                .get(omega.left + x, omega.top + y)
                .map(|p| coverage(p[3]))
                .unwrap_or(0.0);
            e.push(1.0 - a);
        }
    }
    let src = copy_region(input, out)?;
    let (out_w, out_h) = dims(out)?;
    Ok(Prepared {
        omega,
        omega_w,
        omega_h,
        e,
        out_w,
        out_h,
        src,
        base: (
            out.left + plan.base_x() - omega.left,
            out.top + plan.base_y() - omega.top,
        ),
    })
}

/// Common AE-boundary rules applied to either backend's (r', g', b', t).
#[inline]
pub fn finalize(src: [f32; 4], shaded: [f32; 4]) -> [f32; 4] {
    if !(src[3] > 0.0) || shaded[3] == 0.0 {
        return src;
    }
    let mut out = src;
    for c in 0..3 {
        if src[c].is_finite() {
            out[c] = shaded[c];
        }
    }
    out
}

pub fn shade_cpu(prep: &Prepared, plan: &Plan) -> Result<Vec<[f32; 4]>, RenderError> {
    crate::cpu::shade(
        &prep.e,
        prep.omega_w,
        prep.omega_h,
        &prep.src,
        prep.out_w,
        prep.out_h,
        prep.base,
        plan,
    )
    .ok_or(RenderError::OutOfMemory)
}

/// Renders `out` (layer coordinates) with the production GPU backend.
pub fn render(
    input: &Image,
    out: &Rect64,
    plan: &Plan,
    mode: GpuMode,
) -> Result<(Vec<[f32; 4]>, Route), RenderError> {
    render_with(input, out, plan, mode, crate::gpu::shade)
}

/// Same as [`render`] with an injectable GPU backend (policy tests).
pub fn render_with<G>(
    input: &Image,
    out: &Rect64,
    plan: &Plan,
    mode: GpuMode,
    gpu: G,
) -> Result<(Vec<[f32; 4]>, Route), RenderError>
where
    G: FnOnce(&Prepared, &Plan) -> Result<(Vec<[f32; 4]>, GpuRun), String>,
{
    if plan.identity || out.is_empty() {
        return Ok((copy_region(input, out)?, Route::Identity));
    }
    let prep = prepare(input, out, plan)?;
    let checked = |r: Result<(Vec<[f32; 4]>, GpuRun), String>| match r {
        Ok((v, run)) if v.len() == prep.src.len() => Ok((v, run)),
        Ok((v, _)) => Err(format!(
            "GPU returned {} pixels, expected {}",
            v.len(),
            prep.src.len()
        )),
        Err(e) => Err(e),
    };
    let (shaded, route) = match mode {
        GpuMode::Cpu => (shade_cpu(&prep, plan)?, Route::Cpu),
        GpuMode::Gpu => {
            let (v, run) = checked(gpu(&prep, plan)).map_err(RenderError::GpuRequired)?;
            (v, Route::Gpu(run))
        }
        GpuMode::Auto => match checked(gpu(&prep, plan)) {
            Ok((v, run)) => (v, Route::Gpu(run)),
            Err(reason) => (shade_cpu(&prep, plan)?, Route::CpuFallback(reason)),
        },
    };
    Ok((
        prep.src
            .iter()
            .zip(&shaded)
            .map(|(s, d)| finalize(*s, *d))
            .collect(),
        route,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{Scale, ShadowParams};
    use std::cell::Cell;

    fn fixture() -> (Image, Plan) {
        let px = (0..36)
            .map(|i| {
                if (7..29).contains(&i) {
                    [0.8, 0.6, 0.4, 1.0]
                } else {
                    [0.3, 0.2, 0.1, 0.0]
                }
            })
            .collect();
        let img = Image::new(6, 6, -2, 3, px).unwrap();
        let plan = Plan::new(
            &ShadowParams {
                opacity: 1.0,
                ..Default::default()
            },
            &Scale::FULL,
        )
        .unwrap();
        (img, plan)
    }

    #[test]
    fn finalize_keeps_alpha_hidden_rgb_and_nonfinite_channels() {
        let shaded = [9.0, 9.0, 9.0, 0.5];
        assert_eq!(finalize([0.1, 0.2, 0.3, 0.0], shaded), [0.1, 0.2, 0.3, 0.0]);
        let nan = f32::from_bits(0x7fc0_1234);
        let out = finalize([0.1, 0.2, 0.3, nan], shaded);
        assert_eq!(out[3].to_bits(), nan.to_bits());
        assert_eq!(&out[..3], &[0.1, 0.2, 0.3]);
        assert_eq!(
            finalize([0.1, 0.2, 0.3, 0.5], [9.0, 9.0, 9.0, 0.0]),
            [0.1, 0.2, 0.3, 0.5]
        );
        let out = finalize([f32::INFINITY, 0.2, 0.3, 1.5], shaded);
        assert_eq!(out, [f32::INFINITY, 9.0, 9.0, 1.5]);
    }

    #[test]
    fn cpu_mode_and_identity_never_touch_gpu() {
        let (img, plan) = fixture();
        let out = img.rect();
        let calls = Cell::new(0);
        let fake = |_: &Prepared, _: &Plan| {
            calls.set(calls.get() + 1);
            Err::<(Vec<[f32; 4]>, GpuRun), String>("should not run".into())
        };
        let (_, route) = render_with(&img, &out, &plan, GpuMode::Cpu, fake).unwrap();
        assert_eq!(route, Route::Cpu);
        let mut identity = plan.clone();
        identity.identity = true;
        for mode in [GpuMode::Auto, GpuMode::Gpu, GpuMode::Cpu] {
            let (v, route) = render_with(&img, &out, &identity, mode, fake).unwrap();
            assert_eq!(route, Route::Identity);
            assert_eq!(v, img.pixels);
        }
        assert_eq!(calls.get(), 0);
    }

    #[test]
    fn auto_falls_back_and_forced_gpu_fails_explicitly() {
        let (img, plan) = fixture();
        let out = img.rect();
        let (cpu, _) = render_with(&img, &out, &plan, GpuMode::Cpu, |_: &Prepared, _: &Plan| {
            Err("unused".into())
        })
        .unwrap();
        let (auto, route) = render_with(
            &img,
            &out,
            &plan,
            GpuMode::Auto,
            |_: &Prepared, _: &Plan| Err("no adapter".into()),
        )
        .unwrap();
        assert_eq!(route, Route::CpuFallback("no adapter".into()));
        assert_eq!(auto, cpu);
        let forced = render_with(&img, &out, &plan, GpuMode::Gpu, |_: &Prepared, _: &Plan| {
            Err("no adapter".into())
        });
        assert_eq!(forced, Err(RenderError::GpuRequired("no adapter".into())));
        // A malformed GPU result is a GPU failure, not output.
        let short = |_: &Prepared, _: &Plan| {
            Ok((
                vec![[0.0f32; 4]; 3],
                GpuRun {
                    adapter: "fake".into(),
                    backend: String::new(),
                    device_type: String::new(),
                    driver: String::new(),
                    dispatches: 1,
                },
            ))
        };
        let (v, route) = render_with(&img, &out, &plan, GpuMode::Auto, short).unwrap();
        assert!(matches!(route, Route::CpuFallback(_)));
        assert_eq!(v, cpu);
        assert!(matches!(
            render_with(&img, &out, &plan, GpuMode::Gpu, short),
            Err(RenderError::GpuRequired(_))
        ));
    }

    #[test]
    fn oversized_domain_is_rejected_before_allocation() {
        let (img, plan) = fixture();
        let huge = Rect64::new(0, 0, 1 << 14, 1 << 13);
        assert_eq!(
            render_with(
                &img,
                &huge,
                &plan,
                GpuMode::Cpu,
                |_: &Prepared, _: &Plan| Err("x".into())
            )
            .err(),
            Some(RenderError::OutOfMemory)
        );
        assert!(Image::new(2, 2, 0, 0, vec![[0.0; 4]; 3]).is_err());
    }
}
