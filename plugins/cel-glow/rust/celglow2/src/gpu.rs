//! wgpu render path used by `SmartRenderGpu`.
//!
//! The first AE integration keeps field construction on the shared CPU core
//! and moves the full-screen band evaluation/compositing pass to wgpu. This
//! keeps the shader deterministic with the CLI preview while retaining a CPU
//! fallback for unsupported AE GPU frame layouts.

use bytemuck::{Pod, Zeroable};
use celglow_core::{field::Field, FrameBuf, Params};
use std::sync::mpsc;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuParams {
    size: [u32; 2],
    ring_count: u32,
    show_outermost: u32,
    peak: f32,
    distribution: f32,
    line_width: f32,
    core_level: f32,
    core_softness: f32,
    outer_falloff: f32,
    opacity: f32,
    _fill_padding: f32,
    fill: [f32; 4],
    write_glow_alpha: u32,
    phase: f32,
    _padding: [u32; 6],
}

fn checked_size(w: usize, h: usize, channels: usize, limits: &wgpu::Limits) -> Result<u64, String> {
    let invalid = || "unsupported GPU frame dimensions, length or device limit".to_string();
    let width = u32::try_from(w).map_err(|_| invalid())?;
    let height = u32::try_from(h).map_err(|_| invalid())?;
    let count = w.checked_mul(h).and_then(|n| n.checked_mul(4)).ok_or_else(invalid)?;
    let bytes = count.checked_mul(4).ok_or_else(invalid)?;
    if width == 0 || height == 0 || channels != count || count > u32::MAX as usize
        || bytes > isize::MAX as usize || bytes as u64 > limits.max_buffer_size
        || bytes as u64 > u64::from(limits.max_storage_buffer_binding_size)
        || width.div_ceil(8) > limits.max_compute_workgroups_per_dimension
        || height.div_ceil(8) > limits.max_compute_workgroups_per_dimension {
        return Err(invalid());
    }
    Ok(bytes as u64)
}

pub fn render(input: FrameBuf<'_>, params: &Params) -> Result<Vec<f32>, String> {
    // Validate before Field::build, which allocates and indexes by dimensions.
    let limits = wgpu::Limits::downlevel_defaults();
    checked_size(input.w, input.h, input.rgba.len(), &limits)?;
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
    })).map_err(|e| format!("no usable GPU adapter: {e}"))?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("CelGlow AE GPU"),
        required_features: wgpu::Features::empty(),
        required_limits: limits,
        memory_hints: wgpu::MemoryHints::Performance,
        trace: wgpu::Trace::Off,
    })).map_err(|e| format!("cannot create GPU device: {e}"))?;

    let byte_len = checked_size(input.w, input.h, input.rgba.len(), &device.limits())?;
    let mut output = Vec::new();
    output.try_reserve_exact(input.rgba.len()).map_err(|e| format!("GPU readback allocation failed: {e}"))?;
    let field = Field::build(input, params).map_err(|e| e.to_string())?;
    let input_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("CelGlow AE input"), contents: bytemuck::cast_slice(input.rgba),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let field_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("CelGlow AE field"), contents: bytemuck::cast_slice(&field.values),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("CelGlow AE output"), size: byte_len,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("CelGlow AE readback"), size: byte_len,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let uniform = GpuParams {
        size: [input.w as u32, input.h as u32],
        ring_count: params.rings.ring_count as u32,
        show_outermost: u32::from(params.rings.show_outermost),
        peak: field.peak.max(f32::EPSILON),
        distribution: params.rings.distribution,
        line_width: params.rings.line_width / 100.0,
        core_level: params.rings.core_level / 100.0,
        core_softness: params.rings.core_softness / 100.0,
        outer_falloff: params.rings.outer_falloff,
        opacity: params.output.opacity / 100.0,
        _fill_padding: 0.0,
        fill: [params.output.fill_color[0], params.output.fill_color[1], params.output.fill_color[2], 0.0],
        write_glow_alpha: u32::from(params.output.write_glow_alpha),
        phase: params.rings.phase / 360.0,
        _padding: [0; 6],
    };
    let params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("CelGlow AE parameters"), contents: bytemuck::bytes_of(&uniform),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("CelGlow AE bands"), source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("CelGlow AE bands pipeline"), layout: None, module: &shader,
        entry_point: Some("main"), compilation_options: wgpu::PipelineCompilationOptions::default(), cache: None,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("CelGlow AE bind group"), layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: input_buffer.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: field_buffer.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 2, resource: output_buffer.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 3, resource: params_buffer.as_entire_binding() },
        ],
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("CelGlow AE commands") });
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("CelGlow AE pass"), timestamp_writes: None });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups((input.w as u32).div_ceil(8), (input.h as u32).div_ceil(8), 1);
    }
    encoder.copy_buffer_to_buffer(&output_buffer, 0, &readback, 0, byte_len);
    queue.submit(Some(encoder.finish()));
    with_cleanup(|| {
        let (sender, receiver) = mpsc::channel();
        readback.map_async(wgpu::MapMode::Read, .., move |result| { let _ = sender.send(result); });
        device.poll(wgpu::PollType::wait()).map_err(|e| format!("GPU poll failed: {e}"))?;
        receiver.recv().map_err(|e| format!("GPU map callback failed: {e}"))?.map_err(|e| format!("GPU readback failed: {e}"))?;
        let bytes = readback.get_mapped_range(..);
        decode_readback(&bytes, input.rgba.len(), &mut output)?;
        Ok(output)
    }, || readback.unmap())
}

