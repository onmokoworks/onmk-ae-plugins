use crate::physical;
use crate::EffectParams;

const PI: f64 = std::f64::consts::PI;

pub fn process(ep: &EffectParams, src: &[u8], w: usize, h: usize) -> Vec<u8> {
    let mut out = src.to_vec();
    let n = w * h;

    let flicker_mod = compute_flicker(ep);
    let (edge_bright, edge_scale) = compute_edge_trigger(ep, w, h);

    let brightness = ep.global_brightness * flicker_mod * edge_bright;
    let scale = ep.global_scale * edge_scale;
    let lx = ep.light_x;
    let ly = ep.light_y;

    let streak_angle_rad = ep.streak_rotation * PI / 180.0;

    let physical_cfg = physical::Config {
        ray_grid: ep.ray_grid.min(32),
        min_ghost: 1e-7,
        gain: ep.physical_gain,
        normalize: ep.ghost_normalize,
        max_area_boost: ep.max_area_boost,
        ghost_blur: ep.physical_ghost_blur,
        ghost_blur_passes: ep.physical_blur_passes,
        surface_raster: true,
        bloom_strength: ep.bloom_strength,
        bloom_radius: ep.bloom_radius,
        bloom_passes: ep.bloom_passes,
        bloom_octaves: ep.bloom_octaves,
        bloom_chromatic: ep.bloom_chromatic,
    };
    let fov_h = 50_f64.to_radians();
    let fov_v = 2.0 * ((h as f64 / w as f64) * (fov_h * 0.5).tan()).atan();
    let mut sources = if ep.source_mode >= 2 {
        physical::prune_sources(
            physical::extract_sources(src, w, h, ep.source_threshold, ep.source_downsample),
            4,
        )
    } else {
        Vec::new()
    };
    if ep.source_mode == 1 || ep.source_mode == 3 {
        sources.push(physical::BrightSource {
            angle_x: (lx / w as f64 - 0.5) * fov_h,
            angle_y: (ly / h as f64 - 0.5) * fov_v,
            rgb: [1.0; 3],
        });
    }
    let (physical_ghosts, physical_bloom) = if ep.physical_enabled {
        let lens = physical::bundled_lens(ep.lens_preset);
        let ghosts = physical::render(&lens, &sources, w, h, physical_cfg);
        let bloom_source = physical::source_layer(&sources, w, h);
        let bloom = physical::bloom(&bloom_source, w, h, physical_cfg);
        (ghosts, bloom)
    } else {
        (vec![[0.0; 3]; n], vec![[0.0; 3]; n])
    };
    let ocular_glare = crate::ocular::render(ep, &sources, w, h);
    let source_optics = crate::source_optics::render(ep, &sources, w, h);

    for i in 0..n {
        let px = (i % w) as f64 + 0.5;
        let py = (i / w) as f64 + 0.5;

        let dx = px - lx;
        let dy = py - ly;
        let dist = (dx * dx + dy * dy).sqrt();
        let angle = dy.atan2(dx);

        let mut flare_r = 0.0;
        let mut flare_g = 0.0;
        let mut flare_b = 0.0;

        // 1. Hotspot — small intense bright core
        if ep.style_preset >= 6 && ep.hotspot_intensity > 0.0 {
            let r = (ep.hotspot_size * scale).max(0.5);
            let hotspot = ep.hotspot_intensity * (-dist * dist / (2.0 * r * r)).exp();
            flare_r += hotspot;
            flare_g += hotspot;
            flare_b += hotspot;
        }

        // 2. Glow — multi-scale bloom (core, body, halo, veiling glare)
        if ep.style_preset >= 6 && ep.glow_intensity > 0.0 {
            let r = ep.glow_radius * scale;
            if r > 0.0 {
                let t = dist / r;
                let core = 0.34 * (-t * t / (2.0 * 0.18_f64.powi(2))).exp();
                let body = 0.33 / (1.0 + (t / 0.55).powf(ep.glow_falloff));
                let halo = 0.23 / (1.0 + (t / 1.35).powf(ep.glow_falloff * 1.15));
                let veil = 0.10 / (1.0 + (t / 3.2).powf((ep.glow_falloff * 0.72).max(0.5)));
                let glow = ep.glow_intensity * (core + body + halo + veil);
                flare_r += glow * ep.glow_color[0];
                flare_g += glow * ep.glow_color[1];
                flare_b += glow * ep.glow_color[2];
            }
        }

        // 3. Streaks — multiple rays
        if ep.streak_intensity > 0.0 && ep.streak_count > 0 {
            let len = ep.streak_length * scale;
            let wid = ep.streak_width * scale;
            if len > 0.0 && wid > 0.0 {
                let s = compute_streaks(dx, dy, ep.streak_count, streak_angle_rad, len, wid);
                flare_r += s * ep.streak_intensity * ep.streak_color[0];
                flare_g += s * ep.streak_intensity * ep.streak_color[1];
                flare_b += s * ep.streak_intensity * ep.streak_color[2];
            }
        }

        // 4. Stripe — single anamorphic horizontal line
        if ep.stripe_intensity > 0.0 {
            let len = ep.stripe_length * scale;
            let wid = ep.stripe_width * scale;
            if len > 0.0 && wid > 0.0 {
                let s = compute_stripe(dx, dy, len, wid);
                flare_r += s * ep.stripe_intensity * ep.stripe_color[0];
                flare_g += s * ep.stripe_intensity * ep.stripe_color[1];
                flare_b += s * ep.stripe_intensity * ep.stripe_color[2];
            }
        }

        // 5. Ring — with optional spectrum mode
        if ep.ring_intensity > 0.0 {
            let r = ep.ring_radius * scale;
            let ring_w = ep.ring_width * scale;
            if r > 0.0 && ring_w > 0.0 {
                if ep.ring_spectrum {
                    let (rr, rg, rb) =
                        compute_ring_spectrum(dist, angle, r, ring_w, ep.flare_angle);
                    flare_r += rr * ep.ring_intensity;
                    flare_g += rg * ep.ring_intensity;
                    flare_b += rb * ep.ring_intensity;
                } else {
                    let chroma = ep.ring_chromatic * ep.chromatic_amount * 5.0;
                    let (rr, rg, rb) = compute_ring_chromatic(dist, angle, r, ring_w, chroma);
                    flare_r += rr * ep.ring_intensity * ep.ring_color[0];
                    flare_g += rg * ep.ring_intensity * ep.ring_color[1];
                    flare_b += rb * ep.ring_intensity * ep.ring_color[2];
                }
            }
        }

        // 6. Starburst
        if ep.style_preset >= 6 && ep.starburst_intensity > 0.0 && ep.starburst_blades >= 3 {
            let r = ep.starburst_radius * scale;
            if r > 0.0 {
                let s = compute_starburst(dist, angle, r, ep.starburst_blades, ep.flare_angle);
                flare_r += s * ep.starburst_intensity * ep.starburst_color[0];
                flare_g += s * ep.starburst_intensity * ep.starburst_color[1];
                flare_b += s * ep.starburst_intensity * ep.starburst_color[2];
            }
        }

        // 7. Physically traced ghost reflections + multi-octave bloom
        if ep.ghost_intensity > 0.0 {
            let pg = physical_ghosts[i];
            let pb = physical_bloom[i];
            flare_r += finite((pg[0] + pb[0]) as f64) * ep.ghost_intensity;
            flare_g += finite((pg[1] + pb[1]) as f64) * ep.ghost_intensity;
            flare_b += finite((pg[2] + pb[2]) as f64) * ep.ghost_intensity;
        }
        if ep.ocular_enabled {
            let eye = ocular_glare[i];
            flare_r += eye[0] as f64;
            flare_g += eye[1] as f64;
            flare_b += eye[2] as f64;
        }
        if ep.style_preset < 6 {
            let source = source_optics[i];
            flare_r += finite(source[0] as f64);
            flare_g += finite(source[1] as f64);
            flare_b += finite(source[2] as f64);
        }

        // Atmosphere noise modulation
        if ep.atmosphere_amount > 0.0 {
            let noise = atmosphere_noise(px, py, ep.atmosphere_scale);
            let atmo = 1.0 - ep.atmosphere_amount * (1.0 - noise);
            flare_r *= atmo;
            flare_g *= atmo;
            flare_b *= atmo;
        }

        // The manual emitter is independent of plate luminance and of the
        // traced-ghost buffers.  Keeping a compact analytic core here also
        // prevents one bad optical sample from erasing the visible source.
        if ep.source_mode == 1 || ep.source_mode == 3 {
            let sigma = (2.2 * scale).max(0.75);
            let manual_core = 3.5 * (-dist * dist / (2.0 * sigma * sigma)).exp()
                + 0.22 / (1.0 + (dist / (18.0 * scale.max(0.1))).powi(3));
            flare_r = finite(flare_r) + manual_core;
            flare_g = finite(flare_g) + manual_core * 0.92;
            flare_b = finite(flare_b) + manual_core * 0.76;
        }

        // Apply global brightness and flare opacity
        flare_r = finite(flare_r * brightness * ep.flare_opacity);
        flare_g = finite(flare_g * brightness * ep.flare_opacity);
        flare_b = finite(flare_b * brightness * ep.flare_opacity);

        // Composite
        let off = i * 4;
        let sa = src[off] as f64 / 255.0;
        let sr = src[off + 1] as f64 / 255.0;
        let sg = src[off + 2] as f64 / 255.0;
        let sb = src[off + 3] as f64 / 255.0;

        let (br, bg, bb) = apply_blend(sr, sg, sb, flare_r, flare_g, flare_b, ep.transfer_mode);

        let or = (sr * ep.source_opacity + br).min(1.0);
        let og = (sg * ep.source_opacity + bg).min(1.0);
        let ob = (sb * ep.source_opacity + bb).min(1.0);

        out[off] = (sa * 255.0) as u8;
        out[off + 1] = (finite(or) * 255.0).clamp(0.0, 255.0) as u8;
        out[off + 2] = (finite(og) * 255.0).clamp(0.0, 255.0) as u8;
        out[off + 3] = (finite(ob) * 255.0).clamp(0.0, 255.0) as u8;
    }

    out
}

