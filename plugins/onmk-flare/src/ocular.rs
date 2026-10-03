//! Perceptual human-eye glare approximation.
//!
//! This deliberately complements (rather than replaces) the camera-lens tracer:
//! small-angle asymmetric corona/starburst, a lenticular spectral halo and a
//! wide power-law veiling glare are evaluated as separate components.

use crate::{physical::BrightSource, EffectParams};

const PI: f32 = std::f32::consts::PI;

#[derive(Clone, Copy)]
pub struct Config {
    pub enabled: bool,
    pub pupil_mm: f32,
    pub age: f32,
    pub seed: i32,
    pub corona: f32,
    pub corona_radius: f32,
    pub halo: f32,
    pub halo_radius: f32,
    pub veil: f32,
    pub veil_radius: f32,
    pub tear: f32,
    pub phase: f32,
    pub squint: f32,
    pub squint_length: f32,
    pub squint_curve: f32,
    pub chromatic: f32,
}

impl From<&EffectParams> for Config {
    fn from(ep: &EffectParams) -> Self {
        Self {
            enabled: ep.ocular_enabled,
            pupil_mm: ep.ocular_pupil_mm,
            age: ep.ocular_age,
            seed: ep.ocular_seed,
            corona: ep.ocular_corona,
            corona_radius: ep.ocular_corona_radius,
            halo: ep.ocular_halo,
            halo_radius: ep.ocular_halo_radius,
            veil: ep.ocular_veil,
            veil_radius: ep.ocular_veil_radius,
            tear: ep.ocular_tear,
            phase: ep.ocular_phase,
            squint: ep.ocular_squint,
            squint_length: ep.ocular_squint_length,
            squint_curve: ep.ocular_squint_curve,
            chromatic: ep.chromatic_amount as f32,
        }
    }
}

pub fn render(ep: &EffectParams, sources: &[BrightSource], w: usize, h: usize) -> Vec<[f32; 3]> {
    render_config(Config::from(ep), sources, w, h)
}

