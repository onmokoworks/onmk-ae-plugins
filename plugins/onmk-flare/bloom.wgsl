struct Config { dimensions: vec4<u32>, tint: vec4<f32>, strength: vec4<f32> }
@group(0) @binding(0) var<uniform> p: Config;
@group(0) @binding(1) var<storage, read_write> pixels: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> prefix: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read_write> total: array<vec4<f32>>;
// Inclusive prefix, serial in each line: preserve CPU addition order. Lines
// are independent. Separate dispatches provide the required global barriers.
@compute @workgroup_size(64)
fn prefix_rows(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= p.dimensions.y { return; }
    var sum = vec4<f32>(0.0);
    for (var x=0u; x<p.dimensions.x; x+=1u) {
        let i = id.x*p.dimensions.x+x;
        sum += pixels[i]; prefix[i] = sum;
    }
}
@compute @workgroup_size(64)
fn prefix_columns(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= p.dimensions.x { return; }
    var sum = vec4<f32>(0.0);
    for (var y=0u; y<p.dimensions.y; y+=1u) {
        let i = y*p.dimensions.x+id.x;
        sum += pixels[i]; prefix[i] = sum;
    }
}
@compute @workgroup_size(16,16)
fn apply_rows(@builtin(global_invocation_id) id: vec3<u32>) {
    let w=p.dimensions.x; let h=p.dimensions.y; let r=p.dimensions.z;
    if id.x>=w || id.y>=h { return; }
    let lo=id.x-min(id.x,r); let hi=min(id.x+r+1u,w);
    var lower=vec4<f32>(0.0);
    if lo>0u { lower=prefix[id.y*w+lo-1u]; }
    pixels[id.y*w+id.x]=(prefix[id.y*w+hi-1u]-lower)/f32(hi-lo);
}
@compute @workgroup_size(16,16)
fn apply_columns(@builtin(global_invocation_id) id: vec3<u32>) {
    let w=p.dimensions.x; let h=p.dimensions.y; let r=p.dimensions.z;
    if id.x>=w || id.y>=h { return; }
    let lo=id.y-min(id.y,r); let hi=min(id.y+r+1u,h);
    var lower=vec4<f32>(0.0);
    if lo>0u { lower=prefix[(lo-1u)*w+id.x]; }
    pixels[id.y*w+id.x]=(prefix[(hi-1u)*w+id.x]-lower)/f32(hi-lo);
}
@compute @workgroup_size(16,16)
fn accumulate(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x>=p.dimensions.x || id.y>=p.dimensions.y { return; }
    let i=id.y*p.dimensions.x+id.x;
    total[i] += pixels[i]*p.strength.x*p.strength.y*p.tint;
}
