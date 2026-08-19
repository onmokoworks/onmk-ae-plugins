struct Params { width: u32, height: u32, radius: u32, horizontal: u32 }
@group(0) @binding(0) var<storage, read> input: array<f32>;
@group(0) @binding(1) var<storage, read_write> output: array<f32>;
@group(0) @binding(2) var<uniform> p: Params;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
  if (id.x >= p.width || id.y >= p.height) { return; }
  let center = (id.y * p.width + id.x) * 4u;
  var sum = vec3<f32>(0.0); var count = 0.0;
  for (var d = -i32(p.radius); d <= i32(p.radius); d++) {
    let x = clamp(i32(id.x) + select(0, d, p.horizontal != 0u), 0, i32(p.width) - 1);
    let y = clamp(i32(id.y) + select(d, 0, p.horizontal != 0u), 0, i32(p.height) - 1);
    let o = (u32(y) * p.width + u32(x)) * 4u;
    sum += vec3<f32>(input[o], input[o + 1u], input[o + 2u]); count += 1.0;
  }
  let rgb = sum / count; output[center] = rgb.x; output[center + 1u] = rgb.y; output[center + 2u] = rgb.z; output[center + 3u] = input[center + 3u];
}
