//! Backend selection. One render uses exactly one backend for all its pixels.

use crate::gpu::{self, GpuTiming};
use crate::raster::{self, PixelRect, Scene};
use std::sync::Mutex;
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    /// Private GPU device; falls back to CPU on any GPU error (reason recorded).
    Auto,
    /// GPU only; errors are returned, never silently replaced by CPU output.
    Gpu,
    /// CPU only; the GPU is never initialized.
    Cpu,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Used {
    Cpu,
    Gpu,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Timing {
    pub bin_ms: f64,
    pub raster_ms: f64,
    pub gpu: Option<GpuTiming>,
}

#[derive(Clone, Debug)]
pub struct Coverage {
    pub values: Vec<f32>,
    pub used: Used,
    pub timing: Timing,
    /// Set when Auto fell back to CPU.
    pub fallback_reason: Option<String>,
}

static LAST_FALLBACK: Mutex<Option<String>> = Mutex::new(None);

/// Most recent Auto -> CPU fallback reason (for the About dialog / smoke tool).
pub fn last_fallback() -> Option<String> {
    LAST_FALLBACK.lock().ok().and_then(|g| g.clone())
}

pub fn coverage(
    scene: &Scene,
    rect: PixelRect,
    samples: u32,
    backend: Backend,
) -> Result<Coverage, String> {
    let t = Instant::now();
    let bins = raster::bin(scene, rect).map_err(|e| e.to_string())?;
    let mut timing = Timing {
        bin_ms: t.elapsed().as_secs_f64() * 1e3,
        ..Timing::default()
    };
    let cpu = |timing: &mut Timing| {
        let t = Instant::now();
        let values = raster::cpu_coverage(scene, rect, samples, &bins);
        timing.raster_ms = t.elapsed().as_secs_f64() * 1e3;
        values
    };
    match backend {
        Backend::Cpu => Ok(Coverage {
            values: cpu(&mut timing),
            used: Used::Cpu,
            timing,
            fallback_reason: None,
        }),
        Backend::Gpu => {
            let (values, g) = gpu::coverage(scene, rect, samples, &bins)?;
            timing.raster_ms = g.total_ms;
            timing.gpu = Some(g);
            Ok(Coverage {
                values,
                used: Used::Gpu,
                timing,
                fallback_reason: None,
            })
        }
        Backend::Auto => match gpu::coverage(scene, rect, samples, &bins) {
            Ok((values, g)) => {
                timing.raster_ms = g.total_ms;
                timing.gpu = Some(g);
                Ok(Coverage {
                    values,
                    used: Used::Gpu,
                    timing,
                    fallback_reason: None,
                })
            }
            Err(reason) => {
                if let Ok(mut last) = LAST_FALLBACK.lock() {
                    *last = Some(reason.clone());
                }
                Ok(Coverage {
                    values: cpu(&mut timing),
                    used: Used::Cpu,
                    timing,
                    fallback_reason: Some(reason),
                })
            }
        },
    }
}
