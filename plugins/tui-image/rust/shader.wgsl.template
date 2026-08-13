struct Params {
    width: u32,
    height: u32,
    cell_w: u32,
    cell_h: u32,
    preset: u32,
    color_mode: u32,
    contrast: f32,
    gamma: f32,
    edge_boost: f32,
    invert: u32,
    source_mix: f32,
    _pad0: f32,
    foreground: vec4<f32>,
    background: vec4<f32>,
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> source_pixels: array<u32>;
@group(0) @binding(2) var<storage, read_write> output_pixels: array<u32>;

fn unpack_rgba8(px: u32) -> vec4<f32> {
    let r = f32((px >> 24u) & 255u) / 255.0;
    let g = f32((px >> 16u) & 255u) / 255.0;
    let b = f32((px >> 8u) & 255u) / 255.0;
    let a = f32(px & 255u) / 255.0;
    return vec4<f32>(r, g, b, a);
}

fn pack_rgba8(c: vec4<f32>) -> u32 {
    let q = vec4<u32>(clamp(c, vec4<f32>(0.0), vec4<f32>(1.0)) * 255.0);
    return (q.r << 24u) | (q.g << 16u) | (q.b << 8u) | q.a;
}

fn luma(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

fn band_fill(local_y: u32, cell_h: u32, coverage: f32) -> bool {
    return (f32(cell_h) - f32(local_y)) / f32(cell_h) <= coverage;
}

fn near_line(v: u32, target: u32, thickness: u32) -> bool {
    let lo = select(0u, target - thickness, target > thickness);
    let hi = target + thickness;
    return v >= lo && v <= hi;
}

fn ascii_fill(level: u32, lx: u32, ly: u32, cw: u32, ch: u32) -> bool {
    if (level == 0u) {
        return false;
    }
    let cx = cw / 2u;
    let cy = ch / 2u;
    let qx1 = max(cw / 3u, 1u);
    let qx2 = max((cw * 2u) / 3u, 1u);
    let qy1 = max(ch / 3u, 1u);
    let qy2 = max((ch * 2u) / 3u, 1u);
    let t = max(min(cw, ch) / 7u, 1u);

    if (level == 1u) {
        return near_line(lx, cx, t) && near_line(ly, qy2, t);
    }
    if (level == 2u) {
        return near_line(lx, cx, t) && (near_line(ly, qy1, t) || near_line(ly, qy2, t));
    }
    if (level == 3u) {
        return near_line(ly, cy, t) && lx > t && lx + t < cw;
    }
    if (level == 4u) {
        return (near_line(ly, qy1, t) || near_line(ly, qy2, t)) && lx > t && lx + t < cw;
    }
    if (level == 5u) {
        return near_line(ly, cy, t) || near_line(lx, cx, t);
    }
    if (level == 6u) {
        let diag_a = abs(f32(lx) / max(f32(cw - 1u), 1.0) - f32(ly) / max(f32(ch - 1u), 1.0)) < 0.18;
        let diag_b = abs(f32(lx) / max(f32(cw - 1u), 1.0) + f32(ly) / max(f32(ch - 1u), 1.0) - 1.0) < 0.18;
        return near_line(ly, cy, t) || near_line(lx, cx, t) || diag_a || diag_b;
    }
    if (level == 7u) {
        return near_line(lx, qx1, t) || near_line(lx, qx2, t) || near_line(ly, qy1, t) || near_line(ly, qy2, t);
    }
    if (level == 8u) {
        return ((lx + ly) % 3u) != 0u;
    }
    return true;
}

fn braille_fill(level: u32, lx: u32, ly: u32, cw: u32, ch: u32) -> bool {
    if (level == 0u) {
        return false;
    }
    let col = min((lx * 2u) / max(cw, 1u), 1u);
    let row = min((ly * 4u) / max(ch, 1u), 3u);
    let dot_index = row * 2u + col;
    if (dot_index >= level) {
        return false;
    }

    let dot_cx = (f32(col) + 0.5) * f32(cw) * 0.5;
    let dot_cy = (f32(row) + 0.5) * f32(ch) * 0.25;
    let rx = max(f32(cw) * 0.18, 1.0);
    let ry = max(f32(ch) * 0.10, 1.0);
    let dx = (f32(lx) + 0.5 - dot_cx) / rx;
    let dy = (f32(ly) + 0.5 - dot_cy) / ry;
    return dx * dx + dy * dy <= 1.0;
}

@compute @workgroup_size(16, 16, 1)
fn tui_pass(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= params.width || gid.y >= params.height) {
        return;
    }

    let idx = gid.y * params.width + gid.x;
    let src = unpack_rgba8(source_pixels[idx]);

    let cell_w = max(params.cell_w, 1u);
    let cell_h = max(params.cell_h, 1u);
    let x0 = (gid.x / cell_w) * cell_w;
    let y0 = (gid.y / cell_h) * cell_h;
    let x1 = min(x0 + cell_w, params.width);
    let y1 = min(y0 + cell_h, params.height);

    var sum_rgb = vec3<f32>(0.0);
    var sum_luma = 0.0;
    var count = 0.0;
    var yy = y0;
    loop {
        if (yy >= y1) { break; }
        var xx = x0;
        loop {
            if (xx >= x1) { break; }
            let p = unpack_rgba8(source_pixels[yy * params.width + xx]);
            sum_rgb += p.rgb;
            sum_luma += luma(p.rgb);
            count += 1.0;
            xx += 1u;
        }
        yy += 1u;
    }

    let avg_rgb = sum_rgb / max(count, 1.0);
    var y = pow(clamp((sum_luma / max(count, 1.0)) * params.contrast, 0.0, 1.0), max(params.gamma, 0.001));
    if (params.invert != 0u) {
        y = 1.0 - y;
    }

    var levels = 8.0;
    if (params.preset == 1u) {
        levels = 10.0;
    } else if (params.preset == 4u) {
        levels = 12.0;
    }
    let coverage = round(y * levels) / levels;
    let local_x = gid.x - x0;
    let local_y = gid.y - y0;
    let fill = select(
        select(
            band_fill(local_y, cell_h, coverage),
            braille_fill(u32(clamp(floor(y * 8.0 + 0.5), 0.0, 8.0)), local_x, local_y, cell_w, cell_h),
            params.preset == 3u
        ),
        ascii_fill(u32(clamp(floor(y * 9.0 + 0.5), 0.0, 9.0)), local_x, local_y, cell_w, cell_h),
        params.preset == 1u
    );

    var fg = params.foreground.rgb;
    if (params.color_mode == 2u) {
        fg = avg_rgb;
    } else if (params.color_mode == 3u) {
        fg = mix(params.background.rgb, params.foreground.rgb, y);
    } else if (params.color_mode == 4u) {
        fg = vec3<f32>(
            mix(params.background.r, params.foreground.r, y),
            mix(params.background.g, params.foreground.g, sqrt(y)),
            mix(params.background.b, params.foreground.b, pow(y, 0.35)),
        );
    }

    let cell_color = vec4<f32>(select(params.background.rgb, fg, fill), 1.0);
    let mixed = mix(cell_color, src, params.source_mix);
    output_pixels[idx] = pack_rgba8(vec4<f32>(mixed.rgb, 1.0));
}