#[inline]
fn finite(value: f64) -> f64 {
    if value.is_finite() {
        value
    } else {
        0.0
    }
}

fn compute_flicker(ep: &EffectParams) -> f64 {
    if ep.flicker_amount <= 0.0 {
        return 1.0;
    }
    let phase = ep.flicker_phase * PI / 180.0;
    let noise = (phase * 2.137).sin() * 0.5
        + (phase * 5.891).sin() * 0.25
        + (phase * 11.23).sin() * 0.125
        + (phase * 23.71).sin() * 0.0625;
    let norm = noise / 0.9375;
    1.0 - ep.flicker_amount * 0.5 * (1.0 - norm)
}

// S_LensFlare-style edge triggering: brightness/scale increase near frame edge
fn compute_edge_trigger(ep: &EffectParams, w: usize, h: usize) -> (f64, f64) {
    if ep.edge_width <= 0.0 {
        return (1.0, 1.0);
    }
    let wf = w as f64;
    let hf = h as f64;
    let lx = ep.light_x;
    let ly = ep.light_y;

    let dist_left = lx;
    let dist_right = wf - lx;
    let dist_top = ly;
    let dist_bottom = hf - ly;
    let min_dist = dist_left.min(dist_right).min(dist_top).min(dist_bottom);

    let zone = ep.edge_width * wf.min(hf) * 0.5;
    if zone <= 0.0 {
        return (1.0, 1.0);
    }

    let t = (1.0 - (min_dist / zone).min(1.0)).max(0.0);
    let trigger = t * t * (3.0 - 2.0 * t); // smoothstep

    // Edge trigger is an accent, never a visibility gate.  Invalid/legacy
    // parameter values must not multiply the complete flare down to black.
    let bright = (1.0 + (ep.edge_brightness - 1.0) * trigger).max(1.0);
    let sc = (1.0 + (ep.edge_scale - 1.0) * trigger).max(1.0);
    (bright, sc)
}

