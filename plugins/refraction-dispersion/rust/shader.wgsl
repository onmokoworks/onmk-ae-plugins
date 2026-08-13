struct Params {
    size: vec4<u32>,
    mode: vec4<u32>,
    optical0: vec4<f32>,
    optical1: vec4<f32>,
    optical2: vec4<f32>,
    color: vec4<f32>,
    light: vec4<f32>,
    half_vec: vec4<f32>,
    // x = height_source, y = height_invert (0/1), z = coverage_source
    mode2: vec4<u32>,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> source: array<u32>;
@group(0) @binding(2) var<storage, read> mask_layer: array<u32>;
@group(0) @binding(3) var<storage, read> background: array<u32>;
@group(0) @binding(4) var<storage, read_write> height_a: array<f32>;
@group(0) @binding(5) var<storage, read_write> height_b: array<f32>;
@group(0) @binding(6) var<storage, read_write> mask_a: array<f32>;
@group(0) @binding(7) var<storage, read_write> comp_mask: array<f32>;
@group(0) @binding(8) var<storage, read_write> tmp_plane: array<f32>;
@group(0) @binding(9) var<storage, read_write> rgb_a: array<vec4<f32>>;
@group(0) @binding(10) var<storage, read_write> rgb_b: array<vec4<f32>>;
@group(0) @binding(11) var<storage, read_write> output_img: array<u32>;

fn idx(x: u32, y: u32) -> u32 {
    return y * params.size.x + x;
}

fn inside(id: vec3<u32>) -> bool {
    return id.x < params.size.x && id.y < params.size.y;
}

fn unpack_rgba(p: u32) -> vec4<f32> {
    let r = f32((p >> 24u) & 0xFFu);
    let g = f32((p >> 16u) & 0xFFu);
    let b = f32((p >> 8u) & 0xFFu);
    let a = f32(p & 0xFFu);
    return vec4<f32>(r, g, b, a);
}

fn pack_rgba(c: vec4<f32>) -> u32 {
    let r = u32(clamp(c.r, 0.0, 255.0));
    let g = u32(clamp(c.g, 0.0, 255.0));
    let b = u32(clamp(c.b, 0.0, 255.0));
    let a = u32(clamp(c.a, 0.0, 255.0));
    return (r << 24u) | (g << 16u) | (b << 8u) | a;
}

// Edge handling (must mirror src/refract.rs):
//   mode.w == 1 Mirror, == 2 Clamp, == 3 Clamp+Fade.
const EDGE_MIRROR: u32 = 1u;
const EDGE_FADE: u32 = 3u;
const EDGE_FADE_RAMP: f32 = 24.0;

// Plain clamped bilinear sample of the background (no fallback).
fn sample_clamped(fx_in: f32, fy_in: f32) -> vec3<f32> {
    let max_x = max(f32(params.size.x) - 1.0, 0.0);
    let max_y = max(f32(params.size.y) - 1.0, 0.0);
    let fx = clamp(fx_in, 0.0, max_x);
    let fy = clamp(fy_in, 0.0, max_y);
    let x0 = u32(fx);
    let y0 = u32(fy);
    let x1 = min(x0 + 1u, params.size.x - 1u);
    let y1 = min(y0 + 1u, params.size.y - 1u);
    let dx = fx - f32(x0);
    let dy = fy - f32(y0);

    let c00 = unpack_rgba(background[idx(x0, y0)]).rgb;
    let c10 = unpack_rgba(background[idx(x1, y0)]).rgb;
    let c01 = unpack_rgba(background[idx(x0, y1)]).rgb;
    let c11 = unpack_rgba(background[idx(x1, y1)]).rgb;
    let cx0 = c00 * (1.0 - dx) + c10 * dx;
    let cx1 = c01 * (1.0 - dx) + c11 * dx;
    return cx0 * (1.0 - dy) + cx1 * dy;
}

fn reflect_coord(v: f32, m: f32) -> f32 {
    if m <= 0.0 {
        return 0.0;
    }
    let period = 2.0 * m;
    var r = v - period * floor(v / period);
    if r > m {
        r = period - r;
    }
    return r;
}

fn outside_dist(fx: f32, fy: f32) -> f32 {
    let mx = max(f32(params.size.x) - 1.0, 0.0);
    let my = max(f32(params.size.y) - 1.0, 0.0);
    var dx = 0.0;
    if fx < 0.0 {
        dx = -fx;
    } else if fx > mx {
        dx = fx - mx;
    }
    var dy = 0.0;
    if fy < 0.0 {
        dy = -fy;
    } else if fy > my {
        dy = fy - my;
    }
    return max(dx, dy);
}

// Mirror or Clamp sample (both always return real data).
fn sample_edge(fx: f32, fy: f32) -> vec3<f32> {
    if params.mode.w == EDGE_MIRROR {
        let mx = max(f32(params.size.x) - 1.0, 0.0);
        let my = max(f32(params.size.y) - 1.0, 0.0);
        return sample_clamped(reflect_coord(fx, mx), reflect_coord(fy, my));
    }
    return sample_clamped(fx, fy);
}

fn refract_vec(i: vec3<f32>, n: vec3<f32>, eta: f32) -> vec3<f32> {
    let n_dot_i = dot(n, i);
    let k = 1.0 - eta * eta * (1.0 - n_dot_i * n_dot_i);
    if k < 0.0 {
        return vec3<f32>(0.0);
    }
    let s = eta * n_dot_i + sqrt(k);
    return eta * i - s * n;
}

fn blur_plane_x_value(x: u32, y: u32, source_id: u32) -> f32 {
    let r = i32(params.mode.y);
    let diam = f32(2 * r + 1);
    var sum = 0.0;
    var i = -r;
    loop {
        if i > r {
            break;
        }
        let sx = u32(clamp(i32(x) + i, 0, i32(params.size.x) - 1));
        let p = idx(sx, y);
        if source_id == 0u {
            sum += height_a[p];
        } else if source_id == 1u {
            sum += mask_a[p];
        } else {
            sum += rgb_a[p].r;
        }
        i = i + 1;
    }
    return sum / diam;
}

fn blur_plane_y_value(x: u32, y: u32, source_id: u32) -> f32 {
    let r = i32(params.mode.y);
    let diam = f32(2 * r + 1);
    var sum = 0.0;
    var i = -r;
    loop {
        if i > r {
            break;
        }
        let sy = u32(clamp(i32(y) + i, 0, i32(params.size.y) - 1));
        let p = idx(x, sy);
        if source_id == 0u {
            sum += height_b[p];
        } else if source_id == 1u {
            sum += tmp_plane[p];
        } else {
            sum += rgb_b[p].r;
        }
        i = i + 1;
    }
    return sum / diam;
}

fn blur_rgb_x_value(x: u32, y: u32) -> vec4<f32> {
    let r = i32(params.mode.y);
    let diam = f32(2 * r + 1);
    var sum = vec4<f32>(0.0);
    var i = -r;
    loop {
        if i > r {
            break;
        }
        let sx = u32(clamp(i32(x) + i, 0, i32(params.size.x) - 1));
        sum += rgb_a[idx(sx, y)];
        i = i + 1;
    }
    return sum / diam;
}

fn blur_rgb_y_value(x: u32, y: u32) -> vec4<f32> {
    let r = i32(params.mode.y);
    let diam = f32(2 * r + 1);
    var sum = vec4<f32>(0.0);
    var i = -r;
    loop {
        if i > r {
            break;
        }
        let sy = u32(clamp(i32(y) + i, 0, i32(params.size.y) - 1));
        sum += rgb_b[idx(x, sy)];
        i = i + 1;
    }
    return sum / diam;
}

fn ior_slot(slot: u32) -> f32 {
    if slot == 0u {
        return params.optical0.x;
    }
    if slot == 1u {
        return params.optical0.x + (params.optical0.y - params.optical0.x) * 0.5;
    }
    if slot == 2u {
        return params.optical0.y;
    }
    if slot == 3u {
        return params.optical0.y + (params.optical0.z - params.optical0.y) * 0.5;
    }
    if slot == 4u {
        return params.optical0.z;
    }
    return params.optical0.z + (params.optical0.z - params.optical0.x) * 0.25;
}

@compute @workgroup_size(16, 16, 1)
fn init_planes(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) {
        return;
    }
    let p = idx(id.x, id.y);
    let c = unpack_rgba(mask_layer[p]);
    let a = c.a / 255.0;
    let r = c.r / 255.0;
    let g = c.g / 255.0;
    let b = c.b / 255.0;
    let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;

    // Height source (see lib.rs popup order).
    let hs = params.mode2.x;
    var src_val = luma * a;
    if hs == 2u {
        src_val = luma;
    } else if hs == 3u {
        src_val = a;
    } else if hs == 4u {
        src_val = r;
    } else if hs == 5u {
        src_val = g;
    } else if hs == 6u {
        src_val = b;
    } else if hs == 7u {
        src_val = max(r, max(g, b));
    }
    var hgt = src_val;
    if params.mode2.y == 1u {
        hgt = 1.0 - src_val;
    }
    height_a[p] = hgt;

    // Coverage source (uses the pre-invert source value for "Same as Height").
    let cs = params.mode2.z;
    var cov = a;
    if cs == 2u {
        cov = luma;
    } else if cs == 3u {
        cov = 1.0;
    } else if cs == 4u {
        cov = src_val;
    }
    mask_a[p] = clamp(cov, 0.0, 1.0);
}

