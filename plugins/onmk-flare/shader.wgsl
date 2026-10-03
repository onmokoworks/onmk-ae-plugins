// onmkFlare v2.0 — cinematic lens flare with S_LensFlare-inspired elements
// Layout: scalars, six vec4 colors, then CPU-traced optical ghost profiles.

const PI: f32 = 3.14159265358979;

struct Params {
    // Scalars block (192 bytes, 16-byte aligned boundary)
    width: u32,
    height: u32,
    light_x: f32,
    light_y: f32,
    center_x: f32,
    center_y: f32,
    global_brightness: f32,
    global_scale: f32,
    flare_angle: f32,
    hotspot_intensity: f32,
    hotspot_size: f32,
    glow_intensity: f32,
    glow_radius: f32,
    glow_falloff: f32,
    streak_intensity: f32,
    streak_length: f32,
    streak_width: f32,
    streak_count: u32,
    streak_rotation: f32,
    stripe_intensity: f32,
    stripe_length: f32,
    stripe_width: f32,
    ring_intensity: f32,
    ring_radius: f32,
    ring_width: f32,
    ring_chromatic: f32,
    ring_spectrum: u32,
    starburst_intensity: f32,
    starburst_radius: f32,
    starburst_blades: u32,
    ghost_intensity: f32,
    ghost_count: u32,
    ghost_spread: f32,
    ghost_size: f32,
    ghost_chromatic: f32,
    edge_bright_mult: f32,
    edge_scale_mult: f32,
    atmosphere_amount: f32,
    atmosphere_scale: f32,
    chromatic_amount: f32,
    flicker_mod: f32,
    source_opacity: f32,
    flare_opacity: f32,
    transfer_mode: u32,
    source_mode: u32,
    _padding_0: u32,
    _padding_1: u32,
    _padding_2: u32,
    // Colors block (vec4 fields, each 16-byte aligned)
    glow_color: vec4<f32>,
    streak_color: vec4<f32>,
    stripe_color: vec4<f32>,
    ring_color: vec4<f32>,
    starburst_color: vec4<f32>,
    ghost_color: vec4<f32>,
    cooke_ghost_a: array<vec4<f32>, 12>,
    cooke_ghost_b: array<vec4<f32>, 12>,
    cooke_ghost_c: array<vec4<f32>, 12>,
};

@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var<storage, read> source: array<u32>;
@group(0) @binding(2) var<storage, read_write> output: array<u32>;
@group(0) @binding(3) var<storage, read> physical_flare: array<vec4<f32>>;

// ---- Helpers ----

fn unpack_argb(packed: u32) -> vec4<f32> {
    // Storage bytes match the host's ARGB8 order directly on little-endian hosts.
    let a = f32(packed & 0xFFu) / 255.0;
    let r = f32((packed >> 8u) & 0xFFu) / 255.0;
    let g = f32((packed >> 16u) & 0xFFu) / 255.0;
    let b = f32((packed >> 24u) & 0xFFu) / 255.0;
    return vec4<f32>(a, r, g, b);
}

