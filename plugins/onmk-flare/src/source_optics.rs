//! High-dynamic-range source image: sensor core, aperture diffraction and
//! localized veiling glare.  Kept separate from lens ghosts so both layers
//! can be art-directed without destroying the physical ghost geometry.

use crate::{physical::BrightSource, EffectParams};
use std::f32::consts::PI;

pub fn render(ep: &EffectParams, sources: &[BrightSource], w: usize, h: usize) -> Vec<[f32; 3]> {
    let workers = std::thread::available_parallelism().map_or(1, |n| n.get().min(4));
    render_with_workers(ep, sources, w, h, workers)
}

fn render_with_workers(
    ep: &EffectParams,
    sources: &[BrightSource],
    w: usize,
    h: usize,
    workers: usize,
) -> Vec<[f32; 3]> {
    render_impl::<true>(ep, sources, w, h, workers)
}

fn render_impl<const REUSE: bool>(
    ep: &EffectParams,
    sources: &[BrightSource],
    w: usize,
    h: usize,
    workers: usize,
) -> Vec<[f32; 3]> {
    let mut out = vec![[0.0; 3]; w * h];
    if sources.is_empty() || w == 0 || h == 0 {
        return out;
    }
    let mut selected = sources.to_vec();
    selected.sort_by(|a, b| energy(b).total_cmp(&energy(a)));
    selected.truncate(8);
    let fov_h = 50_f64.to_radians();
    let fov_v = 2.0 * ((h as f64 / w as f64) * (fov_h * 0.5).tan()).atan();
    let diagonal = ((w * w + h * h) as f32).sqrt();
    let diffraction = ep.unified_diffraction.clamp(0.0, 1.0) as f32;
    let scale = ep.global_scale.max(0.1) as f32;
    let blades = ep.starburst_blades.clamp(3, 16) as usize;
    let rotation = (ep.flare_angle as f32).to_radians();

    for (source_index, source) in selected.iter().enumerate() {
        let sx = ((0.5 + source.angle_x / fov_h) * w as f64) as f32;
        let sy = ((0.5 + source.angle_y / fov_v) * h as f64) as f32;
        let source_energy = energy(source).max(0.02);
        let core_gain = (2.8 + ep.hotspot_intensity as f32 * 2.4) * source_energy;
        let spike_length = diagonal * (0.10 + 0.25 * diffraction) * scale.sqrt();
        let haze_radius = diagonal * (0.075 + 0.11 * ep.bloom_radius.max(0.005));
        let to_centre_x = w as f32 * 0.5 - sx;
        let to_centre_y = h as f32 * 0.5 - sy;
        let axis_len = (to_centre_x * to_centre_x + to_centre_y * to_centre_y)
            .sqrt()
            .max(1.0);
        let ax = to_centre_x / axis_len;
        let ay = to_centre_y / axis_len;
        let glass_x: Vec<f32> = if REUSE {
            (0..w)
                .map(|x| {
                    let dx = x as f32 + 0.5 - sx;
                    0.10 * (dx * 0.013 + 1.7).sin()
                })
                .collect()
        } else {
            Vec::new()
        };

        // Independent rows can run concurrently without changing the order
        // in which sources accumulate into any pixel. Keep the worker count
        // bounded when AE renders multiple frames at once.
        let rows_per_job = h.div_ceil(workers).max(1);
        std::thread::scope(|scope| {
            for (job, rows) in out.chunks_mut(rows_per_job * w).enumerate() {
                let glass_x = &glass_x;
                scope.spawn(move || {
                    for (local_y, row) in rows.chunks_mut(w).enumerate() {
                        let y = job * rows_per_job + local_y;
                        let row_glass = if REUSE {
                            0.08 * ((y as f32 + 0.5 - sy) * 0.017 - 0.9).cos()
                        } else {
                            0.0
                        };
                        for x in 0..w {
                            let dx = x as f32 + 0.5 - sx;
                            let dy = y as f32 + 0.5 - sy;
                            let r = (dx * dx + dy * dy).sqrt().max(0.001);
                            let theta = dy.atan2(dx) - rotation;

                            // Four exposure scales create a clipped photographic core
                            // without turning the entire glow into a single Gaussian disc.
                            let pin = gaussian::<REUSE>(r, 1.15 * scale) * 10.0;
                            let white_core = gaussian::<REUSE>(r, 3.8 * scale) * 2.7;
                            let near = gaussian::<REUSE>(r, 13.0 * scale) * 0.34;
                            let shoulder = 0.10 / (1.0 + (r / (42.0 * scale)).powf(2.35));

                            // Polygon-aperture diffraction approximation.  The thin main
                            // needles, broad skirts and faint interleaved needles have
                            // separate falloffs so the result does not read as drawn lines.
                            let sector = PI / blades as f32;
                            let local = (theta + sector * 0.5).rem_euclid(sector) - sector * 0.5;
                            let perp = r * local.sin().abs();
                            let widening = 0.58 + 0.0065 * r;
                            let radial_falloff = if REUSE {
                                (-r / spike_length.max(1.0)).exp()
                            } else {
                                0.0
                            };
                            let needle = (-0.5 * (perp / widening).powi(2)).exp()
                                * if REUSE {
                                    radial_falloff
                                } else {
                                    (-r / spike_length.max(1.0)).exp()
                                };
                            let skirt = (-0.5 * (perp / (2.3 + 0.012 * r)).powi(2)).exp()
                                * (-r / (spike_length * 0.52).max(1.0)).exp();
                            let secondary_angle = theta + sector * 0.5;
                            let secondary_local =
                                (secondary_angle + sector * 0.5).rem_euclid(sector) - sector * 0.5;
                            let secondary_perp = r * secondary_local.sin().abs();
                            let secondary = (-0.5 * (secondary_perp / (0.9 + 0.009 * r)).powi(2))
                                .exp()
                                * (-r / (spike_length * 0.58).max(1.0)).exp();
                            let aperture_ripple = 0.82
                                + 0.18
                                    * (r * (0.105 + source_index as f32 * 0.007)
                                        + theta * blades as f32 * 0.35)
                                        .cos()
                                        .powi(2);
                            let diffraction_value = diffraction
                                * (0.105 * needle + 0.030 * skirt + 0.014 * secondary)
                                * aperture_ripple;

                            // Weak Airy-like rings bind the white core to the spikes.
                            let ring_phase = r / (3.2 * scale).max(0.5) * PI;
                            let airy = diffraction
                                * 0.025
                                * (ring_phase.sin() / ring_phase.max(1.0)).powi(2)
                                * (-r / (48.0 * scale)).exp();

                            // Local veiling glare: elliptical along the optical axis and
                            // modulated at very low frequency.  It falls to black instead
                            // of applying a full-frame colour wash.
                            let along = dx * ax + dy * ay;
                            let across = -dx * ay + dy * ax;
                            let shifted = along - haze_radius * 0.20;
                            let haze_q = (shifted / (haze_radius * 1.35)).powi(2)
                                + (across / (haze_radius * 0.78)).powi(2);
                            let glass_variation = if REUSE {
                                0.82 + glass_x[x] + row_glass
                            } else {
                                0.82 + 0.10 * (dx * 0.013 + 1.7).sin()
                                    + 0.08 * (dy * 0.017 - 0.9).cos()
                            };
                            let haze = (0.016 + ep.bloom_strength * 0.007)
                                * source_energy
                                * (-1.7 * haze_q).exp()
                                * glass_variation.max(0.45);

                            for c in 0..3 {
                                let wavelength_scale = [1.07_f32, 1.0, 0.94][c];
                                let spectral_numerator = if REUSE && c == 1 {
                                    radial_falloff
                                } else {
                                    (-r / (spike_length * wavelength_scale).max(1.0)).exp()
                                };
                                let spectral_denominator = if REUSE {
                                    radial_falloff
                                } else {
                                    (-r / spike_length.max(1.0)).exp()
                                };
                                let spectral_falloff =
                                    spectral_numerator / spectral_denominator.max(1e-5);
                                let core_colour = [1.0_f32, 0.91, 0.72][c];
                                let haze_colour = [1.0_f32, 0.62, 0.34][c];
                                let source_colour = source.rgb[c].max(0.05);
                                row[x][c] += core_gain
                                    * source_colour
                                    * ((pin + white_core)
                                        + near * core_colour
                                        + shoulder * core_colour)
                                    + diffraction_value * spectral_falloff * source_colour
                                    + airy * [0.72, 0.88, 1.0][c]
                                    + haze * haze_colour;
                            }
                        }
                    }
                });
            }
        });
    }
    out
}

