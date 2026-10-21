struct Config { dimensions: vec2<u32>, gain: f32, modern: f32, physical_enabled: u32, ocular_enabled: u32, ghost_stride:u32, padding:u32 }
@group(0) @binding(0) var<uniform> p: Config;
@group(0) @binding(1) var<storage, read> ghosts: array<f32>;
@group(0) @binding(2) var<storage, read> bloom: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read> ocular: array<f32>;
@group(0) @binding(4) var<storage, read> source: array<vec4<f32>>;
@group(0) @binding(5) var<storage, read_write> combined: array<vec4<f32>>;
@group(0) @binding(6) var<storage, read_write> invalid: atomic<u32>;
fn finite(v: f32) -> bool { return (bitcast<u32>(v) & 0x7f800000u) != 0x7f800000u; }
fn safe(v: f32) -> f32 { return select(0.0, v, finite(v)); }
@compute @workgroup_size(16,16)
fn combine(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x>=p.dimensions.x || id.y>=p.dimensions.y { return; }
    let i=id.y*p.dimensions.x+id.x;
    var result=vec4<f32>(0.0);
    for (var c=0u;c<3u;c+=1u) {
        if !finite(source[i][c]) || !finite(bloom[i][c]) { atomicStore(&invalid,1u); }
        // Same ordering and finite sanitization as combine_optical_layers.
        var g=0.0;var e=0.0;
        if p.physical_enabled!=0u { g=ghosts[i*p.ghost_stride+c]; }
        if !finite(g) {atomicStore(&invalid,1u);}
        if p.ocular_enabled!=0u { e=ocular[i*3u+c]; }
        result[c]=safe(g+bloom[i][c])*max(p.gain,0.0)
            +safe(e+source[i][c]*p.modern);
        if !finite(result[c]) { atomicStore(&invalid,1u); }
    }
    combined[i]=result;
}