@compute @workgroup_size(16, 16, 1)
fn copy_mask(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) {
        return;
    }
    let p = idx(id.x, id.y);
    comp_mask[p] = mask_a[p];
}

@compute @workgroup_size(16, 16, 1)
fn blur_height_h(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) {
        return;
    }
    height_b[idx(id.x, id.y)] = blur_plane_x_value(id.x, id.y, 0u);
}

@compute @workgroup_size(16, 16, 1)
fn blur_height_v(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) {
        return;
    }
    height_a[idx(id.x, id.y)] = blur_plane_y_value(id.x, id.y, 0u);
}

@compute @workgroup_size(16, 16, 1)
fn blur_mask_h(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) {
        return;
    }
    tmp_plane[idx(id.x, id.y)] = blur_plane_x_value(id.x, id.y, 1u);
}

@compute @workgroup_size(16, 16, 1)
fn blur_mask_v(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) {
        return;
    }
    comp_mask[idx(id.x, id.y)] = blur_plane_y_value(id.x, id.y, 1u);
}

@compute @workgroup_size(16, 16, 1)
fn render_main(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) {
        return;
    }
    let x = id.x;
    let y = id.y;
    let p = idx(x, y);
    let src = unpack_rgba(source[p]);
    let m = clamp(comp_mask[p], 0.0, 1.0);

    if m <= 0.0001 {
        output_img[p] = source[p];
        return;
    }

    let x0 = u32(max(i32(x) - 1, 0));
    let x1 = min(x + 1u, params.size.x - 1u);
    let y0 = u32(max(i32(y) - 1, 0));
    let y1 = min(y + 1u, params.size.y - 1u);
    let dhdx = (height_a[idx(x1, y)] - height_a[idx(x0, y)]) * 0.5;
    let dhdy = (height_a[idx(x, y1)] - height_a[idx(x, y0)]) * 0.5;
    let k = max(params.optical2.z, 0.0) * max(params.optical2.w, 1.0) * 1.5;
    let n = normalize(vec3<f32>(-dhdx * k, -dhdy * k, 1.0));
    let eye = vec3<f32>(0.0, 0.0, -1.0);
    let samples_f = f32(max(params.size.z, 1u));

    var acc = vec3<f32>(0.0);
    if params.mode.z == 2u {
        let rv = refract_vec(eye, n, 1.0 / max(params.color.w, 1.0));
        var sum = vec3<f32>(0.0);
        for (var i = 0u; i < params.size.z; i = i + 1u) {
            let sx = f32(x) + rv.x * params.optical0.w;
            let sy = f32(y) + rv.y * params.optical0.w;
            if params.mode.w == EDGE_FADE {
                let t = clamp(outside_dist(sx, sy) / EDGE_FADE_RAMP, 0.0, 1.0);
                sum += mix(sample_clamped(sx, sy), src.rgb, t);
            } else {
                sum += sample_edge(sx, sy);
            }
        }
        acc = sum / samples_f;
    } else if params.size.w == 0u {
        let rv_r = refract_vec(eye, n, 1.0 / max(params.optical0.x, 1.0));
        let rv_g = refract_vec(eye, n, 1.0 / max(params.optical0.y, 1.0));
        let rv_b = refract_vec(eye, n, 1.0 / max(params.optical0.z, 1.0));
        var ar = 0.0;
        var ag = 0.0;
        var ab = 0.0;
        for (var i = 0u; i < params.size.z; i = i + 1u) {
            // Symmetric / equal-width dispersion (mirrors src/refract.rs): center
            // the blur around the base offset and separate colors symmetrically
            // around green so red no longer pools denser than blue.
            let slide = (f32(i) + 0.5) / samples_f - 0.5; // [-0.5, 0.5)
            let blur_x = slide * 0.2 * params.optical1.x;
            let blur_y = slide * 0.2 * params.optical1.y;
            let sr_x = params.optical0.w * (1.0 - 0.1 * params.optical1.x + blur_x);
            let sg_x = params.optical0.w * (1.0 + blur_x);
            let sb_x = params.optical0.w * (1.0 + 0.1 * params.optical1.x + blur_x);
            let sr_y = params.optical0.w * (1.0 - 0.1 * params.optical1.y + blur_y);
            let sg_y = params.optical0.w * (1.0 + blur_y);
            let sb_y = params.optical0.w * (1.0 + 0.1 * params.optical1.y + blur_y);
            let sx_r = f32(x) + rv_r.x * sr_x;
            let sy_r = f32(y) + rv_r.y * sr_y;
            let sx_g = f32(x) + rv_g.x * sg_x;
            let sy_g = f32(y) + rv_g.y * sg_y;
            let sx_b = f32(x) + rv_b.x * sb_x;
            let sy_b = f32(y) + rv_b.y * sb_y;
            if params.mode.w == EDGE_FADE {
                // Shared fade factor from the tap reaching furthest outside so
                // the channels stay coupled (no seam) and the stretched edge
                // dissolves back into the source image.
                let d = max(outside_dist(sx_r, sy_r), max(outside_dist(sx_g, sy_g), outside_dist(sx_b, sy_b)));
                let t = clamp(d / EDGE_FADE_RAMP, 0.0, 1.0);
                ar += mix(sample_clamped(sx_r, sy_r).r, src.r, t);
                ag += mix(sample_clamped(sx_g, sy_g).g, src.g, t);
                ab += mix(sample_clamped(sx_b, sy_b).b, src.b, t);
            } else {
                ar += sample_edge(sx_r, sy_r).r;
                ag += sample_edge(sx_g, sy_g).g;
                ab += sample_edge(sx_b, sy_b).b;
            }
        }
        let inv_s = 1.0 / samples_f;
        acc.r = ar * inv_s;
        acc.g = ag * inv_s;
        acc.b = ab * inv_s;
    } else {
        // Precompute the refraction vector for each wavelength slot.
        var rvs = array<vec2<f32>, 6>(
            vec2<f32>(0.0), vec2<f32>(0.0), vec2<f32>(0.0),
            vec2<f32>(0.0), vec2<f32>(0.0), vec2<f32>(0.0),
        );
        for (var slot = 0u; slot < 6u; slot = slot + 1u) {
            rvs[slot] = refract_vec(eye, n, 1.0 / max(ior_slot(slot), 1.0)).xy;
        }
        // Accumulate per slot. In Clamp+Fade a single shared fade factor (from
        // the slot reaching furthest outside) keeps the wavelengths in lock-step
        // so no colored seam forms.
        var acc_s = array<vec3<f32>, 6>(
            vec3<f32>(0.0), vec3<f32>(0.0), vec3<f32>(0.0),
            vec3<f32>(0.0), vec3<f32>(0.0), vec3<f32>(0.0),
        );
        for (var i = 0u; i < params.size.z; i = i + 1u) {
            let slide = (f32(i) + 0.5) / samples_f - 0.5; // [-0.5, 0.5)
            var pos = array<vec2<f32>, 6>(
                vec2<f32>(0.0), vec2<f32>(0.0), vec2<f32>(0.0),
                vec2<f32>(0.0), vec2<f32>(0.0), vec2<f32>(0.0),
            );
            var d = 0.0;
            for (var slot = 0u; slot < 6u; slot = slot + 1u) {
                // Symmetric separation (−1..+1 around center) + shared blur width.
                let sep = f32(slot) / 2.5 - 1.0;
                let sx = params.optical0.w * (1.0 + sep * 0.15 * params.optical1.x + slide * 0.2 * params.optical1.x);
                let sy = params.optical0.w * (1.0 + sep * 0.15 * params.optical1.y + slide * 0.2 * params.optical1.y);
                let px = f32(x) + rvs[slot].x * sx;
                let py = f32(y) + rvs[slot].y * sy;
                pos[slot] = vec2<f32>(px, py);
                d = max(d, outside_dist(px, py));
            }
            if params.mode.w == EDGE_FADE {
                let t = clamp(d / EDGE_FADE_RAMP, 0.0, 1.0);
                for (var slot = 0u; slot < 6u; slot = slot + 1u) {
                    acc_s[slot] = acc_s[slot] + mix(sample_clamped(pos[slot].x, pos[slot].y), src.rgb, t);
                }
            } else {
                for (var slot = 0u; slot < 6u; slot = slot + 1u) {
                    acc_s[slot] = acc_s[slot] + sample_edge(pos[slot].x, pos[slot].y);
                }
            }
        }
        let inv_s6 = 1.0 / samples_f;
        var ch = array<f32, 6>(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        for (var slot = 0u; slot < 6u; slot = slot + 1u) {
            let s = acc_s[slot] * inv_s6;
            let sr = s.r;
            let sg = s.g;
            let sb = s.b;
            if slot == 0u {
                ch[slot] = sr * 0.5;
            } else if slot == 1u {
                ch[slot] = (2.0 * sr + 2.0 * sg - sb) / 6.0;
            } else if slot == 2u {
                ch[slot] = sg * 0.5;
            } else if slot == 3u {
                ch[slot] = (2.0 * sg + 2.0 * sb - sr) / 6.0;
            } else if slot == 4u {
                ch[slot] = sb * 0.5;
            } else {
                ch[slot] = (2.0 * sb + 2.0 * sr - sg) / 6.0;
            }
        }
        let r_c = ch[0];
        let y_c = ch[1];
        let g_c = ch[2];
        let c_c = ch[3];
        let b_c = ch[4];
        let v_c = ch[5];
        acc.r = r_c + (2.0 * v_c + 2.0 * y_c - c_c) / 3.0;
        acc.g = g_c + (2.0 * y_c + 2.0 * c_c - v_c) / 3.0;
        acc.b = b_c + (2.0 * c_c + 2.0 * v_c - y_c) / 3.0;
    }

    let luma = 0.2125 * acc.r + 0.7154 * acc.g + 0.0721 * acc.b;
    acc = vec3<f32>(luma) + (acc - vec3<f32>(luma)) * params.optical2.y;

    let light = params.light.xyz;
    let half_vec = params.half_vec.xyz;
    let ndoth = max(dot(n, half_vec), 0.0);
    let k_spec = pow(ndoth, max(params.optical1.w, 1.0));
    let ndotl = max(dot(n, light), 0.0);
    let k_diff = ndotl * params.optical2.x;
    let fresnel_factor = abs(dot(eye, n));
    let fresnel = pow(max(1.0 - fresnel_factor, 0.0), max(params.optical1.z, 0.0));
    let diff_mult = 1.0 + k_diff;
    let spec_add = k_spec * fresnel * 200.0;
    acc = acc * diff_mult + vec3<f32>(spec_add);

    let contrast_factor = max(1.0 + params.color.z, 0.0);
    acc = (acc - vec3<f32>(128.0)) * contrast_factor
        + vec3<f32>(128.0 + params.color.y * 255.0);

    let comp = src.rgb * (1.0 - m) + acc * m;
    let fin = src.rgb * (1.0 - params.color.x) + comp * params.color.x;
    output_img[p] = pack_rgba(vec4<f32>(fin, src.a));
}

