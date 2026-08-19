use crate::core::EdgeParams;
use bytemuck::{Pod, Zeroable};
use std::sync::{mpsc, OnceLock};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    size_step: [u32; 4],
    edge0: [f32; 4],
    edge1: [f32; 4],
    motion: [f32; 4],
    seed_detail: [u32; 4],
}

struct Processor {
    device: wgpu::Device,
    queue: wgpu::Queue,
    shader: wgpu::ShaderModule,
    layout: wgpu::PipelineLayout,
    bind_layout: wgpu::BindGroupLayout,
}
static GPU: OnceLock<Option<Processor>> = OnceLock::new();

fn processor() -> Option<&'static Processor> {
    GPU.get_or_init(|| Processor::new().ok()).as_ref()
}

impl Processor {
    fn new() -> Result<Self, String> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
        }))
        .map_err(|e| format!("no GPU adapter: {e}"))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("EdgeSmith GPU"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            memory_hints: wgpu::MemoryHints::Performance,
            trace: wgpu::Trace::Off,
        }))
        .map_err(|e| format!("GPU device failed: {e}"))?;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("EdgeSmith shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shader.wgsl").into()),
        });
        let storage = |binding, read_only| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("EdgeSmith bind layout"),
            entries: &[
                storage(0, true),
                storage(1, true),
                storage(2, false),
                storage(3, false),
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("EdgeSmith pipeline layout"),
            bind_group_layouts: &[&bind_layout],
            push_constant_ranges: &[],
        });
        Ok(Self {
            device,
            queue,
            shader,
            layout,
            bind_layout,
        })
    }

    fn render(
        &self,
        input: &[f32],
        w: usize,
        h: usize,
        p: &EdgeParams,
    ) -> Result<Vec<f32>, String> {
        if w == 0 || h == 0 || input.len() < w * h * 4 {
            return Err("invalid frame".into());
        }
        let bytes = (w * h * 4 * 4) as u64;
        let source = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("source"),
                contents: bytemuck::cast_slice(input),
                usage: wgpu::BufferUsages::STORAGE,
            });
        let seed_usage = wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_SRC
            | wgpu::BufferUsages::COPY_DST;
        let seeds_a = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("seeds a"),
            size: bytes,
            usage: seed_usage,
            mapped_at_creation: false,
        });
        let seeds_b = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("seeds b"),
            size: bytes,
            usage: seed_usage,
            mapped_at_creation: false,
        });
        let output = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("output"),
            size: bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: bytes,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut u = Uniforms {
            size_step: [w as u32, h as u32, 0, 0],
            edge0: [p.border, p.roughness, p.scale.max(1.0), p.balance / 100.0],
            edge1: [
                p.sharpness / 100.0,
                p.stretch / 100.0,
                p.preserve / 100.0,
                p.mix,
            ],
            motion: [p.angle.to_radians(), p.evolution, 0.0, 0.0],
            seed_detail: [p.seed, p.detail.clamp(1, 8), 0, 0],
        };
        let uniform = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("uniform"),
                contents: bytemuck::bytes_of(&u),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
        let init = self.pipeline("init_seeds");
        let jump = self.pipeline("jump");
        let finish = self.pipeline("finish");
        let group = |read: &wgpu::Buffer, write: &wgpu::Buffer| {
            self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.bind_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: source.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: read.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: write.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: output.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: uniform.as_entire_binding(),
                    },
                ],
            })
        };
        self.dispatch(&init, &group(&seeds_b, &seeds_a), w, h);
        let mut step = 1u32;
        while step < (w.max(h) as u32) {
            step <<= 1;
        }
        step >>= 1;
        let mut a_is_read = true;
        while step > 0 {
            u.size_step[2] = step;
            self.queue.write_buffer(&uniform, 0, bytemuck::bytes_of(&u));
            let (read, write) = if a_is_read {
                (&seeds_a, &seeds_b)
            } else {
                (&seeds_b, &seeds_a)
            };
            self.dispatch(&jump, &group(read, write), w, h);
            a_is_read = !a_is_read;
            step >>= 1;
        }
        let final_seeds = if a_is_read { &seeds_a } else { &seeds_b };
        let spare = if a_is_read { &seeds_b } else { &seeds_a };
        self.dispatch(&finish, &group(final_seeds, spare), w, h);
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("EdgeSmith readback"),
            });
        enc.copy_buffer_to_buffer(&output, 0, &readback, 0, bytes);
        self.queue.submit(Some(enc.finish()));
        let (tx, rx) = mpsc::channel();
        readback.map_async(wgpu::MapMode::Read, .., move |r| {
            let _ = tx.send(r);
        });
        self.device
            .poll(wgpu::PollType::wait())
            .map_err(|e| format!("GPU wait failed: {e}"))?;
        rx.recv()
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        let mapped = readback.get_mapped_range(..);
        let result = bytemuck::cast_slice::<u8, f32>(&mapped).to_vec();
        drop(mapped);
        readback.unmap();
        Ok(result)
    }
    fn pipeline(&self, entry: &str) -> wgpu::ComputePipeline {
        self.device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&self.layout),
                module: &self.shader,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
    }
    fn dispatch(
        &self,
        pipeline: &wgpu::ComputePipeline,
        group: &wgpu::BindGroup,
        w: usize,
        h: usize,
    ) {
        let mut e = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = e.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, group, &[]);
            pass.dispatch_workgroups((w as u32).div_ceil(8), (h as u32).div_ceil(8), 1);
        }
        self.queue.submit(Some(e.finish()));
    }
}

pub fn render(input: &[f32], w: usize, h: usize, p: &EdgeParams) -> Result<Vec<f32>, String> {
    processor()
        .ok_or_else(|| "GPU unavailable".to_string())?
        .render(input, w, h, p)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gpu_shader_renders_when_adapter_is_available() {
        let mut src = vec![0.0f32; 8 * 8 * 4];
        for y in 2..6 {
            for x in 2..6 {
                src[(y * 8 + x) * 4 + 3] = 1.0;
            }
        }
        if let Ok(out) = render(&src, 8, 8, &EdgeParams::default()) {
            assert_eq!(out.len(), src.len());
            assert!(out.iter().all(|v| v.is_finite()));
        }
    }
}
