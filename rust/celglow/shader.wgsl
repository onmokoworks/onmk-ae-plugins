struct Band {
    inner_px: f32,
    outer_px: f32,
    opacity: f32,
    edge_soft_px: f32,
    color: vec4<f32>,
};

struct Params {
    out_w: u32,
    out_h: u32,
    in_w: u32,
    in_h: u32,
    ox0: i32,
    oy0: i32,
    plane_left: i32,
    plane_top: i32,
    copies: u32,
    band_count: u32,
    distribution: u32,
    global_blend: u32,
    preserve_alpha: u32,
    view: u32,
    use_jfa: u32,
    jfa_step: u32,
    read_from_a: u32,
    source_mode: u32,
    _pad0_x: u32,
    _pad0_y: u32,
    _pad0_z: u32,
    amount: f32,
    max_d: f32,
    rotation_step: f32,
    scale_step: f32,
    ring_radius: f32,
    line_x: f32,
    line_y: f32,
    copy_opacity_step: f32,
    center_x: f32,
    center_y: f32,
    luma_t: f32,
    luma_s: f32,
    color_tolerance: f32,
    _pad1_x: f32,
    _pad1_y: f32,
    _pad1_z: f32,
    _pad1_w: f32,
    _pad1_u: f32,
    _pad1_v: f32,
    source_color: vec4<f32>,
    bands: array<Band, 8>,
};

@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var<storage, read> source: array<f32>;
@group(0) @binding(2) var<storage, read> dist_buf: array<f32>;
@group(0) @binding(3) var<storage, read_write> soft_buf: array<f32>;
@group(0) @binding(4) var<storage, read_write> output: array<f32>;
@group(0) @binding(5) var<storage, read_write> seed_a: array<vec4<f32>>;
@group(0) @binding(6) var<storage, read_write> seed_b: array<vec4<f32>>;

