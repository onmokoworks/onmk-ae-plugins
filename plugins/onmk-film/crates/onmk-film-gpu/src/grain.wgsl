struct Params { width:u32, height:u32, seed:u32, mono:u32, cell:f32, amount:f32, clump:f32, shadows:f32, mids:f32, highlights:f32, grain_base:f32, _pad:f32 }
@group(0) @binding(0) var<storage, read_write> pixels: array<f32>;
@group(0) @binding(1) var<uniform> p: Params;
fn hash_u32(v0:u32)->u32 { var v=v0; v ^= v>>16u; v*=0x7feb352du; v^=v>>15u; v*=0x846ca68bu; v^=v>>16u; return v; }
fn hash_f32(v:u32)->f32 { return f32(hash_u32(v))/4294967295.0*2.0-1.0; }
fn fade(t:f32)->f32 { return t*t*t*(t*(t*6.0-15.0)+10.0); }
fn lattice_hash(q:vec2<i32>,s:u32,c:u32)->f32 { return hash_f32(u32(q.x)*0x1f123bb5u+u32(q.y)*0x5f356495u+s+c*0x9e3779b9u); }
fn lattice(pos:vec2<f32>, seed:u32, channel:u32)->f32 {
 let base=vec2<i32>(floor(pos)); let f=fract(pos); let u=vec2<f32>(fade(f.x),fade(f.y));
 let a=mix(lattice_hash(base,seed,channel),lattice_hash(base+vec2<i32>(1,0),seed,channel),u.x);
 let b=mix(lattice_hash(base+vec2<i32>(0,1),seed,channel),lattice_hash(base+vec2<i32>(1,1),seed,channel),u.x); return mix(a,b,u.y);
}
@compute @workgroup_size(8,8)
fn main(@builtin(global_invocation_id) id:vec3<u32>) {
 if(id.x>=p.width||id.y>=p.height){return;} let o=(id.y*p.width+id.x)*4u;
 let rgb=vec3<f32>(pixels[o],pixels[o+1u],pixels[o+2u]); let y=max(dot(rgb,vec3<f32>(0.2126,0.7152,0.0722)),0.0001);
 let s=(1.0-clamp(y,0.0,1.0)); let hi=clamp(y,0.0,1.0); let mid=max(1.0-abs(2.0*hi-1.0),0.0); let sw=s*s; let hw=hi*hi;
 let band=(sw*p.shadows+mid*p.mids+hw*p.highlights)/max(sw+mid+hw,0.000001);
 let density=max(-log(clamp(y,0.0001,8.0)),0.0); let sigma=p.amount*p.grain_base*sqrt(density+0.15)*band;
 let pos=vec2<f32>(f32(id.x),f32(id.y))/p.cell; let cl=1.0+(0.65+0.35*lattice(pos*0.35,p.seed^0xc1a55ed0u,7u)-1.0)*p.clump;
 for(var c=0u;c<3u;c++){ let channel=select(c+1u,0u,p.mono!=0u); let g=max(1.0+lattice(pos,p.seed,channel)*sigma*cl*0.35,0.05); pixels[o+c]=max(pixels[o+c]*g,0.0); }
}