fn compute_streaks(dx: f64, dy: f64, count: i32, base_rot: f64, length: f64, width: f64) -> f64 {
    let mut total = 0.0;
    let step = PI / count as f64;
    for i in 0..count {
        let fi = i as f64;
        let irregular = (fi * 12.9898 + 4.1414).sin();
        let theta = base_rot + step * fi + irregular * step * 0.16;
        let cos_t = theta.cos();
        let sin_t = theta.sin();
        let along = dx * cos_t + dy * sin_t;
        let perp = (-dx * sin_t + dy * cos_t).abs();
        let along_abs = along.abs();
        if along_abs < length && perp < width * 4.0 {
            let along_fade = 1.0 - along_abs / length;
            let perp_fade = (-perp * perp / (2.0 * width * width)).exp();
            let weight = 0.28 + 0.72 * (0.5 + 0.5 * (fi * 7.173 + 1.7).sin()).powi(2);
            let broken = 0.72 + 0.28 * (along_abs * 0.031 + fi * 2.31).sin().abs();
            total += along_fade.powf(2.7) * perp_fade * weight * broken;
        }
    }
    total
}

// Horizontal anamorphic stripe (like S_LensFlare Stripe element)
fn compute_stripe(dx: f64, dy: f64, length: f64, width: f64) -> f64 {
    let along_abs = dx.abs();
    let perp = dy.abs();
    if along_abs >= length || perp >= width * 5.0 {
        return 0.0;
    }
    let along_fade = 1.0 - along_abs / length;
    let perp_fade = (-perp * perp / (2.0 * width * width)).exp();
    along_fade.powi(3) * perp_fade
}