fn smoothstep_local(edge0: f32, edge1: f32, x: f32) -> f32 {
    if (edge1 <= edge0) {
        return select(0.0, 1.0, x >= edge1);
    }
    let t = clamp((x - edge0) / (edge1 - edge0), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

fn band_weight(d_px: f32, b: Band, glow_inner_px: f32, glow_outer_px: f32) -> f32 {
    let inner = b.inner_px;
    let outer = b.outer_px;
    if (outer <= inner + 0.0001 || d_px < inner || d_px > outer) {
        return 0.0;
    }
    let es = max(b.edge_soft_px, 0.0);
    var aa_in = 1.0;
    var aa_out = 1.0;
    if (es > 0.0001 && abs(inner - glow_inner_px) <= 0.001) {
        aa_in = smoothstep_local(glow_inner_px, glow_inner_px + es, d_px);
    }
    if (es > 0.0001 && abs(outer - glow_outer_px) <= 0.001) {
        aa_out = 1.0 - smoothstep_local(glow_outer_px - es, glow_outer_px, d_px);
    }
    return b.opacity * aa_in * aa_out;
}

fn sample_1ch(buf_idx: i32, x: f32, y: f32, fallback: f32) -> f32 {
    let ix = i32(round(x));
    let iy = i32(round(y));
    if (ix < 0 || iy < 0 || ix >= i32(p.in_w) || iy >= i32(p.in_h)) {
        return fallback;
    }
    let idx = u32(iy) * p.in_w + u32(ix);
    if (buf_idx == 0) {
        return dist_buf[idx];
    }
    return soft_buf[idx];
}

fn seed_at(idx: u32) -> vec4<f32> {
    if (p.read_from_a != 0u) {
        return seed_a[idx];
    }
    return seed_b[idx];
}

fn seed_write(idx: u32, value: vec4<f32>) {
    if (p.read_from_a != 0u) {
        seed_b[idx] = value;
    } else {
        seed_a[idx] = value;
    }
}

fn distance_from_seed(x: f32, y: f32) -> f32 {
    let ix = i32(round(x));
    let iy = i32(round(y));
    if (ix < 0 || iy < 0 || ix >= i32(p.in_w) || iy >= i32(p.in_h)) {
        return p.max_d;
    }
    let s = seed_at(u32(iy) * p.in_w + u32(ix));
    if (s.z < 0.5) {
        return p.max_d;
    }
    let dx = x - s.x;
    let dy = y - s.y;
    return sqrt(dx * dx + dy * dy);
}

fn copy_transform(x: f32, y: f32, idx: u32) -> vec2<f32> {
    let i = f32(idx);
    let scale = pow(max(p.scale_step, 0.01), i);
    var ox = 0.0;
    var oy = 0.0;
    var theta = p.rotation_step * i;
    if (p.distribution == 2u) {
        let a = 6.28318530718 * i / max(f32(p.copies), 1.0);
        ox = cos(a) * p.ring_radius;
        oy = sin(a) * p.ring_radius;
        theta = theta + a;
    } else if (p.distribution == 3u) {
        ox = p.line_x * i;
        oy = p.line_y * i;
    }
    let px = x - (p.center_x + ox);
    let py = y - (p.center_y + oy);
    let c = cos(theta);
    let s = sin(theta);
    return vec2<f32>(
        (px * c + py * s) / scale + p.center_x,
        (-px * s + py * c) / scale + p.center_y,
    );
}

fn combine_rgb(src: vec3<f32>, glow: vec3<f32>, ga: f32) -> vec3<f32> {
    if (p.global_blend == 2u) {
        return vec3<f32>(
            1.0 - max(1.0 - src.r, 0.0) * (1.0 - clamp(glow.r, 0.0, 1.0)),
            1.0 - max(1.0 - src.g, 0.0) * (1.0 - clamp(glow.g, 0.0, 1.0)),
            1.0 - max(1.0 - src.b, 0.0) * (1.0 - clamp(glow.b, 0.0, 1.0)),
        );
    }
    if (p.global_blend == 3u) {
        let a = clamp(ga, 0.0, 1.0);
        return src * (1.0 - a) + glow * a;
    }
    return src + glow;
}

fn source_mask_at(idx: u32) -> f32 {
    let si = idx * 4u;
    let a = source[si];
    let r = source[si + 1u];
    let g = source[si + 2u];
    let b = source[si + 3u];
    let y709 = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    var luma_m = 0.0;
    if (p.luma_s <= 0.0) {
        luma_m = select(0.0, 1.0, y709 >= p.luma_t);
    } else {
        let t0 = p.luma_t - p.luma_s;
        let t1 = p.luma_t + p.luma_s;
        luma_m = clamp((y709 - t0) / max(t1 - t0, 0.0001), 0.0, 1.0);
    }

    let dr = r - p.source_color.r;
    let dg = g - p.source_color.g;
    let db = b - p.source_color.b;
    let color_dist = sqrt(dr * dr + dg * dg + db * db) / sqrt(3.0);
    let color_soft = max(0.10, p.color_tolerance * 0.25);
    let color_m = 1.0 - smoothstep_local(p.color_tolerance, p.color_tolerance + color_soft, color_dist);

    var m = luma_m;
    if (p.source_mode == 2u) {
        m = color_m;
    } else if (p.source_mode == 3u) {
        m = max(luma_m, color_m);
    } else if (p.source_mode == 4u) {
        m = min(luma_m, color_m);
    }
    return m * a;
}

@compute @workgroup_size(16, 16)
fn init_mask(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= p.in_w || id.y >= p.in_h) {
        return;
    }
    let idx = id.y * p.in_w + id.x;
    let m = source_mask_at(idx);
    soft_buf[idx] = m;
    if (m > 0.0) {
        seed_a[idx] = vec4<f32>(f32(id.x), f32(id.y), 1.0, 0.0);
    } else {
        seed_a[idx] = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }
    seed_b[idx] = vec4<f32>(0.0, 0.0, 0.0, 0.0);
}

@compute @workgroup_size(16, 16)
fn jfa_pass(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= p.in_w || id.y >= p.in_h) {
        return;
    }
    let idx = id.y * p.in_w + id.x;
    let step = i32(p.jfa_step);
    var best = seed_at(idx);
    var best_d2 = 3.402823e38;
    if (best.z >= 0.5) {
        let dx = f32(id.x) - best.x;
        let dy = f32(id.y) - best.y;
        best_d2 = dx * dx + dy * dy;
    }
    for (var oy = -1; oy <= 1; oy = oy + 1) {
        for (var ox = -1; ox <= 1; ox = ox + 1) {
            let sx = i32(id.x) + ox * step;
            let sy = i32(id.y) + oy * step;
            if (sx < 0 || sy < 0 || sx >= i32(p.in_w) || sy >= i32(p.in_h)) {
                continue;
            }
            let cand = seed_at(u32(sy) * p.in_w + u32(sx));
            if (cand.z < 0.5) {
                continue;
            }
            let dx = f32(id.x) - cand.x;
            let dy = f32(id.y) - cand.y;
            let d2 = dx * dx + dy * dy;
            if (d2 < best_d2) {
                best_d2 = d2;
                best = cand;
            }
        }
    }
    seed_write(idx, best);
}

