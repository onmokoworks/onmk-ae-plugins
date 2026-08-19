//! Host-independent reference implementation for onmk Film.

mod blur;
mod color;
mod grain;
mod params;
mod pipeline;

pub use blur::gaussian_blur_rgba as blur_cpu;
pub use grain::apply_grain as grain_cpu;
pub use params::{FilmParams, FilmStock, Gauge, InputColorspace, LookPreset};
pub use pipeline::{mae_rgb, render_cpu as render, render_with_backends, render_with_blur};