fn compute_ring(dist: f64, radius: f64, width: f64) -> f64 {
    let d = (dist - radius).abs();
    (-d * d / (2.0 * width * width)).exp()
}

fn ring_modulation(angle: f64) -> f64 {
    let uneven = 0.58 + 0.42 * (0.5 + 0.5 * (angle * 3.0 + 0.7).sin());
    let broken = (0.5 + 0.5 * (angle * 7.0 - 1.2).cos()).powf(0.38);
    uneven * (0.36 + 0.64 * broken)
}

fn compute_ring_chromatic(
    dist: f64,
    angle: f64,
    radius: f64,
    width: f64,
    chroma: f64,
) -> (f64, f64, f64) {
    let modulation = ring_modulation(angle);
    if chroma <= 0.001 {
        let v = compute_ring(dist, radius, width) * modulation;
        return (v, v, v);
    }
    let r = compute_ring(dist, radius * (1.0 - chroma * 0.02), width) * modulation;
    let g = compute_ring(dist, radius, width) * modulation;
    let b = compute_ring(dist, radius * (1.0 + chroma * 0.02), width) * modulation;
    (r, g, b)
}

// S_LensFlare-style Chroma Ring: rainbow colors around the ring
fn compute_ring_spectrum(
    dist: f64,
    angle: f64,
    radius: f64,
    width: f64,
    flare_angle: f64,
) -> (f64, f64, f64) {
    let ring_val = compute_ring(dist, radius, width) * ring_modulation(angle);
    if ring_val <= 0.001 {
        return (0.0, 0.0, 0.0);
    }
    let hue = ((angle + flare_angle * PI / 180.0) / (2.0 * PI) + 0.5).fract() * 360.0;
    let (r, g, b) = hsv_to_rgb(hue, 0.85, 1.0);
    (ring_val * r, ring_val * g, ring_val * b)
}