@compute @workgroup_size(16, 16)
fn render(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= p.out_w || id.y >= p.out_h) {
        return;
    }
    let di = (id.y * p.out_w + id.x) * 4u;
    let layer_x = p.ox0 + i32(id.x);
    let layer_y = p.oy0 + i32(id.y);
    let ix = layer_x - p.plane_left;
    let iy = layer_y - p.plane_top;

    var in_a = 0.0;
    var in_r = 0.0;
    var in_g = 0.0;
    var in_b = 0.0;
    var d_px = p.max_d;
    var sm = 0.0;
    if (ix >= 0 && iy >= 0 && ix < i32(p.in_w) && iy < i32(p.in_h)) {
        let ii = u32(iy) * p.in_w + u32(ix);
        let si = ii * 4u;
        in_a = source[si];
        in_r = source[si + 1u];
        in_g = source[si + 2u];
        in_b = source[si + 3u];
        if (p.use_jfa != 0u) {
            let s = seed_at(ii);
            if (s.z >= 0.5) {
                let dx = f32(ix) - s.x;
                let dy = f32(iy) - s.y;
                d_px = sqrt(dx * dx + dy * dy);
            }
        } else {
            d_px = dist_buf[ii];
        }
        sm = soft_buf[ii];
    }

    var gr = 0.0;
    var gg = 0.0;
    var gb = 0.0;
    var ga = 0.0;
    let glow_inner_px = p.bands[0].inner_px;
    let glow_outer_px = p.bands[p.band_count - 1u].outer_px;
    for (var c = 0u; c < p.copies; c = c + 1u) {
        let q = copy_transform(f32(ix), f32(iy), c);
        var cd = p.max_d;
        if (p.use_jfa != 0u) {
            cd = distance_from_seed(q.x, q.y);
        } else {
            cd = sample_1ch(0, q.x, q.y, p.max_d);
        }
        let copy_opacity = clamp(1.0 + p.copy_opacity_step * f32(c), 0.0, 2.0);
        for (var b = 0u; b < p.band_count; b = b + 1u) {
            let w = band_weight(cd, p.bands[b], glow_inner_px, glow_outer_px) * copy_opacity;
            if (w > 0.0) {
                gr = gr + p.bands[b].color.r * w;
                gg = gg + p.bands[b].color.g * w;
                gb = gb + p.bands[b].color.b * w;
                ga = ga + w;
            }
        }
    }
    gr = gr * p.amount;
    gg = gg * p.amount;
    gb = gb * p.amount;
    ga = ga * p.amount;

    var oa = in_a;
    var orgb = vec3<f32>(in_r, in_g, in_b);
    if (p.view == 2u) {
        oa = ga;
        orgb = vec3<f32>(gr, gg, gb);
    } else if (p.view == 4u) {
        let u = clamp(d_px / p.max_d, 0.0, 1.0);
        orgb = vec3<f32>(u, u, u);
    } else if (p.view == 5u) {
        orgb = vec3<f32>(sm, sm, sm);
    } else {
        orgb = combine_rgb(orgb, vec3<f32>(gr, gg, gb), ga);
        if (p.preserve_alpha == 0u) {
            oa = in_a + (1.0 - in_a) * clamp(ga, 0.0, 1.0);
        }
    }

    output[di] = oa;
    output[di + 1u] = orgb.r;
    output[di + 2u] = orgb.g;
    output[di + 3u] = orgb.b;
}
