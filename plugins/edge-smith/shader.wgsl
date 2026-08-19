struct Uniforms { size_step: vec4<u32>, edge0: vec4<f32>, edge1: vec4<f32>, motion: vec4<f32>, seed_detail: vec4<u32>, };
@group(0) @binding(0) var<storage,read> source: array<f32>;
@group(0) @binding(1) var<storage,read> seeds_in: array<vec4<i32>>;
@group(0) @binding(2) var<storage,read_write> seeds_out: array<vec4<i32>>;
@group(0) @binding(3) var<storage,read_write> output: array<f32>;
@group(0) @binding(4) var<uniform> p: Uniforms;
fn valid(q:vec2<i32>)->bool{return q.x>=0&&q.y>=0&&q.x<i32(p.size_step.x)&&q.y<i32(p.size_step.y);}
fn d2(a:vec2<i32>,b:vec2<i32>)->i32{let d=a-b;return dot(d,d);}
@compute @workgroup_size(8,8)
fn init_seeds(@builtin(global_invocation_id) g:vec3<u32>){if(g.x>=p.size_step.x||g.y>=p.size_step.y){return;}let i=g.y*p.size_step.x+g.x;let q=vec2<i32>(g.xy);if(source[i*4u+3u]>=0.5){seeds_out[i]=vec4<i32>(q,-1,-1);}else{seeds_out[i]=vec4<i32>(-1,-1,q);}}
@compute @workgroup_size(8,8)
fn jump(@builtin(global_invocation_id) g:vec3<u32>){if(g.x>=p.size_step.x||g.y>=p.size_step.y){return;}let i=g.y*p.size_step.x+g.x;let q=vec2<i32>(g.xy);var best=seeds_in[i];var bi=best.xy;var bo=best.zw;let s=i32(p.size_step.z);for(var oy=-1;oy<=1;oy++){for(var ox=-1;ox<=1;ox++){let n=q+vec2<i32>(ox*s,oy*s);if(valid(n)){let ns=seeds_in[u32(n.y)*p.size_step.x+u32(n.x)];if(valid(ns.xy)&&(!valid(bi)||d2(q,ns.xy)<d2(q,bi))){bi=ns.xy;}if(valid(ns.zw)&&(!valid(bo)||d2(q,ns.zw)<d2(q,bo))){bo=ns.zw;}}}}seeds_out[i]=vec4<i32>(bi,bo);}
fn hash(v0:u32)->f32{var v=v0;v=v^(v>>16u);v=v*0x7feb352du;v=v^(v>>15u);v=v*0x846ca68bu;v=v^(v>>16u);return f32(v)/4294967295.0;}
fn hash2(q:vec2<i32>,s:u32)->f32{let y=bitcast<u32>(q.y);let yr=(y<<16u)|(y>>16u);return hash(bitcast<u32>(q.x)^yr^s);}
fn noise(pos:vec2<f32>,seed:u32)->f32{let b=vec2<i32>(floor(pos));let f=fract(pos);let u=f*f*(3.0-2.0*f);let a=mix(hash2(b,seed),hash2(b+vec2<i32>(1,0),seed),u.x);let c=mix(hash2(b+vec2<i32>(0,1),seed),hash2(b+vec2<i32>(1,1),seed),u.x);return mix(a,c,u.y)*2.0-1.0;}
fn fbm(pos0:vec2<f32>)->f32{let c=cos(p.motion.x);let s=sin(p.motion.x);var pos=vec2<f32>(pos0.x*c-pos0.y*s,pos0.x*s+pos0.y*c)/max(p.edge0.z,1.0);pos.y=pos.y*max(0.01,1.0-p.edge1.y);var sum=0.0;var amp=0.5;var norm=0.0;for(var o=0u;o<p.seed_detail.y;o++){sum=sum+noise(pos+vec2<f32>(p.motion.y*0.01,p.motion.y*0.006),p.seed_detail.x+o*1013u)*amp;norm=norm+amp;pos=pos*2.0;amp=amp*0.5;}return sum/max(norm,0.00001);}
@compute @workgroup_size(8,8)
fn finish(@builtin(global_invocation_id) g:vec3<u32>){if(g.x>=p.size_step.x||g.y>=p.size_step.y){return;}let i=g.y*p.size_step.x+g.x;let si=i*4u;let q=vec2<i32>(g.xy);let sd=seeds_in[i];let inside=source[si+3u]>=0.5;var ds=select(-1000000.0,1000000.0,inside);if(inside&&valid(sd.zw)){ds=sqrt(f32(d2(q,sd.zw)));}if(!inside&&valid(sd.xy)){ds=-sqrt(f32(d2(q,sd.xy)));}let n=fbm(vec2<f32>(g.xy));let biased=select(n*(1.0-p.edge0.w),n*(1.0+p.edge0.w),n>=0.0);let protect=clamp(abs(ds)/(p.edge1.z*25.0+1.0),0.0,1.0);let shaped=ds+p.edge0.x+biased*p.edge0.y*(1.0-protect*0.85);let soft=(1.0-clamp(p.edge1.x,0.0,1.0))*3.5+0.35;let t=clamp((shaped+soft)/(2.0*soft),0.0,1.0);let ea=t*t*(3.0-2.0*t);let alpha=mix(source[si+3u],ea,p.edge1.w);var ci=i;if(!inside&&valid(sd.xy)){ci=u32(sd.xy.y)*p.size_step.x+u32(sd.xy.x);}let cs=ci*4u;output[si]=source[cs];output[si+1u]=source[cs+1u];output[si+2u]=source[cs+2u];output[si+3u]=alpha;}
