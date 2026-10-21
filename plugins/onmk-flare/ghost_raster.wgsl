struct Config { width:u32, height:u32, tiles_x:u32, pad:u32 }
struct Triangle { a:vec4<f32>, b:vec4<f32>, c:vec4<f32>, factors:vec4<f32> }
@group(0) @binding(0) var<uniform> p:Config;
@group(0) @binding(1) var<storage,read> triangles:array<Triangle>;
@group(0) @binding(2) var<storage,read> ranges:array<vec2<u32>>;
@group(0) @binding(3) var<storage,read> indices:array<u32>;
@group(0) @binding(4) var<storage,read_write> output:array<vec4<f32>>;
fn edge(a:vec2<f32>,b:vec2<f32>,q:vec2<f32>)->f32 {
    let first=(q.x-a.x)*(b.y-a.y);
    let second=(q.y-a.y)*(b.x-a.x);
    return first-second;
}
@compute @workgroup_size(8,8)
fn raster(@builtin(global_invocation_id) id:vec3<u32>) {
    if id.x>=p.width || id.y>=p.height {return;}
    let q=vec2<f32>(id.xy)+vec2<f32>(0.5);
    let tile=(id.y/16u)*p.tiles_x+id.x/16u;
    let range=ranges[tile];var sum=vec4<f32>(0.0);
    for (var j=0u;j<range.y;j+=1u) {
        let t=triangles[indices[range.x+j]];let channel=u32(t.a.w);
        if t.b.w==0.0 {
            let wx=max(1.0-abs(q.x-t.a.x)/1.5,0.0);
            let wy=max(1.0-abs(q.y-t.a.y)/1.5,0.0);
            sum[channel]+=t.a.z*wx*wy;
        } else {
            let e0=edge(t.a.xy,t.b.xy,q);let e1=edge(t.b.xy,t.c.xy,q);let e2=edge(t.c.xy,t.a.xy,q);
            if (e0>=0.0 && e1>=0.0 && e2>=0.0) || (e0<=0.0 && e1<=0.0 && e2<=0.0) {
                let wa=abs(e1)*t.factors.x;let wb=abs(e2)*t.factors.x;
                let wc=max(1.0-wa-wb,0.0);var local=1.0;
                if t.factors.z>1e-12 {local=(wa*t.a.z+wb*t.b.z+wc*t.c.z)/t.factors.z;}
                sum[channel]+=t.factors.y*local;
            }
        }
    }
    output[id.y*p.width+id.x]=sum;
}
