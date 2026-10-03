// CPU reference: src/source_optics.rs. One invocation owns one pixel;
// sources accumulate in the same energy-sorted order, without float atomics.
struct Config { data: array<vec4<f32>, 27> }
@group(0) @binding(0) var<uniform> config: Config;
@group(0) @binding(1) var<storage, read_write> output: array<vec4<f32>>;
const PI: f32 = 3.141592653589793;
fn square(x: f32) -> f32 { return x * x; }
fn gaussian(r: f32, sigma: f32) -> f32 {
    let exponent = -0.5 * square(r / max(sigma, 0.25));
    if exponent <= -104.0 { return 0.0; }
    return exp(exponent);
}
fn euclidean(x: f32, divisor: f32) -> f32 {
    // Rust rem_euclid starts with truncating remainder, not floor division.
    let remainder = x - trunc(x / divisor) * divisor;
    return select(remainder, remainder + divisor, remainder < 0.0);
}
@compute @workgroup_size(16, 16)
fn source_pass(@builtin(global_invocation_id) id: vec3<u32>) {
    let width = u32(config.data[0].x);
    let height = u32(config.data[0].y);
    if id.x >= width || id.y >= height { return; }
    let diffraction = config.data[1].x;
    let scale = config.data[1].y;
    let blades = config.data[1].z;
    let rotation = config.data[1].w;
    let bloom_strength = config.data[2].x;
    var result = vec3<f32>(0.0);
    for (var i = 0u; i < u32(config.data[0].z); i += 1u) {
        let pos = config.data[3u + i * 3u];
        let shape = config.data[4u + i * 3u];
        let rgb = config.data[5u + i * 3u].xyz;
        let dx = f32(id.x) + 0.5 - pos.x;
        let dy = f32(id.y) + 0.5 - pos.y;
        let r = max(sqrt(dx * dx + dy * dy), 0.001);
        var direction = 0.0;
        if dx != 0.0 || dy != 0.0 { direction = atan2(dy, dx); }
        let theta = direction - rotation;
        let pin = gaussian(r, 1.15 * scale) * 10.0;
        let white_core = gaussian(r, 3.8 * scale) * 2.7;
        let near = gaussian(r, 13.0 * scale) * 0.34;
        let shoulder = 0.10 / (1.0 + pow(r / (42.0 * scale), 2.35));
        let sector = PI / blades;
        let local = euclidean(theta + sector * 0.5, sector) - sector * 0.5;
        let perp = r * abs(sin(local));
        let radial = exp(-r / max(shape.x, 1.0));
        let needle = exp(-0.5 * square(perp / (0.58 + 0.0065 * r))) * radial;
        let skirt = exp(-0.5 * square(perp / (2.3 + 0.012 * r)))
            * exp(-r / max(shape.x * 0.52, 1.0));
        let secondary_angle = theta + sector * 0.5;
        let secondary_local = euclidean(secondary_angle + sector * 0.5, sector) - sector * 0.5;
        let secondary_perp = r * abs(sin(secondary_local));
        let secondary = exp(-0.5 * square(secondary_perp / (0.9 + 0.009 * r)))
            * exp(-r / max(shape.x * 0.58, 1.0));
        let ripple = 0.82 + 0.18 * square(cos(r * (0.105 + f32(i) * 0.007) + theta * blades * 0.35));
        let diffraction_value = diffraction * (0.105 * needle + 0.030 * skirt + 0.014 * secondary) * ripple;
        let phase = r / max(3.2 * scale, 0.5) * PI;
        let airy = diffraction * 0.025 * square(sin(phase) / max(phase, 1.0)) * exp(-r / (48.0 * scale));
        let along = dx * shape.z + dy * shape.w;
        let across = -dx * shape.w + dy * shape.z;
        let shifted = along - shape.y * 0.20;
        let haze_q = square(shifted / (shape.y * 1.35)) + square(across / (shape.y * 0.78));
        let variation = 0.82 + 0.10 * sin(dx * 0.013 + 1.7) + 0.08 * cos(dy * 0.017 - 0.9);
        let haze = (0.016 + bloom_strength * 0.007) * pos.z * exp(-1.7 * haze_q) * max(variation, 0.45);
        let wave = vec3<f32>(1.07, 1.0, 0.94);
        let core_colour = vec3<f32>(1.0, 0.91, 0.72);
        let haze_colour = vec3<f32>(1.0, 0.62, 0.34);
        let airy_colour = vec3<f32>(0.72, 0.88, 1.0);
        for (var c = 0u; c < 3u; c += 1u) {
            var numerator = radial;
            if c != 1u { numerator = exp(-r / max(shape.x * wave[c], 1.0)); }
            let spectral = numerator / max(radial, 1e-5);
            let source_colour = max(rgb[c], 0.05);
            result[c] += pos.w * source_colour * ((pin + white_core) + near * core_colour[c] + shoulder * core_colour[c])
                + diffraction_value * spectral * source_colour + airy * airy_colour[c] + haze * haze_colour[c];
        }
    }
    output[id.y * width + id.x] = vec4<f32>(result, 0.0);
}