@compute @workgroup_size(16, 16, 1)
fn init_rgb_blur(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) {
        return;
    }
    let p = idx(id.x, id.y);
    rgb_a[p] = unpack_rgba(output_img[p]);
}

@compute @workgroup_size(16, 16, 1)
fn blur_rgb_h(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) {
        return;
    }
    rgb_b[idx(id.x, id.y)] = blur_rgb_x_value(id.x, id.y);
}

@compute @workgroup_size(16, 16, 1)
fn blur_rgb_v(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) {
        return;
    }
    rgb_a[idx(id.x, id.y)] = blur_rgb_y_value(id.x, id.y);
}

@compute @workgroup_size(16, 16, 1)
fn apply_rgb_blur(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) {
        return;
    }
    let p = idx(id.x, id.y);
    let m = clamp(comp_mask[p], 0.0, 1.0);
    if m <= 0.0001 {
        return;
    }
    let cur = unpack_rgba(output_img[p]);
    let blurred = rgb_a[p];
    let rgb = cur.rgb * (1.0 - m) + blurred.rgb * m;
    output_img[p] = pack_rgba(vec4<f32>(rgb, cur.a));
}

@compute @workgroup_size(16, 16, 1)
fn apply_output_mode(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) {
        return;
    }
    let p = idx(id.x, id.y);
    let src = unpack_rgba(source[p]);
    if params.mode.x == 3u {
        let v = clamp(comp_mask[p], 0.0, 1.0) * 255.0;
        output_img[p] = pack_rgba(vec4<f32>(v, v, v, src.a));
    } else if params.mode.x == 4u {
        let out = unpack_rgba(output_img[p]);
        let d = abs(out.rgb - src.rgb);
        output_img[p] = pack_rgba(vec4<f32>(d, src.a));
    }
}
