struct Params {
    width: u32,
    height: u32,
    num_frames: u32,
    interpolation: u32,
    mix: f32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> frames: array<u32>;
@group(0) @binding(2) var<storage, read> original: array<u32>;
@group(0) @binding(3) var<storage, read> time_map: array<f32>;
@group(0) @binding(4) var<storage, read_write> output: array<u32>;

// Native little-endian: memory [A,R,G,B] maps to u32 = A | (R<<8) | (G<<16) | (B<<24).
fn unpack_argb(v: u32) -> vec4<f32> {
    return vec4<f32>(
        f32( v         & 0xFFu) / 255.0,
        f32((v >> 8u)  & 0xFFu) / 255.0,
        f32((v >> 16u) & 0xFFu) / 255.0,
        f32((v >> 24u) & 0xFFu) / 255.0
    );
}

fn pack_argb(c: vec4<f32>) -> u32 {
    let a = u32(clamp(c.x * 255.0 + 0.5, 0.0, 255.0));
    let r = u32(clamp(c.y * 255.0 + 0.5, 0.0, 255.0));
    let g = u32(clamp(c.z * 255.0 + 0.5, 0.0, 255.0));
    let b = u32(clamp(c.w * 255.0 + 0.5, 0.0, 255.0));
    return a | (r << 8u) | (g << 16u) | (b << 24u);
}

@compute @workgroup_size(16, 16)
fn slit_scan_pass(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.width || id.y >= params.height {
        return;
    }

    let idx = id.y * params.width + id.x;
    let npx = params.width * params.height;

    let map_val = clamp(time_map[idx], 0.0, 1.0);
    let max_idx = f32(params.num_frames - 1u);
    let frame_f = map_val * max_idx;

    var result: vec4<f32>;

    if params.interpolation == 1u {
        let fi = min(u32(round(frame_f)), params.num_frames - 1u);
        result = unpack_argb(frames[fi * npx + idx]);
    } else {
        let lo = min(u32(floor(frame_f)), params.num_frames - 1u);
        let hi = min(lo + 1u, params.num_frames - 1u);
        let t = frame_f - f32(lo);
        let p_lo = unpack_argb(frames[lo * npx + idx]);
        let p_hi = unpack_argb(frames[hi * npx + idx]);
        result = mix(p_lo, p_hi, t);
    }

    if params.mix < 1.0 {
        let orig = unpack_argb(original[idx]);
        result = mix(orig, result, params.mix);
    }

    output[idx] = pack_argb(result);
}
