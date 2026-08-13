// Starglow compute shader — matches CPU IIR quality via parallel prefix scan

struct Params {
    width: u32,
    height: u32,
    input_channel: u32,
    threshold: f32,
    threshold_soft: f32,
    boost_light: f32,
    // Streak pass params
    dx: i32,
    dy: i32,
    step: u32,
    decay: f32,
    // Shimmer
    shimmer_amount: f32,
    shimmer_detail: f32,
    shimmer_phase: f32,
    // Composite
    source_opacity: f32,
    starglow_opacity: f32,
    transfer_mode: u32,
    // Colormap (5 colors)
    cm0: vec4<f32>,
    cm1: vec4<f32>,
    cm2: vec4<f32>,
    cm3: vec4<f32>,
    cm4: vec4<f32>,
    // Control flags
    read_from_a: u32,
    has_map: u32,
    // Spectrum
    spectrum_mode: u32,
    spectrum_offset: f32,
    spectrum_density: f32,
    spectrum_random: f32,
    _pad0: u32,
    _pad1: u32,
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read_write> source: array<u32>;
@group(0) @binding(2) var<storage, read_write> buf_a: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read_write> buf_b: array<vec4<f32>>;
@group(0) @binding(4) var<storage, read_write> accum: array<vec4<f32>>;
@group(0) @binding(5) var<storage, read_write> output: array<u32>;
@group(0) @binding(6) var<storage, read> luma_map: array<f32>;

// ---- Helpers ----

fn unpack_argb(packed: u32) -> vec4<f32> {
    let a = f32((packed >> 24u) & 0xFFu) / 255.0;
    let r = f32((packed >> 16u) & 0xFFu) / 255.0;
    let g = f32((packed >> 8u) & 0xFFu) / 255.0;
    let b = f32(packed & 0xFFu) / 255.0;
    return vec4<f32>(a, r, g, b);
}

fn pack_argb(c: vec4<f32>) -> u32 {
    let a = u32(clamp(c.x * 255.0, 0.0, 255.0));
    let r = u32(clamp(c.y * 255.0, 0.0, 255.0));
    let g = u32(clamp(c.z * 255.0, 0.0, 255.0));
    let b = u32(clamp(c.w * 255.0, 0.0, 255.0));
    return (a << 24u) | (r << 16u) | (g << 8u) | b;
}

fn smoothstep_manual(low: f32, high: f32, x: f32) -> f32 {
    let t = clamp((x - low) / (high - low), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

fn extract_brightness(pixel: vec4<f32>) -> f32 {
    let r = pixel.y; let g = pixel.z; let b = pixel.w;
    switch params.input_channel {
        case 1u: { return (max(r, max(g, b)) + min(r, min(g, b))) * 0.5; }
        case 2u: { return 0.2126 * r + 0.7152 * g + 0.0722 * b; }
        case 3u: { return pixel.x; }
        case 4u: { return r; }
        case 5u: { return g; }
        case 6u: { return b; }
        default: { return 0.2126 * r + 0.7152 * g + 0.0722 * b; }
    }
}

fn sample_colormap(t: f32) -> vec3<f32> {
    let tc = clamp(t, 0.0, 1.0);
    let seg = tc * 4.0;
    let idx = u32(seg);
    let frac = seg - f32(idx);
    var c0: vec3<f32>; var c1: vec3<f32>;
    switch min(idx, 3u) {
        case 0u: { c0 = params.cm0.xyz; c1 = params.cm1.xyz; }
        case 1u: { c0 = params.cm1.xyz; c1 = params.cm2.xyz; }
        case 2u: { c0 = params.cm2.xyz; c1 = params.cm3.xyz; }
        case 3u: { c0 = params.cm3.xyz; c1 = params.cm4.xyz; }
        default: { c0 = params.cm4.xyz; c1 = params.cm4.xyz; }
    }
    return mix(c0, c1, frac);
}

fn hash_noise(ix: i32, iy: i32) -> f32 {
    var n = ix * 374761393 + iy * 668265263;
    n = (n ^ (n >> 13u)) * 1274126177;
    n = n ^ (n >> 16u);
    return f32(n & 0x7FFFFFFF) / f32(0x7FFFFFFF);
}

fn value_noise(x: f32, y: f32, phase: f32) -> f32 {
    let px = x + phase * 0.137;
    let ix = i32(floor(px)); let iy = i32(floor(y));
    var fx = px - floor(px); var fy = y - floor(y);
    fx = fx * fx * fx * (fx * (fx * 6.0 - 15.0) + 10.0);
    fy = fy * fy * fy * (fy * (fy * 6.0 - 15.0) + 10.0);
    let n00 = hash_noise(ix, iy); let n10 = hash_noise(ix + 1, iy);
    let n01 = hash_noise(ix, iy + 1); let n11 = hash_noise(ix + 1, iy + 1);
    return mix(mix(n00, n10, fx), mix(n01, n11, fx), fy);
}

// ---- Pass 1: Threshold ----
// Extracts bright pixels modulated by map. Writes to buf_a.
// read_from_a == 1 on first call → also clears accumulator.

@compute @workgroup_size(16, 16)
fn threshold_pass(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.width || id.y >= params.height { return; }
    let idx = id.y * params.width + id.x;

    let pixel = unpack_argb(source[idx]);
    let bright = extract_brightness(pixel);

    let soft_range = max(params.threshold_soft, 0.001);
    let low = params.threshold - soft_range * 0.5;
    let high = params.threshold + soft_range * 0.5;
    var mask = smoothstep_manual(low, high, bright) * params.boost_light;

    // Apply luminance map modulation
    if params.has_map != 0u {
        mask *= luma_map[idx];
    }

    buf_a[idx] = vec4<f32>(pixel.y * mask, pixel.z * mask, pixel.w * mask, 0.0);

    // Clear accumulator only on first direction
    if params.read_from_a == 1u {
        accum[idx] = vec4<f32>(0.0);
    }
}

// ---- Pass 2: Additive parallel prefix scan ----
// new[i] = current[i] + current[i - dir*step] * decay
// Mathematically equivalent to sequential IIR after log2(length) passes.

@compute @workgroup_size(16, 16)
fn streak_pass(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.width || id.y >= params.height { return; }
    let idx = id.y * params.width + id.x;

    let sx = i32(id.x) - params.dx * i32(params.step);
    let sy = i32(id.y) - params.dy * i32(params.step);

    var effective_decay = params.decay;
    if params.shimmer_amount > 0.0 {
        let noise = value_noise(
            f32(id.x) * params.shimmer_detail * 0.02,
            f32(id.y) * params.shimmer_detail * 0.02,
            params.shimmer_phase
        );
        effective_decay *= (1.0 - params.shimmer_amount * (1.0 - noise));
    }

    var current: vec4<f32>;
    var prev = vec4<f32>(0.0);

    if params.read_from_a != 0u {
        current = buf_a[idx];
        if sx >= 0 && sx < i32(params.width) && sy >= 0 && sy < i32(params.height) {
            prev = buf_a[u32(sy) * params.width + u32(sx)];
        }
        buf_b[idx] = current + vec4<f32>(prev.xyz * effective_decay, 0.0);
    } else {
        current = buf_b[idx];
        if sx >= 0 && sx < i32(params.width) && sy >= 0 && sy < i32(params.height) {
            prev = buf_b[u32(sy) * params.width + u32(sx)];
        }
        buf_a[idx] = current + vec4<f32>(prev.xyz * effective_decay, 0.0);
    }
}

// ---- Spectrum helpers ----

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

fn sample_spectrum(depth: f32, x: u32, y: u32) -> vec3<f32> {
    var rand_off = 0.0;
    if params.spectrum_random > 0.0 {
        rand_off = hash_noise(i32(x), i32(y)) * 360.0 * params.spectrum_random;
    }
    var hue = params.spectrum_offset + rand_off + depth * params.spectrum_density * 360.0;
    hue = hue % 360.0;
    if hue < 0.0 { hue += 360.0; }
    return hsv_to_rgb(hue, 1.0, 1.0);
}

// ---- Pass 3: Accumulate with colormap ----
// Subtracts original bright to get pure glow, applies colormap.

@compute @workgroup_size(16, 16)
fn accumulate_pass(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.width || id.y >= params.height { return; }
    let idx = id.y * params.width + id.x;

    var streak: vec4<f32>;
    if params.read_from_a != 0u {
        streak = buf_a[idx];
    } else {
        streak = buf_b[idx];
    }

    // Reconstruct original bright pixel value
    let pixel = unpack_argb(source[idx]);
    let bright = extract_brightness(pixel);
    let soft_range = max(params.threshold_soft, 0.001);
    let low = params.threshold - soft_range * 0.5;
    let high = params.threshold + soft_range * 0.5;
    var mask = smoothstep_manual(low, high, bright) * params.boost_light;
    if params.has_map != 0u {
        mask *= luma_map[idx];
    }
    let orig = vec3<f32>(pixel.y * mask, pixel.z * mask, pixel.w * mask);

    // Pure glow = streak - source bright
    let glow = max(streak.xyz - orig, vec3<f32>(0.0));
    let glow_lum = glow.x * 0.2126 + glow.y * 0.7152 + glow.z * 0.0722;
    let total_lum = streak.x * 0.2126 + streak.y * 0.7152 + streak.z * 0.0722;

    var depth = 0.0;
    if total_lum > 0.0001 {
        depth = clamp(glow_lum / total_lum, 0.0, 1.0);
    }

    var cm: vec3<f32>;
    if params.spectrum_mode != 0u {
        cm = sample_spectrum(depth, id.x, id.y);
    } else {
        cm = sample_colormap(depth);
    }
    accum[idx] += vec4<f32>(glow_lum * cm, 0.0);
}

// ---- Pass 4: Composite ----

fn apply_blend(sr: f32, sg: f32, sb: f32, gr: f32, gg: f32, gb: f32) -> vec3<f32> {
    switch params.transfer_mode {
        case 1u: { return vec3<f32>(0.0); }
        case 2u: { let a = clamp((gr + gg + gb) / 3.0, 0.0, 1.0); return vec3<f32>(gr, gg, gb) * a; }
        case 4u: { return vec3<f32>(gr, gg, gb); }
        case 5u: { return vec3<f32>(sr * gr, sg * gg, sb * gb); }
        case 6u: { return vec3<f32>(sr + gr - sr * gr, sg + gg - sg * gg, sb + gb - sb * gb); }
        default: { return vec3<f32>(gr, gg, gb); }
    }
}

@compute @workgroup_size(16, 16)
fn composite_pass(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.width || id.y >= params.height { return; }
    let idx = id.y * params.width + id.x;

    let pixel = unpack_argb(source[idx]);
    let glow = accum[idx].xyz * params.starglow_opacity;
    let blended = apply_blend(pixel.y, pixel.z, pixel.w, glow.x, glow.y, glow.z);
    let result = vec4<f32>(
        pixel.x,
        pixel.y * params.source_opacity + blended.x,
        pixel.z * params.source_opacity + blended.y,
        pixel.w * params.source_opacity + blended.z
    );
    output[idx] = pack_argb(result);
}