fn hsv_to_rgb(h: f64, s: f64, v: f64) -> (f64, f64, f64) {
    let c = v * s;
    let hp = h / 60.0;
    let x = c * (1.0 - ((hp % 2.0) - 1.0).abs());
    let (r1, g1, b1) = match hp as i32 % 6 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    (r1 + m, g1 + m, b1 + m)
}

fn compute_starburst(dist: f64, angle: f64, radius: f64, blades: i32, flare_angle: f64) -> f64 {
    let angle_offset = flare_angle * PI / 180.0;
    let warped =
        angle + angle_offset + 0.025 * (dist / radius.max(1.0)).powi(2) * (angle * 2.0).sin();
    let blade_angle = warped * blades as f64;
    let blade_mod = (0.5 + 0.5 * blade_angle.cos()).powi(12);
    let lobe = 0.35 + 0.65 * (0.5 + 0.5 * (warped * 3.0 + 0.9).sin()).powi(2);
    let radial = (-1.35 * dist / radius.max(1.0)).exp() / (1.0 + dist / radius.max(1.0));
    blade_mod * radial * lobe
}

// Fractal value noise for atmosphere (S_LensFlare Atmosphere effect)
fn atmosphere_noise(x: f64, y: f64, scale: f64) -> f64 {
    let sx = x * scale * 0.01;
    let sy = y * scale * 0.01;
    let n1 = value_noise(sx, sy);
    let n2 = value_noise(sx * 2.0, sy * 2.0);
    let n3 = value_noise(sx * 4.0, sy * 4.0);
    (n1 * 0.5 + n2 * 0.25 + n3 * 0.125) / 0.875
}

fn value_noise(x: f64, y: f64) -> f64 {
    let ix = x.floor() as i32;
    let iy = y.floor() as i32;
    let mut fx = x - x.floor();
    let mut fy = y - y.floor();
    fx = fx * fx * fx * (fx * (fx * 6.0 - 15.0) + 10.0);
    fy = fy * fy * fy * (fy * (fy * 6.0 - 15.0) + 10.0);
    let n00 = hash_f(ix, iy);
    let n10 = hash_f(ix + 1, iy);
    let n01 = hash_f(ix, iy + 1);
    let n11 = hash_f(ix + 1, iy + 1);
    let nx0 = n00 + (n10 - n00) * fx;
    let nx1 = n01 + (n11 - n01) * fx;
    nx0 + (nx1 - nx0) * fy
}

fn hash_f(x: i32, y: i32) -> f64 {
    let mut n = (x.wrapping_mul(374761393)).wrapping_add(y.wrapping_mul(668265263));
    n = (n ^ (n >> 13)).wrapping_mul(1274126177);
    n = n ^ (n >> 16);
    (n & 0x7FFFFFFF) as f64 / 0x7FFFFFFF as f64
}

fn apply_blend(sr: f64, sg: f64, sb: f64, fr: f64, fg: f64, fb: f64, mode: i32) -> (f64, f64, f64) {
    match mode {
        1 => (0.0, 0.0, 0.0),
        2 => {
            let a = ((fr + fg + fb) / 3.0).min(1.0);
            (fr * a, fg * a, fb * a)
        }
        4 => (fr, fg, fb),
        5 => (sr + fr - sr * fr, sg + fg - sg * fg, sb + fb - sb * fb),
        7 => {
            let overlay = |s: f64, f: f64| {
                if s < 0.5 {
                    2.0 * s * f
                } else {
                    1.0 - 2.0 * (1.0 - s) * (1.0 - f)
                }
            };
            (overlay(sr, fr), overlay(sg, fg), overlay(sb, fb))
        }
        8 => {
            let soft = |s: f64, f: f64| {
                if f < 0.5 {
                    s - (1.0 - 2.0 * f) * s * (1.0 - s)
                } else {
                    s + (2.0 * f - 1.0) * (s.sqrt() - s)
                }
            };
            (soft(sr, fr), soft(sg, fg), soft(sb, fb))
        }
        _ => (fr, fg, fb),
    }
}