fn gaussian<const REUSE: bool>(r: f32, sigma: f32) -> f32 {
    let exponent = -0.5 * (r / sigma.max(0.25)).powi(2);
    // exp(-104) is below half the smallest f32 subnormal (2^-150).
    // No nonzero f32 tail is discarded. NaN continues through the original exp.
    if REUSE && exponent <= -104.0 {
        0.0
    } else {
        exponent.exp()
    }
}

fn energy(s: &BrightSource) -> f32 {
    0.2126 * s.rgb[0] + 0.7152 * s.rgb[1] + 0.0722 * s.rgb[2]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gaussian_zero_shortcut_matches_exp() {
        for exponent in [
            -104.0_f32,
            f32::from_bits((-104.0_f32).to_bits() + 1),
            -104.1,
            -105.0,
            -200.0,
            -1000.0,
            f32::NEG_INFINITY,
        ] {
            assert_eq!(exponent.exp().to_bits(), 0.0_f32.to_bits());
        }
        for scale in [0.1, 0.25, 1.0, 8.0, 100.0] {
            for i in 0..20000 {
                let r = i as f32 * 0.137;
                assert_eq!(
                    gaussian::<false>(r, scale).to_bits(),
                    gaussian::<true>(r, scale).to_bits()
                );
            }
        }
    }

    #[test]
    #[ignore = "1080p exact old/new source optics timing"]
    fn benchmark_source_math_reuse() {
        let ep = EffectParams {
            unified_diffraction: 0.18,
            global_scale: 1.0,
            starburst_blades: 6,
            hotspot_intensity: 1.6,
            bloom_radius: 0.018,
            bloom_strength: 0.6,
            ..Default::default()
        };
        let sources = [BrightSource {
            angle_x: -0.12,
            angle_y: 0.03,
            rgb: [1.0, 0.8, 0.6],
        }];
        for run in 0..3 {
            let start = std::time::Instant::now();
            let original = render_impl::<false>(&ep, &sources, 1920, 1080, 4);
            let old = start.elapsed();
            let start = std::time::Instant::now();
            let optimized = render_impl::<true>(&ep, &sources, 1920, 1080, 4);
            let new = start.elapsed();
            assert!(original
                .iter()
                .flatten()
                .zip(optimized.iter().flatten())
                .all(|(a, b)| a.to_bits() == b.to_bits()));
            eprintln!("source optics {run}: old={old:?}, reused={new:?}");
        }
    }

    #[test]
    fn parallel_source_optics_is_bit_exact() {
        let sources = [
            BrightSource {
                angle_x: -0.16,
                angle_y: 0.05,
                rgb: [1.0, 0.7, 0.3],
            },
            BrightSource {
                angle_x: 0.23,
                angle_y: -0.12,
                rgb: [0.2, 0.8, 1.0],
            },
        ];
        let ep = EffectParams {
            unified_diffraction: 0.73,
            global_scale: 1.4,
            starburst_blades: 7,
            flare_angle: 23.0,
            hotspot_intensity: 1.6,
            bloom_radius: 0.018,
            bloom_strength: 0.6,
            ..Default::default()
        };
        for (w, h) in [(0, 0), (1, 1), (31, 17), (97, 61)] {
            let serial = render_with_workers(&ep, &sources, w, h, 1);
            let original = render_impl::<false>(&ep, &sources, w, h, 1);
            assert!(serial
                .iter()
                .flatten()
                .zip(original.iter().flatten())
                .all(|(a, b)| a.to_bits() == b.to_bits()));
            let parallel = render_with_workers(&ep, &sources, w, h, 4);
            assert!(serial
                .iter()
                .flatten()
                .zip(parallel.iter().flatten())
                .all(|(a, b)| a.to_bits() == b.to_bits()));
            assert_eq!(serial.len(), parallel.len());
        }
    }
}
