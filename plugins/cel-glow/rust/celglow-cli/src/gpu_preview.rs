//! Fast GPU preview for interactive look development.
//!
//! This path deliberately keeps field generation on the CPU for now, then
//! performs the full-screen ring, fill-colour, and alpha composite on a GPU
//! compute pass. The CPU renderer remains the exact reference while this
//! preview path is extended with wobble and alpha roughening.

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
    glow_only: f32,
    fill: [f32; 4],
    write_glow_alpha: u32,
    // Uniform buffers use 16-byte member alignment; keep the host payload at
    // the 96 bytes required by the WGSL layout.
    _padding: [u32; 8],
}

pub fn render(input: FrameBuf<'_>, params: &Params, glow_only: bool) -> Result<Vec<f32>, String> {
    let field = Field::build(input, params);
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
    }))
    .map_err(|e| format!("no usable GPU adapter: {e}"))?;
    let info = adapter.get_info();
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("CelGlow GPU preview"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::downlevel_defaults(),
        memory_hints: wgpu::MemoryHints::Performance,
        trace: wgpu::Trace::Off,
    }))
    .map_err(|e| format!("cannot create GPU device: {e}"))?;

    let byte_len = (input.w * input.h * 4 * std::mem::size_of::<f32>()) as u64;
    let input_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("CelGlow preview input"),
        contents: bytemuck::cast_slice(input.rgba),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let field_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("CelGlow preview field"),
        contents: bytemuck::cast_slice(&field.values),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("CelGlow preview output"),
        size: byte_len,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("CelGlow preview readback"),
        size: byte_len,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let uniform = GpuParams {
        size: [input.w as u32, input.h as u32],
        ring_count: u32::from(params.rings.ring_count),
        show_outermost: u32::from(params.rings.show_outermost),
        peak: field.peak.max(f32::EPSILON),
        distribution: params.rings.distribution,
        line_width: params.rings.line_width / 100.0,
        core_level: params.rings.core_level / 100.0,
        core_softness: params.rings.core_softness / 100.0,
        outer_falloff: params.rings.outer_falloff,
        opacity: params.output.opacity / 100.0,
        glow_only: f32::from(glow_only),
        fill: [
            params.output.fill_color[0],
            params.output.fill_color[1],
            params.output.fill_color[2],
            0.0,
        ],
        write_glow_alpha: u32::from(params.output.write_glow_alpha),
        _padding: [0; 8],
    };
    let params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("CelGlow preview parameters"),
        contents: bytemuck::bytes_of(&uniform),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("CelGlow preview shader"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("CelGlow preview pipeline"),
        layout: None,
        module: &shader,
        entry_point: Some("main"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("CelGlow preview bind group"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: input_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: field_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: output_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: params_buffer.as_entire_binding(),
            },
        ],
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("CelGlow preview commands"),
    });
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("CelGlow preview pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(
            (input.w as u32).div_ceil(8),
            (input.h as u32).div_ceil(8),
            1,
        );
    }
    encoder.copy_buffer_to_buffer(&output_buffer, 0, &readback, 0, byte_len);
    queue.submit(Some(encoder.finish()));

    let (sender, receiver) = mpsc::channel();
    readback.map_async(wgpu::MapMode::Read, .., move |result| {
        let _ = sender.send(result);
    });
    device
        .poll(wgpu::PollType::wait())
        .map_err(|e| format!("GPU preview poll failed: {e}"))?;
    receiver
        .recv()
        .map_err(|e| format!("GPU preview map callback failed: {e}"))?
        .map_err(|e| format!("GPU preview readback failed: {e}"))?;
    let bytes = readback.get_mapped_range(..);
    let output = bytemuck::cast_slice::<u8, f32>(&bytes).to_vec();
    drop(bytes);
    readback.unmap();
    eprintln!("GPU preview: {} ({:?})", info.name, info.backend);
    Ok(output)
}

const SHADER: &str = r#"
struct Params {
    size: vec2<u32>, ring_count: u32, show_outermost: u32,
    peak: f32, distribution: f32, line_width: f32, core_level: f32,
    core_softness: f32, outer_falloff: f32, opacity: f32, glow_only: f32,
    fill: vec4<f32>, write_glow_alpha: u32, _padding: vec3<u32>,
};
@group(0) @binding(0) var<storage, read> input_rgba: array<f32>;
@group(0) @binding(1) var<storage, read> field: array<f32>;
@group(0) @binding(2) var<storage, read_write> output_rgba: array<f32>;
@group(0) @binding(3) var<uniform> p: Params;

fn smooth01(a: f32, b: f32, x: f32) -> f32 {
    let t = clamp((x - a) / max(b - a, 0.000001), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= p.size.x || gid.y >= p.size.y) { return; }
    let pixel = gid.y * p.size.x + gid.x;
    let i = pixel * 4u;
    let f = clamp(field[pixel] / p.peak, 0.0, 1.0);
    let core = smooth01(p.core_level - p.core_softness, p.core_level + p.core_softness, f);
    let u = clamp(f / max(p.core_level, 0.000001), 0.0, 1.0);
    let phase = f32(p.ring_count) * pow(max(1.0 - u, 0.0), p.distribution);
    let band = i32(floor(phase));
    let half_width = p.line_width * 0.5;
    let d = abs(fract(phase) - 0.5);
    let outer = select(pow(u, p.outer_falloff), 1.0, p.outer_falloff <= 0.0);
    var glow_a = select(0.0, 1.0, d <= half_width) * outer * (1.0 - core) * p.opacity;
    if (p.show_outermost == 0u && band == i32(p.ring_count) - 1) { glow_a = 0.0; }
    let glow = p.fill.xyz * glow_a;
    let source = vec4<f32>(input_rgba[i], input_rgba[i + 1u], input_rgba[i + 2u], input_rgba[i + 3u]);
    let result_rgb = select(source.xyz + glow, glow, p.glow_only > 0.5);
    let result_a = select(source.w, max(source.w, glow_a), p.write_glow_alpha != 0u || p.glow_only > 0.5);
    output_rgba[i] = result_rgb.x;
    output_rgba[i + 1u] = result_rgb.y;
    output_rgba[i + 2u] = result_rgb.z;
    output_rgba[i + 3u] = result_a;
}
"#;
