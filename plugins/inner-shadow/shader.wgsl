// onmk Inner Shadow compute passes. Mirrors src/cpu.rs (docs/design.md §7.1).
// All planes are f32, row-major, w x h; samples outside the plane are 1.0
// (exterior / transparent boundary condition).

struct U {
    w: u32,
    h: u32,
    out_w: u32,
    out_h: u32,
    i0: i32,
    i1: i32,
    i2: i32,
    i3: i32,
    f0: f32,
    f1: f32,
    f2: f32,
    f3: f32,
    f4: f32,
    f5: f32,
    f6: f32,
    f7: f32,
};

@group(0) @binding(0) var<storage, read> src0: array<f32>;
@group(0) @binding(1) var<storage, read> src1: array<f32>;
@group(0) @binding(2) var<storage, read_write> dst: array<f32>;
@group(0) @binding(3) var<storage, read> aux: array<u32>;
@group(0) @binding(4) var<uniform> u: U;

fn inside(x: i32, y: i32) -> bool {
    return x >= 0 && y >= 0 && x < i32(u.w) && y < i32(u.h);
}

fn at0(x: i32, y: i32) -> f32 {
    if (!inside(x, y)) {
        return 1.0;
    }
    return src0[u32(y) * u.w + u32(x)];
}

// Sparse max table step: dst(x) = max(src(x), src(x + half)); i0 = half.
@compute @workgroup_size(16, 16, 1)
fn level(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= u.w || id.y >= u.h) {
        return;
    }
    let x = i32(id.x);
    let y = i32(id.y);
    dst[id.y * u.w + id.x] = max(at0(x, y), at0(x + u.i0, y));
}

// Span accumulation for one level. i0 = 2^level, i1 = first span,
// i2 = span count, i3 = 1 to overwrite (first level) else max-accumulate.
// aux holds (dy, half_width) pairs as i32 bit patterns.
@compute @workgroup_size(16, 16, 1)
fn accum(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= u.w || id.y >= u.h) {
        return;
    }
    let x = i32(id.x);
    let y = i32(id.y);
    let i = id.y * u.w + id.x;
    var m = 0.0;
    if (u.i3 == 0) {
        m = dst[i];
    }
    for (var s = 0; s < u.i2; s = s + 1) {
        let k = u32(u.i1 + s) * 2u;
        let dy = bitcast<i32>(aux[k]);
        let hw = bitcast<i32>(aux[k + 1u]);
        m = max(m, max(at0(x - hw, y + dy), at0(x + hw - u.i0 + 1, y + dy)));
    }
    dst[i] = m;
}

// lo + (hi - lo) * mix with the exterior boundary; f0 = mix.
fn mixed(x: i32, y: i32) -> f32 {
    if (!inside(x, y)) {
        return 1.0;
    }
    let i = u32(y) * u.w + u32(x);
    let lo = src0[i];
    return lo + (src1[i] - lo) * u.f0;
}

// Horizontal Gaussian; i0 = radius, aux = f32 weights (2 * radius + 1).
@compute @workgroup_size(16, 16, 1)
fn blur_h(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= u.w || id.y >= u.h) {
        return;
    }
    let x = i32(id.x);
    let y = i32(id.y);
    var sum = 0.0;
    let n = 2 * u.i0 + 1;
    for (var k = 0; k < n; k = k + 1) {
        sum = sum + bitcast<f32>(aux[u32(k)]) * mixed(x + k - u.i0, y);
    }
    dst[id.y * u.w + id.x] = sum;
}

// Vertical Gaussian; i0 = radius, aux = f32 weights.
@compute @workgroup_size(16, 16, 1)
fn blur_v(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= u.w || id.y >= u.h) {
        return;
    }
    let x = i32(id.x);
    let y = i32(id.y);
    var sum = 0.0;
    let n = 2 * u.i0 + 1;
    for (var k = 0; k < n; k = k + 1) {
        sum = sum + bitcast<f32>(aux[u32(k)]) * at0(x, y + k - u.i0);
    }
    dst[id.y * u.w + id.x] = sum;
}

// i2 = 1 Normal, 2 Multiply.
fn shade_channel(c: f32, k: f32, t: f32) -> f32 {
    if (u.i2 == 2) {
        return c * (1.0 - t * (1.0 - k));
    }
    return c + t * (k - c);
}

// Bilinear shift + straight-RGB composite, in place on output-aligned RGBA.
// i0/i1 = sample base, f0..f3 = bilinear weights (x0, x1, y0, y1),
// f4 = opacity, f5..f7 = shadow colour. Writes (r', g', b', t).
@compute @workgroup_size(16, 16, 1)
fn composite(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= u.out_w || id.y >= u.out_h) {
        return;
    }
    let x0 = i32(id.x) + u.i0;
    let y0 = i32(id.y) + u.i1;
    let m = (at0(x0, y0) * u.f0 + at0(x0 + 1, y0) * u.f1) * u.f2
        + (at0(x0, y0 + 1) * u.f0 + at0(x0 + 1, y0 + 1) * u.f1) * u.f3;
    let t = u.f4 * clamp(m, 0.0, 1.0);
    let b = (id.y * u.out_w + id.x) * 4u;
    dst[b] = shade_channel(dst[b], u.f5, t);
    dst[b + 1u] = shade_channel(dst[b + 1u], u.f6, t);
    dst[b + 2u] = shade_channel(dst[b + 2u], u.f7, t);
    dst[b + 3u] = t;
}