fn pack_argb(c: vec4<f32>) -> u32 {
    let a = u32(clamp(c.x * 255.0, 0.0, 255.0));
    let r = u32(clamp(c.y * 255.0, 0.0, 255.0));
    let g = u32(clamp(c.z * 255.0, 0.0, 255.0));
    let b = u32(clamp(c.w * 255.0, 0.0, 255.0));
    return a | (r << 8u) | (g << 16u) | (b << 24u);
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> vec3<f32> {
    let c = v * s;
    let hp = h / 60.0;
    let x = c * (1.0 - abs(hp % 2.0 - 1.0));
    var r1: f32; var g1: f32; var b1: f32;
    let sector = u32(hp) % 6u;
    switch sector {
        case 0u: { r1 = c; g1 = x; b1 = 0.0; }
        case 1u: { r1 = x; g1 = c; b1 = 0.0; }
        case 2u: { r1 = 0.0; g1 = c; b1 = x; }
        case 3u: { r1 = 0.0; g1 = x; b1 = c; }
        case 4u: { r1 = x; g1 = 0.0; b1 = c; }
        default: { r1 = c; g1 = 0.0; b1 = x; }
    }
    let m = v - c;
    return vec3<f32>(r1 + m, g1 + m, b1 + m);
}

fn hash_noise(ix: i32, iy: i32) -> f32 {
    var n = ix * 374761393 + iy * 668265263;
    n = (n ^ (n >> 13u)) * 1274126177;
    n = n ^ (n >> 16u);
    return f32(n & 0x7FFFFFFF) / f32(0x7FFFFFFF);
}

fn value_noise(x: f32, y: f32) -> f32 {
    let ix = i32(floor(x)); let iy = i32(floor(y));
    var fx = x - floor(x); var fy = y - floor(y);
    fx = fx * fx * fx * (fx * (fx * 6.0 - 15.0) + 10.0);
    fy = fy * fy * fy * (fy * (fy * 6.0 - 15.0) + 10.0);
    let n00 = hash_noise(ix, iy); let n10 = hash_noise(ix + 1, iy);
    let n01 = hash_noise(ix, iy + 1); let n11 = hash_noise(ix + 1, iy + 1);
    return mix(mix(n00, n10, fx), mix(n01, n11, fx), fy);
}

fn atmosphere_fbm(x: f32, y: f32) -> f32 {
    let sx = x * p.atmosphere_scale * 0.01;
    let sy = y * p.atmosphere_scale * 0.01;
    let n1 = value_noise(sx, sy);
    let n2 = value_noise(sx * 2.0, sy * 2.0);
    let n3 = value_noise(sx * 4.0, sy * 4.0);
    return (n1 * 0.5 + n2 * 0.25 + n3 * 0.125) / 0.875;
}

// ---- 1. Hotspot: tiny intense bright core ----

fn compute_hotspot(dist: f32, scale: f32) -> f32 {
    let r = max(p.hotspot_size * scale, 0.5);
    return p.hotspot_intensity * exp(-dist * dist / (2.0 * r * r));
}

// ---- 2. Glow: multi-scale bloom (core, body, halo, veiling glare) ----

fn compute_glow(dist: f32, scale: f32) -> f32 {
    let r = p.glow_radius * scale;
    if r <= 0.0 { return 0.0; }
    let t = dist / r;
    let core = 0.34 * exp(-t * t / (2.0 * 0.18 * 0.18));
    let body = 0.33 / (1.0 + pow(t / 0.55, p.glow_falloff));
    let halo = 0.23 / (1.0 + pow(t / 1.35, p.glow_falloff * 1.15));
    let veil = 0.10 / (1.0 + pow(t / 3.2, max(p.glow_falloff * 0.72, 0.5)));
    return p.glow_intensity * (core + body + halo + veil);
}

// ---- 3. Streaks: multiple directional rays ----

fn compute_streaks(dx: f32, dy: f32, scale: f32) -> f32 {
    let len = p.streak_length * scale;
    let wid = p.streak_width * scale;
    if len <= 0.0 || wid <= 0.0 || p.streak_count == 0u { return 0.0; }

    var total = 0.0;
    let step = PI / f32(p.streak_count);
    let base_rot = p.streak_rotation * PI / 180.0;

    for (var i = 0u; i < p.streak_count; i++) {
        let fi = f32(i);
        let irregular = sin(fi * 12.9898 + 4.1414);
        let theta = base_rot + step * fi + irregular * step * 0.16;
        let cos_t = cos(theta);
        let sin_t = sin(theta);
        let along = dx * cos_t + dy * sin_t;
        let perp_val = abs(-dx * sin_t + dy * cos_t);
        let along_abs = abs(along);
        if along_abs < len && perp_val < wid * 4.0 {
            let along_fade = 1.0 - along_abs / len;
            let perp_fade = exp(-perp_val * perp_val / (2.0 * wid * wid));
            let weight = 0.28 + 0.72 * pow(0.5 + 0.5 * sin(fi * 7.173 + 1.7), 2.0);
            let broken = 0.72 + 0.28 * abs(sin(along_abs * 0.031 + fi * 2.31));
            total += pow(along_fade, 2.7) * perp_fade * weight * broken;
        }
    }
    return total * p.streak_intensity;
}

// ---- 4. Stripe: single anamorphic horizontal line ----

fn compute_stripe(dx: f32, dy: f32, scale: f32) -> f32 {
    let len = p.stripe_length * scale;
    let wid = p.stripe_width * scale;
    if len <= 0.0 || wid <= 0.0 { return 0.0; }
    let along_abs = abs(dx);
    let perp_val = abs(dy);
    if along_abs >= len || perp_val >= wid * 5.0 { return 0.0; }
    let along_fade = 1.0 - along_abs / len;
    let perp_fade = exp(-perp_val * perp_val / (2.0 * wid * wid));
    return pow(along_fade, 3.0) * perp_fade * p.stripe_intensity;
}

// ---- 5. Ring: with optional spectrum (chroma ring) mode ----

fn ring_single(dist: f32, radius: f32, rw: f32) -> f32 {
    if rw <= 0.0 { return 0.0; }
    let d = abs(dist - radius);
    return exp(-d * d / (2.0 * rw * rw));
}

fn ring_modulation(angle: f32) -> f32 {
    let uneven = 0.58 + 0.42 * (0.5 + 0.5 * sin(angle * 3.0 + 0.7));
    let broken = pow(0.5 + 0.5 * cos(angle * 7.0 - 1.2), 0.38);
    return uneven * (0.36 + 0.64 * broken);
}

fn compute_ring(dist: f32, angle: f32, scale: f32) -> vec3<f32> {
    let r = p.ring_radius * scale;
    let rw = p.ring_width * scale;
    if r <= 0.0 || rw <= 0.0 { return vec3<f32>(0.0); }

    if p.ring_spectrum != 0u {
        let ring_val = ring_single(dist, r, rw) * ring_modulation(angle);
        if ring_val <= 0.001 { return vec3<f32>(0.0); }
        let hue = fract((angle + p.flare_angle * PI / 180.0) / (2.0 * PI) + 0.5) * 360.0;
        let rgb = hsv_to_rgb(hue, 0.85, 1.0);
        return ring_val * rgb * p.ring_intensity;
    }

    let chroma = p.ring_chromatic * p.chromatic_amount * 5.0;
    let modulation = ring_modulation(angle);
    if chroma <= 0.001 {
        let v = ring_single(dist, r, rw) * modulation;
        return vec3<f32>(v) * p.ring_intensity * p.ring_color.xyz;
    }
    let rv = ring_single(dist, r * (1.0 - chroma * 0.02), rw);
    let gv = ring_single(dist, r, rw);
    let bv = ring_single(dist, r * (1.0 + chroma * 0.02), rw);
    return vec3<f32>(rv * p.ring_color.x, gv * p.ring_color.y, bv * p.ring_color.z) * p.ring_intensity * modulation;
}

// ---- 6. Starburst: iris diffraction pattern ----

fn compute_starburst(dist: f32, angle: f32, scale: f32) -> f32 {
    let r = p.starburst_radius * scale;
    if r <= 0.0 || p.starburst_blades < 3u { return 0.0; }
    let angle_offset = p.flare_angle * PI / 180.0;
    let warped = angle + angle_offset + 0.025 * pow(dist / max(r, 1.0), 2.0) * sin(angle * 2.0);
    let blade_angle = warped * f32(p.starburst_blades);
    let blade_mod = pow(0.5 + 0.5 * cos(blade_angle), 12.0);
    let lobe = 0.35 + 0.65 * pow(0.5 + 0.5 * sin(warped * 3.0 + 0.9), 2.0);
    let radial = exp(-1.35 * dist / max(r, 1.0)) / (1.0 + dist / max(r, 1.0));
    return blade_mod * radial * lobe * p.starburst_intensity;
}

// ---- 7. Ghost orbs: mixed disc + ring along flare axis ----

fn ghost_disc(dist2: f32, sigma2: f32) -> f32 {
    return exp(-dist2 / (2.0 * sigma2));
}

fn ghost_ring(dist: f32, size: f32) -> f32 {
    let ring_r = size * 0.8;
    let ring_w = size * 0.15;
    if ring_w <= 0.0 { return 0.0; }
    let d = abs(dist - ring_r);
    return exp(-d * d / (2.0 * ring_w * ring_w));
}

// Strongest Cooke Triplet surface-pair profiles, generated from the same
// MIT-derived optical model as src/optics.rs.
// A = axis position, size, Fresnel-normalized intensity, ring amount.
const COOKE_GHOST_A: array<vec4<f32>, 12> = array<vec4<f32>, 12>(
    vec4<f32>(-1.350000, 2.800000, 1.000000, 0.593530),
    vec4<f32>( 0.028173, 0.824894, 0.999440, 0.520000),
    vec4<f32>( 1.350000, 2.800000, 0.999440, 0.518492),
    vec4<f32>(-0.204399, 2.800000, 0.566590, 0.346783),
    vec4<f32>( 0.993208, 2.800000, 0.566590, 0.780327),
    vec4<f32>( 0.342623, 0.826216, 0.566590, 0.606667),
    vec4<f32>( 0.109503, 2.800000, 0.566590, 0.347599),
    vec4<f32>(-0.477817, 1.953844, 0.566238, 0.508004),
    vec4<f32>(-0.334036, 2.800000, 0.566238, 0.248670),
    vec4<f32>(-0.609264, 0.760537, 0.518913, 0.346667),
    vec4<f32>( 0.466996, 2.800000, 0.517808, 0.333713),
    vec4<f32>(-1.350000, 2.691416, 0.517808, 0.428090)
);
// B = aspect, rotation, red tint, green tint. Blue is normalized to 1.
const COOKE_GHOST_B: array<vec4<f32>, 12> = array<vec4<f32>, 12>(
    vec4<f32>(0.802520, 2.138629, 0.942497, 0.982113),
    vec4<f32>(0.989282, 0.782222, 0.932155, 0.952295),
    vec4<f32>(1.650000, 2.630037, 0.965482, 0.883526),
    vec4<f32>(1.010731, 2.249000, 1.000000, 0.633530),
    vec4<f32>(0.659147, 0.927629, 1.000000, 0.639620),
    vec4<f32>(1.000000, 3.031222, 1.000000, 0.734137),
    vec4<f32>(0.682222, 0.699444, 1.000000, 0.633530),
    vec4<f32>(0.520000, 2.712815, 1.000000, 0.642566),
    vec4<f32>(0.692064, 0.381037, 1.000000, 0.638939),
    vec4<f32>(0.980000, 0.062629, 1.000000, 0.646904),
    vec4<f32>(0.885855, 1.674815, 1.000000, 0.519599),
    vec4<f32>(0.684805, 1.419037, 1.000000, 0.521632)
);
const COOKE_GHOST_BLUE: array<f32, 12> = array<f32, 12>(
    0.861980, 1.000000, 0.940579, 0.820662, 0.819040, 0.877130,
    0.820662, 0.844309, 0.843692, 0.829615, 0.734283, 0.732297
);

fn ghost_shape(gdx: f32, gdy: f32, size: f32, aspect: f32,
               rotation: f32, ring_amount: f32) -> f32 {
    let cs = cos(rotation);
    let sn = sin(rotation);
    let ex = (gdx * cs + gdy * sn) / aspect;
    let ey = -gdx * sn + gdy * cs;
    let dist = sqrt(ex * ex + ey * ey);
    let sector = PI / 3.0;
    let local = ((atan2(ey, ex) + PI / 6.0) % sector) - PI / 6.0;
    let polygon_edge = cos(PI / 6.0) / max(cos(local), 0.01);
    let q = dist / (size * polygon_edge);
    let disc = exp(-0.5 * pow(q, 4.0));
    let rim = exp(-pow(q - 0.82, 2.0) / (2.0 * 0.085 * 0.085));
    let inner = exp(-pow(q - 0.46, 2.0) / (2.0 * 0.22 * 0.22));
    return disc * (1.0 - ring_amount) + rim * ring_amount + inner * 0.08;
}

fn compute_ghosts(px: f32, py: f32, scale: f32) -> vec3<f32> {
    if p.ghost_count == 0u { return vec3<f32>(0.0); }
    let base_size = p.ghost_size * scale;
    let chroma = p.ghost_chromatic * p.chromatic_amount * 3.0;

    var r_total = 0.0;
    var g_total = 0.0;
    var b_total = 0.0;

    for (var i = 0u; i < p.ghost_count; i++) {
        let optical_a = p.cooke_ghost_a[i];
        let optical_b = p.cooke_ghost_b[i];
        let pos = optical_a.x;
        let gx = p.center_x + (p.light_x - p.center_x) * pos;
        let gy = p.center_y + (p.light_y - p.center_y) * pos;
        let size_mult = optical_a.y;
        let intensity_mult = optical_a.z;
        let ring_amount = optical_a.w;
        let aspect = optical_b.x;
        let rotation = optical_b.y;
        let tint = vec3<f32>(optical_b.z, optical_b.w, p.cooke_ghost_c[i].x);

        let gdx = px - gx;
        let gdy = py - gy;
        let sz = base_size * size_mult;
        if sz <= 0.0 { continue; }

        if chroma <= 0.001 {
            let v = ghost_shape(gdx, gdy, sz, aspect, rotation, ring_amount) * intensity_mult;
            r_total += v * tint.x; g_total += v * tint.y; b_total += v * tint.z;
        } else {
            let shift = chroma * 2.0;
            let sz_r = sz * (1.0 - shift * 0.1);
            let sz_b = sz * (1.0 + shift * 0.1);
            r_total += ghost_shape(gdx, gdy, sz_r, aspect, rotation, ring_amount) * intensity_mult * tint.x;
            g_total += ghost_shape(gdx, gdy, sz, aspect, rotation, ring_amount) * intensity_mult * tint.y;
            b_total += ghost_shape(gdx, gdy, sz_b, aspect, rotation, ring_amount) * intensity_mult * tint.z;
        }
    }
    return vec3<f32>(r_total, g_total, b_total) * p.ghost_intensity * p.ghost_color.xyz;
}

// ---- Blend modes ----

fn apply_blend(src: vec3<f32>, flare: vec3<f32>) -> vec3<f32> {
    switch p.transfer_mode {
        case 1u: { return vec3<f32>(0.0); }
        case 2u: {
            let a = clamp((flare.x + flare.y + flare.z) / 3.0, 0.0, 1.0);
            return flare * a;
        }
        case 4u: { return flare; }
        case 5u: { return src + flare - src * flare; }
        case 7u: {
            let r = select(1.0 - 2.0 * (1.0 - src.x) * (1.0 - flare.x), 2.0 * src.x * flare.x, src.x < 0.5);
            let g = select(1.0 - 2.0 * (1.0 - src.y) * (1.0 - flare.y), 2.0 * src.y * flare.y, src.y < 0.5);
            let b = select(1.0 - 2.0 * (1.0 - src.z) * (1.0 - flare.z), 2.0 * src.z * flare.z, src.z < 0.5);
            return vec3<f32>(r, g, b);
        }
        case 8u: {
            let r = select(src.x + (2.0 * flare.x - 1.0) * (sqrt(src.x) - src.x),
                           src.x - (1.0 - 2.0 * flare.x) * src.x * (1.0 - src.x),
                           flare.x >= 0.5);
            let g = select(src.y + (2.0 * flare.y - 1.0) * (sqrt(src.y) - src.y),
                           src.y - (1.0 - 2.0 * flare.y) * src.y * (1.0 - src.y),
                           flare.y >= 0.5);
            let b = select(src.z + (2.0 * flare.z - 1.0) * (sqrt(src.z) - src.z),
                           src.z - (1.0 - 2.0 * flare.z) * src.z * (1.0 - src.z),
                           flare.z >= 0.5);
            return vec3<f32>(r, g, b);
        }
        default: { return flare; }
    }
}

// ---- Main pass ----

@compute @workgroup_size(16, 16)
fn flare_pass(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= p.width || id.y >= p.height { return; }
    let idx = id.y * p.width + id.x;

    let px = f32(id.x) + 0.5;
    let py = f32(id.y) + 0.5;

    let dx = px - p.light_x;
    let dy = py - p.light_y;
    let dist = sqrt(dx * dx + dy * dy);
    let angle = atan2(dy, dx);

    let brightness = p.global_brightness * p.flicker_mod * p.edge_bright_mult;
    let scale = p.global_scale * p.edge_scale_mult;

    var flare = vec3<f32>(0.0);

    // 1. Hotspot
    if p.hotspot_intensity > 0.0 {
        flare += vec3<f32>(compute_hotspot(dist, scale));
    }

    // 2. Glow
    if p.glow_intensity > 0.0 {
        flare += compute_glow(dist, scale) * p.glow_color.xyz;
    }

    // 3. Streaks
    if p.streak_intensity > 0.0 && p.streak_count > 0u {
        flare += compute_streaks(dx, dy, scale) * p.streak_color.xyz;
    }

    // 4. Stripe
    if p.stripe_intensity > 0.0 {
        flare += compute_stripe(dx, dy, scale) * p.stripe_color.xyz;
    }

    // 5. Ring
    if p.ring_intensity > 0.0 {
        flare += compute_ring(dist, angle, scale);
    }

    // 6. Starburst
    if p.starburst_intensity > 0.0 {
        flare += compute_starburst(dist, angle, scale) * p.starburst_color.xyz;
    }

    // 7. Full wavelength ray-traced optical ghosts and chromatic bloom.
    // Camera ghosts are pre-scaled on the CPU; this buffer also carries the
    // independent human-eye glare layer, so it must remain visible at zero
    // Ghost Intensity.
    flare += physical_flare[idx].xyz;

    // Atmosphere noise modulation
    if p.atmosphere_amount > 0.0 {
        let noise = atmosphere_fbm(px, py);
        let atmo = 1.0 - p.atmosphere_amount * (1.0 - noise);
        flare *= atmo;
    }

    // Manual source guarantee: independent of input luminance and resilient
    // to invalid values in optional physical layers.
    if p.source_mode == 1u || p.source_mode == 3u {
        let sigma = max(2.2 * scale, 0.75);
        let manual_core = 3.5 * exp(-dist * dist / (2.0 * sigma * sigma))
            + 0.22 / (1.0 + pow(dist / (18.0 * max(scale, 0.1)), 3.0));
        // WGSL has no portable isFinite() builtin.  NaN fails equality with
        // itself, while this bound also rejects infinities and absurd values.
        let flare_is_finite = all(flare == flare) && all(abs(flare) < vec3<f32>(1.0e20));
        flare = select(vec3<f32>(0.0), flare, flare_is_finite);
        flare += manual_core * vec3<f32>(1.0, 0.92, 0.76);
    }

    flare *= brightness * p.flare_opacity;

    // Composite
    let pixel = unpack_argb(source[idx]);
    let src_rgb = vec3<f32>(pixel.y, pixel.z, pixel.w);
    let blended = apply_blend(src_rgb, flare);
    let result = vec4<f32>(
        pixel.x,
        src_rgb.x * p.source_opacity + blended.x,
        src_rgb.y * p.source_opacity + blended.y,
        src_rgb.z * p.source_opacity + blended.z
    );
    output[idx] = pack_argb(result);
}
