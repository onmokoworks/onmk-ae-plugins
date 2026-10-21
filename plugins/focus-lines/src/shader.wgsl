// FocusLines coverage kernel. Same policy as raster.rs::cpu_coverage:
// S scanlines per pixel, union of primitive intervals clipped to the pixel,
// minus the hole-ellipse interval, averaged over S. Not an exact area.

struct Params {
    rect_x0: i32,
    rect_y0: i32,
    width: u32,
    height: u32,
    strip_y0: u32,
    strip_h: u32,
    tiles_x: u32,
    tiles_y: u32,
    tile_x0: i32,
    tile_y0: i32,
    samples: u32,
    hole: u32,
    hcx: f32,
    hcy: f32,
    m00: f32,
    m01: f32,
    m10: f32,
    m11: f32,
    _pad0: u32,
    _pad1: u32,
};

@group(0) @binding(0) var<uniform> P: Params;
// 8 vertices per primitive packed as 4 vec4 (x0, y0, x1, y1).
@group(0) @binding(1) var<storage, read> verts: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> counts: array<u32>;
@group(0) @binding(3) var<storage, read> offsets: array<u32>;
@group(0) @binding(4) var<storage, read> items: array<u32>;
@group(0) @binding(5) var<storage, read_write> out_cov: array<f32>;

const TILE: i32 = 16;
const MAX_IV: u32 = 8u;

fn vtx(p: u32, i: u32) -> vec2<f32> {
    let q = verts[p * 4u + i / 2u];
    if (i % 2u) == 0u {
        return q.xy;
    }
    return q.zw;
}

fn floor_div(a: i32, b: i32) -> i32 {
    var q = a / b;
    if (a % b != 0) && ((a < 0) != (b < 0)) {
        q = q - 1;
    }
    return q;
}

// Slow path for unusually many disjoint slivers within one pixel. Repeatedly
// advance to the next interval endpoint; no fixed interval capacity or filling
// of gaps. The usual <=8-piece case retains the fast local-array path.
fn interval(p: u32, ys: f32, px0: f32, px1: f32) -> vec2<f32> {
    var lo = px1;
    var hi = px0;
    let c = counts[p];
    for (var e = 0u; e < c; e = e + 1u) {
        let a = vtx(p, e);
        let b = vtx(p, (e + 1u) % c);
        if (a.y <= ys && b.y > ys) || (b.y <= ys && a.y > ys) {
            let x = a.x + (ys - a.y) * (b.x - a.x) / (b.y - a.y);
            lo = min(lo, x);
            hi = max(hi, x);
        }
    }
    return vec2<f32>(max(lo, px0), min(hi, px1));
}

fn full_union(begin: u32, end: u32, ys: f32, px0: f32, px1: f32,
              has_hole: bool, hl: f32, hr: f32) -> f32 {
    var cursor = px0;
    var length = 0.0;
    loop {
        if cursor >= px1 { break; }
        var far = cursor;
        var next = px1;
        for (var k = begin; k < end; k = k + 1u) {
            let iv = interval(items[k], ys, px0, px1);
            if iv.y <= iv.x || iv.y <= cursor { continue; }
            if iv.x <= cursor {
                far = max(far, iv.y);
            } else {
                next = min(next, iv.x);
            }
        }
        if far > cursor {
            var piece = far - cursor;
            if has_hole { piece -= max(0.0, min(far, hr) - max(cursor, hl)); }
            length += piece;
            cursor = far;
        } else {
            cursor = next;
        }
    }
    return length;
}

@compute @workgroup_size(16, 16, 1)
fn cs_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if gid.x >= P.width || gid.y >= P.strip_h {
        return;
    }
    let row = P.strip_y0 + gid.y;
    if row >= P.height {
        return;
    }
    let ax = P.rect_x0 + i32(gid.x);
    let ay = P.rect_y0 + i32(row);
    let tx = floor_div(ax, TILE) - P.tile_x0;
    let ty = floor_div(ay, TILE) - P.tile_y0;
    let out_index = gid.y * P.width + gid.x;
    if tx < 0 || ty < 0 || u32(tx) >= P.tiles_x || u32(ty) >= P.tiles_y {
        out_cov[out_index] = 0.0;
        return;
    }
    let t = u32(ty) * P.tiles_x + u32(tx);
    let begin = offsets[t];
    let end = offsets[t + 1u];
    let px0 = f32(ax);
    let px1 = px0 + 1.0;
    var total = 0.0;
    if begin < end {
        for (var s = 0u; s < P.samples; s = s + 1u) {
            let ys = f32(ay) + (f32(s) + 0.5) / f32(P.samples);
            var lo: array<f32, 8>;
            var hi: array<f32, 8>;
            var n = 0u;
            var overflow = false;
            for (var k = begin; k < end; k = k + 1u) {
                let p = items[k];
                let c = counts[p];
                var xmin = 3.0e38;
                var xmax = -3.0e38;
                var hits = 0u;
                for (var e = 0u; e < c; e = e + 1u) {
                    let a = vtx(p, e);
                    let b = vtx(p, (e + 1u) % c);
                    if (a.y <= ys && b.y > ys) || (b.y <= ys && a.y > ys) {
                        let x = a.x + (ys - a.y) * (b.x - a.x) / (b.y - a.y);
                        xmin = min(xmin, x);
                        xmax = max(xmax, x);
                        hits = hits + 1u;
                    }
                }
                if hits < 2u || xmax <= xmin {
                    continue;
                }
                var ia = max(xmin, px0);
                var ib = min(xmax, px1);
                if ib <= ia {
                    continue;
                }
                // Insert into the disjoint interval set (union).
                var i = 0u;
                loop {
                    if i >= n {
                        break;
                    }
                    if ia <= hi[i] && ib >= lo[i] {
                        ia = min(ia, lo[i]);
                        ib = max(ib, hi[i]);
                        n = n - 1u;
                        lo[i] = lo[n];
                        hi[i] = hi[n];
                    } else {
                        i = i + 1u;
                    }
                }
                if n < MAX_IV {
                    lo[n] = ia;
                    hi[n] = ib;
                    n = n + 1u;
                } else {
                    overflow = true;
                }
            }
            if n == 0u {
                continue;
            }
            var has_hole = false;
            var hl = 0.0;
            var hr = 0.0;
            if P.hole != 0u {
                let dy = ys - P.hcy;
                let qa = P.m00 * P.m00 + P.m10 * P.m10;
                let qb = 2.0 * (P.m00 * P.m01 + P.m10 * P.m11) * dy;
                let qc = (P.m01 * P.m01 + P.m11 * P.m11) * dy * dy - 1.0;
                let disc = qb * qb - 4.0 * qa * qc;
                if qa > 0.0 && disc > 0.0 {
                    let r = sqrt(disc);
                    hl = P.hcx + (-qb - r) / (2.0 * qa);
                    hr = P.hcx + (-qb + r) / (2.0 * qa);
                    has_hole = true;
                }
            }
            var len = 0.0;
            for (var j = 0u; j < n; j = j + 1u) {
                len = len + (hi[j] - lo[j]);
                if has_hole {
                    len = len - max(0.0, min(hi[j], hr) - max(lo[j], hl));
                }
            }
            if overflow {
                len = full_union(begin, end, ys, px0, px1, has_hole, hl, hr);
            }
            total = total + clamp(len, 0.0, 1.0);
        }
    }
    out_cov[out_index] = clamp(total / f32(P.samples), 0.0, 1.0);
}
