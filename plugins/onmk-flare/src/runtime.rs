//! One runtime per loaded plugin, shared by all AE effect instances. No helper
//! threads or timers: idle eviction and retry are checked on the next render.
use crate::{gpu::GpuProcessor, EffectParams};
use std::time::{Duration, Instant};

#[derive(Default)]
pub(crate) struct Runtime {
    gpu: Option<GpuProcessor>,
    retry_at: Option<Instant>,
    last_frame: Option<Instant>,
}
impl Runtime {
    pub fn render(&mut self, ep: &EffectParams, src: &[u8], w: usize, h: usize) -> Vec<u8> {
        self.render_pixels(ep,src,w,h)
    }
    pub(crate) fn render_pixels<T: crate::pixel::Channel>(&mut self, ep: &EffectParams, src: &[T], w: usize, h: usize) -> Vec<T> {
        if w == 0 || h == 0 || w.checked_mul(h).and_then(|n| n.checked_mul(4)) != Some(src.len()) {
            return src.to_vec();
        }
        let now = Instant::now();
        if self
            .last_frame
            .is_some_and(|t| now.duration_since(t) >= Duration::from_secs(30))
        {
            if let Some(gpu) = &self.gpu {
                gpu.release_caches();
            }
        }
        if self.gpu.is_none() && self.retry_at.is_none_or(|t| now >= t) {
            self.gpu = std::panic::catch_unwind(GpuProcessor::new).ok();
            if self.gpu.is_none() {
                self.retry_at = Some(now + Duration::from_secs(30));
            }
        }
        let result = if let Some(gpu) = &self.gpu {
            gpu.process_pixels(ep, src, w, h)
        } else {
            crate::flare::process_pixels(ep, src, w, h)
        };
        if self.gpu.as_ref().is_some_and(|g| !g.is_healthy()) {
            self.gpu.take();
            self.retry_at = Some(Instant::now() + Duration::from_secs(30));
        }
        self.last_frame = Some(Instant::now());
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn float_cpu_cooldown_fallback_preserves_sub_byte_values() {
        let mut runtime=Runtime { retry_at:Some(Instant::now()+Duration::from_secs(30)), ..Default::default() };
        let ep=EffectParams { style_preset:6, source_opacity:1.0,transfer_mode:4,global_scale:1.0, ..Default::default() };
        let src=[1.0_f32,0.5000305,0.50006104,0.000030517578];
        assert_eq!(runtime.render_pixels(&ep,&src,1,1),src);
        assert!(runtime.gpu.is_none());
    }
    #[test]
    #[ignore = "real GPU destruction, CPU cooldown, reinitialization and idle cleanup"]
    fn runtime_device_recovery() {
        let mut runtime = Runtime::default();
        let ep = EffectParams {
            global_scale: 1.0,
            global_brightness: 1.0,
            flare_opacity: 1.0,
            source_opacity: 1.0,
            source_mode: 1,
            light_x: 30.0,
            light_y: 20.0,
            ..Default::default()
        };
        let src = vec![128; 97 * 61 * 4];
        let original = runtime.render(&ep, &src, 97, 61);
        runtime.gpu.as_ref().unwrap().destroy_device_for_test();
        assert_eq!(
            runtime.render(&ep, &src, 97, 61),
            crate::flare::process(&ep, &src, 97, 61)
        );
        assert!(runtime.gpu.is_none());
        runtime.render(&ep, &src, 97, 61);
        assert!(
            runtime.gpu.is_none(),
            "do not reinitialize every frame during cooldown"
        );
        runtime.retry_at = Some(Instant::now());
        assert_eq!(original, runtime.render(&ep, &src, 97, 61));
        assert!(runtime.gpu.as_ref().unwrap().is_healthy());
        runtime.last_frame = Some(Instant::now() - Duration::from_secs(31));
        assert_eq!(original, runtime.render(&ep, &src, 97, 61));
    }
}