pub fn render_config(cfg: Config, sources: &[BrightSource], w: usize, h: usize) -> Vec<[f32; 3]> {
    let mut out = vec![[0.0; 3]; w * h];
    if !cfg.enabled || sources.is_empty() {
        return out;
    }

    // The strongest few sources carry structured glare. This avoids turning
    // textured highlights into thousands of overlapping starbursts.
    let mut selected: Vec<_> = sources.iter().collect();
    selected.sort_by(|a, b| {
        energy(b)
            .partial_cmp(&energy(a))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    selected.truncate(12);

    let min_dim = w.min(h) as f32;
    let pupil = cfg.pupil_mm.clamp(2.0, 8.0);
    let pupil_open = ((pupil - 3.0) / 5.0).clamp(0.0, 1.0);
    let age_scatter = 1.0 + (cfg.age / 70.0).powi(4);
    let corona_radius = min_dim * cfg.corona_radius * (0.45 + 0.85 * pupil_open);
    let halo_radius = min_dim * cfg.halo_radius;
    let veil_radius = min_dim * cfg.veil_radius;
    let seed = cfg.seed as u32;
    let phase = cfg.phase * PI / 180.0;

    for src in selected {
        let sx = ((0.5 + src.angle_x / 50_f64.to_radians()) * w as f64) as f32;
        let fov_v = 2.0 * ((h as f64 / w as f64) * (25_f64.to_radians()).tan()).atan();
        let sy = ((0.5 + src.angle_y / fov_v) * h as f64) as f32;
        let src_energy = energy(src).max(0.0);
        if src_energy <= 1e-6 {
            continue;
        }

        for y in 0..h {
            for x in 0..w {
                let dx = x as f32 + 0.5 - sx;
                let dy = y as f32 + 0.5 - sy;
                let r = (dx * dx + dy * dy).sqrt().max(0.35);
                let theta = dy.atan2(dx);

                // Wide-angle straylight. A softened inverse-square core plus a
                // weak inverse-radius tail approximates the CIE-domain falloff.
                let rn = r / veil_radius.max(1.0);
                let veil = cfg.veil
                    * age_scatter
                    * (0.78 / (1.0 + 18.0 * rn * rn) + 0.22 / (1.0 + 7.0 * rn));

                // Many irregular needles. Harmonics are seeded per virtual eye;
                // high powers turn their maxima into narrow, unequal spokes.
                let a = theta + hash01(seed ^ 0x68bc_21ebu32) * 2.0 * PI;
                let tear = cfg.tear
                    * (0.55 * (phase * 0.73 + a * 2.0).sin()
                        + 0.30 * (phase * 1.31 - a * 5.0).sin());
                let needles = harmonic_needles(a, seed, tear);
                let cr = r / corona_radius.max(1.0);
                let radial = (-1.65 * cr).exp() / (1.0 + 1.8 * cr);
                let corona = cfg.corona * pupil_open * needles * radial;

                // Squinting turns eyelashes and the wet lid margin into many
                // individual curved caustics. Evaluate those fibres in image
                // space rather than as angular spokes: a spoke is necessarily
                // straight, while the observed bundles bow and fan outward.
                let squint_len = min_dim * cfg.squint_length;
                // Dispersion is zero at the source and grows along each fibre,
                // keeping the core white while separating coloured outer rims.
                let colour_shift = cfg.chromatic * (1.8 + 8.5 * (r / squint_len.max(1.0)));
                let squint_r = cfg.squint
                    * eyelash_fibres(
                        dx,
                        dy,
                        squint_len,
                        cfg.squint_curve,
                        seed,
                        phase,
                        colour_shift,
                    );
                let squint_g = cfg.squint
                    * eyelash_fibres(dx, dy, squint_len, cfg.squint_curve, seed, phase, 0.0);
                let squint_b = cfg.squint
                    * eyelash_fibres(
                        dx,
                        dy,
                        squint_len,
                        cfg.squint_curve,
                        seed,
                        phase,
                        -colour_shift,
                    );

                // A lens-fibre halo. RGB radii differ slightly with wavelength.
                let halo_width = (halo_radius * 0.055).max(0.8);
                let hr = ring(r, halo_radius * 1.018, halo_width);
                let hg = ring(r, halo_radius, halo_width);
                let hb = ring(r, halo_radius * 0.975, halo_width * 1.08);
                let halo_gain = cfg.halo * (0.55 + 0.45 * age_scatter.min(2.0));

                let core = cfg.corona
                    * 0.18
                    * (-(r * r) / (2.0 * (corona_radius * 0.055).max(0.8).powi(2))).exp();
                let base = veil + corona + core;
                let i = y * w + x;
                out[i][0] += src.rgb[0] * src_energy * (base + squint_r + halo_gain * hr);
                out[i][1] += src.rgb[1] * src_energy * (base + squint_g + halo_gain * hg);
                out[i][2] += src.rgb[2] * src_energy * (base + squint_b + halo_gain * hb);
            }
        }
    }
    out
}

fn energy(s: &BrightSource) -> f32 {
    0.2126 * s.rgb[0] + 0.7152 * s.rgb[1] + 0.0722 * s.rgb[2]
}

fn ring(r: f32, radius: f32, width: f32) -> f32 {
    let d = (r - radius) / width;
    (-0.5 * d * d).exp()
}

fn harmonic_needles(a: f32, seed: u32, tear: f32) -> f32 {
    let p0 = hash01(seed ^ 0xa511_e9b3) * 2.0 * PI;
    let p1 = hash01(seed ^ 0x63d8_3595) * 2.0 * PI;
    let p2 = hash01(seed ^ 0x9e37_79b9) * 2.0 * PI;
    let fine = (0.5 + 0.5 * (a * 17.0 + p0 + tear).cos()).powf(18.0);
    let medium = (0.5 + 0.5 * (a * 11.0 + p1 - tear * 0.7).cos()).powf(13.0);
    let coarse = (0.5 + 0.5 * (a * 7.0 + p2 + tear * 1.3).cos()).powf(10.0);
    (0.52 * fine + 0.31 * medium + 0.17 * coarse).min(1.0)
}

fn eyelash_fibres(
    dx: f32,
    dy: f32,
    length: f32,
    curve_amount: f32,
    seed: u32,
    phase: f32,
    chroma_shift: f32,
) -> f32 {
    let axial = dx.abs();
    let u = axial / length.max(1.0);
    if u > 1.18 {
        return 0.0;
    }

    let side_seed = if dx < 0.0 { 0x3141_5926 } else { 0x2718_2818 };
    let mut sum = 0.0;
    // Two uneven fans. The slight bias toward downward fibres mirrors the lid
    // geometry in the supplied observation without forcing bilateral symmetry.
    for fibre in 0..26u32 {
        let h0 = hash01(seed ^ side_seed ^ fibre.wrapping_mul(0x9e37_79b9));
        let h1 = hash01(seed ^ side_seed ^ fibre.wrapping_mul(0x85eb_ca6b) ^ 0xa511_e9b3);
        let h2 = hash01(seed ^ side_seed ^ fibre.wrapping_mul(0xc2b2_ae35) ^ 0x63d8_3595);
        let fan_t = fibre as f32 / 25.0;
        let angle = -0.48 + fan_t * 0.83 + (h0 - 0.5) * 0.075;
        let slope = angle.tan();
        let bow_sign = if fibre % 3 == 0 { -1.0 } else { 1.0 };
        let bow = bow_sign * curve_amount * length * (0.34 + 0.82 * h1);
        let asym = if dx < 0.0 { 1.08 } else { 0.88 };
        let centre_y = axial * slope + bow * u * u * asym + chroma_shift * u * u;
        let width = 1.15 + 2.15 * h2 + 1.7 * u;
        let distance = (dy - centre_y).abs();
        let ridge = (-(distance * distance) / (2.0 * width * width)).exp();
        let fade = (1.0 - (u / 1.18).powf(1.35)).max(0.0) * (-0.34 * u).exp();
        let strand = 0.26 + 0.74 * h0;
        let shimmer = 0.76 + 0.24 * (phase + u * (13.0 + 9.0 * h1) + h2 * PI).cos();
        sum += ridge * fade * strand * shimmer;
    }

    // A thin wet lid margin supplies the bright near-horizontal central bundle.
    let lid_bow = curve_amount * length * 0.075 * u * u * if dx < 0.0 { -1.0 } else { 1.0 };
    let lid_distance = (dy - lid_bow).abs();
    let lid = (-(lid_distance * lid_distance) / (2.0 * (1.15 + u).powi(2))).exp()
        * (1.0 - (u / 1.12).powf(1.5)).max(0.0);
    (sum * 0.29 + lid * 0.52).min(2.2)
}

fn hash01(mut x: u32) -> f32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    (x as f64 / u32::MAX as f64) as f32
}

fn p2(seed: u32) -> f32 {
    hash01(seed ^ 0xd1b5_4a35) * 2.0 * PI
}

#[cfg(test)]
mod preview_test {
    use super::*;
    use crate::physical;
    use image::{ImageBuffer, Rgba};

    #[test]
    #[ignore = "writes the user-facing ocular comparison render"]
    fn write_ocular_preview() {
        let input_path = std::env::var("OCULAR_PREVIEW_INPUT").expect("OCULAR_PREVIEW_INPUT");
        let output_path = std::env::var("OCULAR_PREVIEW_OUTPUT").expect("OCULAR_PREVIEW_OUTPUT");
        let input = image::open(input_path).unwrap().to_rgba8();
        let (w, h) = input.dimensions();
        // Sun position measured from the supplied 686x386 reference.
        let sx = 234.0 / 686.0;
        let sy = 112.0 / 386.0;
        let fov_h = 50_f64.to_radians();
        let fov_v = 2.0 * ((h as f64 / w as f64) * (fov_h * 0.5).tan()).atan();
        let source = BrightSource {
            angle_x: (sx - 0.5) * fov_h,
            angle_y: (sy - 0.5) * fov_v,
            rgb: [1.0, 0.91, 0.72],
        };
        let cfg = Config {
            enabled: true,
            pupil_mm: 6.4,
            age: 30.0,
            seed: 17,
            corona: 0.66,
            corona_radius: 0.28,
            halo: 0.08,
            halo_radius: 0.16,
            veil: 0.16,
            veil_radius: 0.72,
            tear: 0.22,
            phase: 38.0,
            squint: 1.45,
            squint_length: 1.10,
            squint_curve: 0.31,
            chromatic: 0.45,
        };
        let eye = render_config(cfg, &[source], w as usize, h as usize);
        let mut comparison = ImageBuffer::<Rgba<u8>, Vec<u8>>::new(w * 2, h);
        for y in 0..h {
            for x in 0..w {
                let src = input.get_pixel(x, y);
                comparison.put_pixel(x, y, *src);
                let e = eye[(y * w + x) as usize];
                let mut dst = [0u8; 4];
                for c in 0..3 {
                    let linear = (src[c] as f32 / 255.0).powf(2.2) + e[c] * 0.42;
                    dst[c] = (linear.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0) as u8;
                }
                dst[3] = src[3];
                comparison.put_pixel(w + x, y, Rgba(dst));
            }
        }
        comparison.save(output_path).unwrap();
    }

    #[test]
    #[ignore = "writes the unified physical + ocular reference render"]
    fn write_unified_reference() {
        let input_path = std::env::var("UNIFIED_PREVIEW_INPUT").expect("UNIFIED_PREVIEW_INPUT");
        let output_path = std::env::var("UNIFIED_PREVIEW_OUTPUT").expect("UNIFIED_PREVIEW_OUTPUT");
        let input = image::open(input_path).unwrap().to_rgba8();
        let (w, h) = input.dimensions();
        let sx = 234.0 / 686.0;
        let sy = 112.0 / 386.0;
        let fov_h = 50_f64.to_radians();
        let fov_v = 2.0 * ((h as f64 / w as f64) * (fov_h * 0.5).tan()).atan();
        let source = BrightSource {
            angle_x: (sx - 0.5) * fov_h,
            angle_y: (sy - 0.5) * fov_v,
            rgb: [1.0, 0.82, 0.58],
        };
        let eye = render_config(
            Config {
                enabled: false,
                pupil_mm: 5.4,
                age: 30.0,
                seed: 17,
                corona: 0.72,
                corona_radius: 0.24,
                halo: 0.10,
                halo_radius: 0.16,
                veil: 0.24,
                veil_radius: 0.68,
                tear: 0.12,
                phase: 23.0,
                squint: 0.38,
                squint_length: 0.92,
                squint_curve: 0.27,
                chromatic: 0.35,
            },
            &[source],
            w as usize,
            h as usize,
        );
        let pcfg = physical::Config {
            ray_grid: 32,
            gain: 240_000.0,
            ghost_blur: 0.006,
            bloom_strength: 2.8,
            bloom_radius: 0.018,
            ..physical::Config::default()
        };
        let ghosts = physical::render(
            &physical::bundled_lens(2),
            &[source],
            w as usize,
            h as usize,
            pcfg,
        );
        let bloom = physical::bloom(
            &physical::source_layer(&[source], w as usize, h as usize),
            w as usize,
            h as usize,
            pcfg,
        );
        let mut output = ImageBuffer::<Rgba<u8>, Vec<u8>>::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) as usize;
                let src = input.get_pixel(x, y);
                let mut dst = [0u8; 4];
                for c in 0..3 {
                    let base = (src[c] as f32 / 255.0).powf(2.2);
                    let optical = ghosts[i][c] * 1.15 + bloom[i][c] * 0.9 + eye[i][c] * 0.72;
                    dst[c] = ((base + optical).clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0) as u8;
                }
                dst[3] = src[3];
                output.put_pixel(x, y, Rgba(dst));
            }
        }
        output.save(output_path).unwrap();
    }

    #[test]
    #[ignore = "writes the black-background unified flare reference"]
    fn write_black_flare_reference() {
        let output_path = std::env::var("BLACK_FLARE_OUTPUT").expect("BLACK_FLARE_OUTPUT");
        let (w, h) = (1280usize, 720usize);
        let sx = 0.32;
        let sy = 0.42;
        let fov_h = 50_f64.to_radians();
        let fov_v = 2.0 * ((h as f64 / w as f64) * (fov_h * 0.5).tan()).atan();
        let source = BrightSource {
            angle_x: (sx - 0.5) * fov_h,
            angle_y: (sy - 0.5) * fov_v,
            rgb: [1.0, 0.78, 0.48],
        };
        let eye = render_config(
            Config {
                enabled: true,
                pupil_mm: 5.5,
                age: 32.0,
                seed: 17,
                corona: 0.34,
                corona_radius: 0.23,
                halo: 0.025,
                halo_radius: 0.16,
                veil: 0.18,
                veil_radius: 0.68,
                tear: 0.14,
                phase: 23.0,
                squint: 0.055,
                squint_length: 0.92,
                squint_curve: 0.27,
                chromatic: 0.42,
            },
            &[source],
            w,
            h,
        );
        let pcfg = physical::Config {
            ray_grid: 40,
            gain: 420_000.0,
            ghost_blur: 0.005,
            bloom_strength: 1.15,
            bloom_radius: 0.016,
            bloom_octaves: 5,
            ..physical::Config::default()
        };
        let ghosts = physical::render(&physical::bundled_lens(1), &[source], w, h, pcfg);
        let bloom = physical::bloom(&physical::source_layer(&[source], w, h), w, h, pcfg);
        let mut output = ImageBuffer::<Rgba<u8>, Vec<u8>>::new(w as u32, h as u32);
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                let dx = x as f32 + 0.5 - sx as f32 * w as f32;
                let dy = y as f32 + 0.5 - sy as f32 * h as f32;
                let core = 7.0 * (-(dx * dx + dy * dy) / (2.0 * 7.0_f32.powi(2))).exp();
                let stripe = 0.16
                    * (-(dy * dy) / (2.0 * 1.6_f32.powi(2))).exp()
                    * (-dx.abs() / (w as f32 * 0.34)).exp();
                let mut rays = 0.0_f32;
                for ray in 0..9 {
                    let rf = ray as f32;
                    let theta = -2.65 + rf * 0.69 + 0.17 * (rf * 4.73).sin();
                    let along = dx * theta.cos() + dy * theta.sin();
                    let perp = (-dx * theta.sin() + dy * theta.cos()).abs();
                    let length = 150.0 + 370.0 * (0.5 + 0.5 * (rf * 7.17 + 1.2).sin());
                    let weight = 0.18 + 0.82 * (0.5 + 0.5 * (rf * 5.31).cos()).powi(2);
                    if along > 0.0 {
                        rays += weight
                            * (-along / length).exp()
                            * (-(perp * perp) / (2.0 * (1.2 + rf % 3.0).powi(2))).exp();
                    }
                }
                let mut dst = [0u8; 4];
                for c in 0..3 {
                    let linear = ghosts[i][c] * 5.0
                        + bloom[i][c] * 0.42
                        + eye[i][c] * 0.10
                        + core * [1.0, 0.72, 0.42][c]
                        + stripe * [0.32, 0.58, 1.0][c]
                        + rays * 0.018 * [1.0, 0.72, 0.44][c];
                    let filmic = linear.max(0.0) / (1.0 + linear.max(0.0));
                    dst[c] = (filmic.powf(1.0 / 2.2) * 255.0).clamp(0.0, 255.0) as u8;
                }
                dst[3] = 255;
                output.put_pixel(x as u32, y as u32, Rgba(dst));
            }
        }
        output.save(output_path).unwrap();
    }
}