fn decode_readback(bytes: &[u8], channels: usize, output: &mut Vec<f32>) -> Result<(), String> {
    if channels.checked_mul(4) != Some(bytes.len()) || output.capacity() < channels {
        return Err("GPU readback length or capacity mismatch".into());
    }
    output.clear();
    output.extend(bytes.chunks_exact(4).map(|b| f32::from_ne_bytes(b.try_into().unwrap())));
    Ok(())
}

fn with_cleanup<T>(work: impl FnOnce() -> T, cleanup: impl FnOnce()) -> T {
    struct Cleanup<F: FnOnce()>(Option<F>);
    impl<F: FnOnce()> Drop for Cleanup<F> {
        fn drop(&mut self) { if let Some(cleanup) = self.0.take() { cleanup(); } }
    }
    let _cleanup = Cleanup(Some(cleanup));
    work()
}

const SHADER: &str = r#"
struct Params { size: vec2<u32>, ring_count: u32, show_outermost: u32, peak: f32, distribution: f32, line_width: f32, core_level: f32, core_softness: f32, outer_falloff: f32, opacity: f32, fill: vec4<f32>, write_glow_alpha: u32, phase: f32, _padding: vec3<u32>, };
@group(0) @binding(0) var<storage, read> input_rgba: array<f32>;
@group(0) @binding(1) var<storage, read> field: array<f32>;
@group(0) @binding(2) var<storage, read_write> output_rgba: array<f32>;
@group(0) @binding(3) var<uniform> p: Params;
fn smooth01(a: f32, b: f32, x: f32) -> f32 { if (a>=b) { return select(0.0,1.0,x>=b); } let t = clamp((x-a)/(b-a),0.0,1.0); return t*t*(3.0-2.0*t); }
@compute @workgroup_size(8,8)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
  if (gid.x >= p.size.x || gid.y >= p.size.y) { return; }
  let px = gid.y*p.size.x+gid.x; let i=px*4u;
  let f=clamp(field[px]/p.peak,0.0,1.0);
  let fade=min(p.core_softness,p.core_level);
  let core=smooth01(p.core_level-fade,p.core_level+fade,f);
  let u=clamp(f/max(p.core_level,0.000001),0.0,1.0);
  let phase=f32(p.ring_count)*pow(max(1.0-u,0.0),p.distribution)+p.phase;
  let band=i32(floor(phase)); let d=abs(fract(phase)-0.5);
  let outer=select(pow(u,p.outer_falloff),1.0,p.outer_falloff<=0.0);
  var a=select(0.0,1.0,d<=p.line_width*0.5)*outer*(1.0-core)*p.opacity;
  if (band<0 || band>=i32(p.ring_count)) { a=0.0; }
  if (p.show_outermost==0u && band==i32(p.ring_count)-1) { a=0.0; }
  let src=vec4<f32>(input_rgba[i],input_rgba[i+1u],input_rgba[i+2u],input_rgba[i+3u]);
  output_rgba[i]=src.x+p.fill.x*a; output_rgba[i+1u]=src.y+p.fill.y*a; output_rgba[i+2u]=src.z+p.fill.z*a;
  let glow_alpha=max(p.fill.x*a,max(p.fill.y*a,p.fill.z*a));
  output_rgba[i+3u]=select(src.w,max(src.w,glow_alpha),p.write_glow_alpha!=0u);
}
"#;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires actual GPU and isolated CPU phase reference"]
    fn gpu_ring_phase_matches_cpu() {
        let source = [0.5,0.5,0.5,0.0,1.0,1.0,1.0,0.0];
        let input = FrameBuf {w:2,h:1,rgba:&source};
        let mut params = Params::default();
        params.source.channel = celglow_core::params::SourceChannel::Luma;
        params.source.spread = 0.0;
        params.wobble.amount = 0.0;
        params.unevenness.amount = 0.0;
        params.texture.influence = 0.0;
        params.rings.ring_count = 1;
        params.rings.distribution = 1.0;
        params.rings.line_width = 100.0;
        params.rings.line_hardness = 100.0;
        params.rings.core_level = 75.0;
        params.rings.core_softness = 1.0;
        params.rings.outer_falloff = 0.0;
        params.rings.brightness_variation = 0.0;
        params.rings.color_scramble = 0.0;
        params.rings.show_outermost = true;
        for phase in [-180.0,0.0,180.0,360.0] {
            params.rings.phase = phase;
            let mut reference = [0.0;8];
            celglow_core::render(input,(0,0),(0,0),(2,1),&params,celglow_core::DebugView::Result,&mut reference).unwrap();
            let actual = render(input,&params).unwrap();
            for (a,b) in actual.iter().zip(reference) {
                assert!((a-b).abs()<1e-6,"phase={phase}: {actual:?} != {reference:?}");
            }
        }
    }
    #[test]
    fn readback_checks_length_and_preserves_float_bits_without_alignment() {
        let values = [0.0f32, -1.0, 4.0, f32::from_bits(0x7fc01234)];
        let mut bytes = vec![0xff];
        for value in values { bytes.extend_from_slice(&value.to_ne_bytes()); }
        let mut output = Vec::with_capacity(4);
        decode_readback(&bytes[1..],4,&mut output).unwrap();
        assert_eq!(output.iter().map(|v|v.to_bits()).collect::<Vec<_>>(), values.map(f32::to_bits));
        let original = output.iter().map(|v|v.to_bits()).collect::<Vec<_>>();
        assert!(decode_readback(&bytes[2..],4,&mut output).is_err());
        assert!(decode_readback(&bytes[1..],usize::MAX,&mut output).is_err());
        assert_eq!(output.iter().map(|v|v.to_bits()).collect::<Vec<_>>(),original);
        assert!(decode_readback(&bytes[1..],4,&mut Vec::new()).is_err());
    }
    #[test]
    fn readback_cleanup_covers_errors_and_unwind() {
        for mode in 0..3 {
            let calls = std::cell::Cell::new(0);
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(||
                with_cleanup(|| match mode {0=>Ok(7),1=>Err("injected"),_=>panic!("injected")},
                    || calls.set(calls.get()+1))));
            assert_eq!(calls.get(),1);
            match mode {0=>assert_eq!(result.unwrap(),Ok(7)),1=>assert_eq!(result.unwrap(),Err("injected")),_=>assert!(result.is_err())}
        }
    }
    #[test]
    fn gpu_dimensions_rejected_before_field_or_device_creation() {
        let mut limits = wgpu::Limits::downlevel_defaults();
        assert_eq!(checked_size(17,31,17*31*4,&limits).unwrap(),17*31*16);
        for (w,h,n) in [(0,1,0),(1,1,3),(1,1,5),(usize::MAX,2,0),(65536,65536,0)] {
            assert!(checked_size(w,h,n,&limits).is_err());
        }
        limits.max_buffer_size = 63;
        assert!(checked_size(2,2,16,&limits).is_err());
        limits.max_buffer_size = 64;
        limits.max_storage_buffer_binding_size = 63;
        assert!(checked_size(2,2,16,&limits).is_err());
        limits.max_storage_buffer_binding_size = 64;
        assert_eq!(checked_size(2,2,16,&limits).unwrap(),64);
        limits.max_compute_workgroups_per_dimension = 0;
        assert!(checked_size(1,1,4,&limits).is_err());
        for frame in [FrameBuf {w:usize::MAX,h:2,rgba:&[]}, FrameBuf {w:1,h:1,rgba:&[]}] {
            let error = render(frame,&Params::default()).unwrap_err();
            assert!(error.contains("frame dimensions"));
        }
    }
    #[test]
    fn uniform_matches_wgsl_alignment() {
        assert_eq!(std::mem::offset_of!(GpuParams, fill), 48);
        assert_eq!(std::mem::offset_of!(GpuParams, write_glow_alpha), 64);
        assert_eq!(std::mem::offset_of!(GpuParams, phase), 68);
        assert_eq!(std::mem::size_of::<GpuParams>(), 96);
    }
    #[test]
    #[ignore = "requires actual GPU; tests uniform transport, not full CPU/GPU parity"]
    fn gpu_transports_fill_and_alpha_flag() {
        let mut params = Params::default();
        params.rings.ring_count = 1;
        params.rings.line_width = 100.0;
        params.rings.core_level = 75.0;
        params.rings.core_softness = 1.0;
        params.rings.outer_falloff = 0.0;
        params.rings.show_outermost = true;
        params.output.fill_color = [0.2,0.4,0.8];
        params.output.opacity = 50.0;
        params.source.channel = celglow_core::params::SourceChannel::Luma;
        params.source.spread = 0.0;
        for (w,h) in [(2,1),(17,31)] {
            let mut source = [0.5,0.5,0.5,0.0].repeat(w*h);
            source[(w*h-1)*4..(w*h-1)*4+3].fill(1.0);
            for source_alpha in [0.0,0.25,0.75] {
              for pixel in source.chunks_exact_mut(4) { pixel[3] = source_alpha; }
              for write_alpha in [false,true] {
                params.output.write_glow_alpha = write_alpha;
                let output = render(FrameBuf {w,h,rgba:&source}, &params).unwrap();
                assert_eq!(output.len(),source.len());
                for (i,pixel) in output.chunks_exact(4).enumerate() {
                    let expected = if i == w*h-1 { [1.0,1.0,1.0,source_alpha] }
                        else { celglow_core::render::composite_with_options(
                            [0.5,0.5,0.5,source_alpha],[0.1,0.2,0.4,0.5],&params) };
                    for (actual, expected) in pixel.iter().zip(expected) {
                        assert!((actual-expected).abs()<1e-6,"{pixel:?} != {expected}");
                    }
                }
              }
            }
        }
    }
    #[test]
    #[ignore = "requires actual GPU and CPU black-field reference"]
    fn gpu_black_field_has_no_out_of_range_ring() {
        let mut params = Params::default();
        params.rings.line_width = 100.0;
        params.rings.show_outermost = true;
        params.output.write_glow_alpha = true;
        let source = [0.0;4];
        let input = FrameBuf {w:1,h:1,rgba:&source};
        let mut reference = [0.0;4];
        celglow_core::render(input,(0,0),(0,0),(1,1),&params,celglow_core::DebugView::Result,&mut reference).unwrap();
        assert_eq!(reference, [0.0;4]);
        assert_eq!(render(input,&params).unwrap(), reference);
    }
    #[test]
    #[ignore = "requires actual GPU and CPU core-boundary reference"]
    fn gpu_core_softness_boundaries_match_cpu() {
        let source = [1.0;4];
        let input = FrameBuf {w:1,h:1,rgba:&source};
        for (level,softness) in [(100.0,0.0),(25.0,100.0)] {
            let mut params = Params::default();
            params.wobble.amount = 0.0;
            params.unevenness.amount = 0.0;
            params.texture.influence = 0.0;
            params.source.spread = 0.0;
            params.rings.ring_count = 1;
            params.rings.line_width = 100.0;
            params.rings.show_outermost = true;
            params.rings.core_level = level;
            params.rings.core_softness = softness;
            let mut reference = [0.0;4];
            celglow_core::render(input,(0,0),(0,0),(1,1),&params,celglow_core::DebugView::Result,&mut reference).unwrap();
            assert_eq!(reference,source);
            let output = render(input,&params).unwrap();
            for (actual,expected) in output.iter().zip(reference) {
                assert!((actual-expected).abs()<1e-6,"level={level} softness={softness}: {output:?} != {reference:?}");
            }
        }
    }
}
