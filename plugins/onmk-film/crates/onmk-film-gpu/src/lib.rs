//! Metal-first wgpu blur backend. Callers should fall back to the CPU renderer on error.

use std::borrow::Cow;
use wgpu::util::DeviceExt;

#[derive(Debug)]
pub enum GpuError {
    NoAdapter,
    Device,
    Map,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GrainParams {
    pub seed: u32,
    pub mono: u32,
    pub cell: f32,
    pub amount: f32,
    pub clump: f32,
    pub shadows: f32,
    pub mids: f32,
    pub highlights: f32,
    pub grain_base: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct GrainUniform {
    width: u32,
    height: u32,
    seed: u32,
    mono: u32,
    cell: f32,
    amount: f32,
    clump: f32,
    shadows: f32,
    mids: f32,
    highlights: f32,
    grain_base: f32,
    _pad: f32,
}

pub fn grain_rgba(
    buf: &mut [f32],
    width: u32,
    height: u32,
    p: GrainParams,
) -> Result<(), GpuError> {
    if buf.len() != width as usize * height as usize * 4 {
        return Err(GpuError::Device);
    }
    let result = pollster::block_on(run_grain(buf, width, height, p))?;
    buf.copy_from_slice(&result);
    Ok(())
}

async fn run_grain(
    src: &[f32],
    width: u32,
    height: u32,
    p: GrainParams,
) -> Result<Vec<f32>, GpuError> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::METAL,
        ..Default::default()
    });
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        })
        .await
        .map_err(|_| GpuError::NoAdapter)?;
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("onmk Film grain"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            memory_hints: wgpu::MemoryHints::Performance,
            trace: wgpu::Trace::Off,
        })
        .await
        .map_err(|_| GpuError::Device)?;
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("grain"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("grain.wgsl"))),
    });
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            entry(0, false),
            wgpu::BindGroupLayoutEntry {
                binding: 1,
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
        label: None,
        bind_group_layouts: &[&bgl],
        push_constant_ranges: &[],
    });
    let pipe = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: Some(&layout),
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let data = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: slice_bytes(src),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    });
    let u = GrainUniform {
        width,
        height,
        seed: p.seed,
        mono: p.mono,
        cell: p.cell,
        amount: p.amount,
        clump: p.clump,
        shadows: p.shadows,
        mids: p.mids,
        highlights: p.highlights,
        grain_base: p.grain_base,
        _pad: 0.0,
    };
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytes(&u),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: data.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: uniform.as_entire_binding(),
            },
        ],
    });
    let size = std::mem::size_of_val(src) as u64;
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut enc = device.create_command_encoder(&Default::default());
    {
        let mut pass = enc.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipe);
        pass.set_bind_group(0, &bg, &[]);
        pass.dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1);
    }
    enc.copy_buffer_to_buffer(&data, 0, &staging, 0, size);
    queue.submit(Some(enc.finish()));
    let slice = staging.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    let _ = device.poll(wgpu::PollType::Wait);
    rx.recv()
        .map_err(|_| GpuError::Map)?
        .map_err(|_| GpuError::Map)?;
    let mapped = slice.get_mapped_range();
    Ok(mapped
        .chunks_exact(4)
        .map(|b| f32::from_ne_bytes(b.try_into().unwrap()))
        .collect())
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Params {
    width: u32,
    height: u32,
    radius: u32,
    horizontal: u32,
}

fn bytes<T>(value: &T) -> &[u8] {
    unsafe { std::slice::from_raw_parts(value as *const T as *const u8, std::mem::size_of::<T>()) }
}

fn slice_bytes<T>(value: &[T]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(value.as_ptr() as *const u8, std::mem::size_of_val(value)) }
}

/// Applies a two-pass separable box approximation to an RGBA float image.
pub fn blur_rgba(src: &[f32], width: u32, height: u32, radius: u32) -> Result<Vec<f32>, GpuError> {
    if src.len() != width as usize * height as usize * 4 {
        return Err(GpuError::Device);
    }
    if radius == 0 || width == 0 || height == 0 {
        return Ok(src.to_vec());
    }
    pollster::block_on(run(src, width, height, radius.min(128)))
}

async fn run(src: &[f32], width: u32, height: u32, radius: u32) -> Result<Vec<f32>, GpuError> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::METAL,
        ..Default::default()
    });
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        })
        .await
        .map_err(|_| GpuError::NoAdapter)?;
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("onmk Film"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            memory_hints: wgpu::MemoryHints::Performance,
            trace: wgpu::Trace::Off,
        })
        .await
        .map_err(|_| GpuError::Device)?;
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("onmk Film blur"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("blur.wgsl"))),
    });
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            entry(0, true),
            entry(1, false),
            wgpu::BindGroupLayoutEntry {
                binding: 2,
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
        label: None,
        bind_group_layouts: &[&bgl],
        push_constant_ranges: &[],
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: Some(&layout),
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let size = std::mem::size_of_val(src) as u64;
    let a = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: slice_bytes(src),
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_SRC
            | wgpu::BufferUsages::COPY_DST,
    });
    let b = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size,
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_SRC
            | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    for _ in 0..3 {
        for (horizontal, input, output) in [(1, &a, &b), (0, &b, &a)] {
            let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytes(&Params {
                    width,
                    height,
                    radius,
                    horizontal,
                }),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: input.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: output.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: uniform.as_entire_binding(),
                    },
                ],
            });
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bg, &[]);
            pass.dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1);
        }
    }
    encoder.copy_buffer_to_buffer(&a, 0, &staging, 0, size);
    queue.submit(Some(encoder.finish()));
    let slice = staging.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    let _ = device.poll(wgpu::PollType::Wait);
    rx.recv()
        .map_err(|_| GpuError::Map)?
        .map_err(|_| GpuError::Map)?;
    let mapped = slice.get_mapped_range();
    let out = mapped
        .chunks_exact(4)
        .map(|b| f32::from_ne_bytes(b.try_into().unwrap()))
        .collect();
    drop(mapped);
    staging.unmap();
    Ok(out)
}

fn entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn gpu_blur_matches_cpu_reference() {
        let (w, h) = (31usize, 23usize);
        let mut source = vec![0.0f32; w * h * 4];
        for pixel in source.chunks_exact_mut(4) {
            pixel[3] = 1.0;
        }
        let center = ((h / 2) * w + w / 2) * 4;
        source[center..center + 3].fill(1.0);
        let mut cpu = vec![0.0; source.len()];
        onmk_film_core::blur_cpu(&source, &mut cpu, w, h, 8.0, 1.0);
        let gpu = blur_rgba(&source, w as u32, h as u32, 4).expect("Metal blur");
        let mae = cpu
            .iter()
            .zip(&gpu)
            .map(|(a, b)| (a - b).abs())
            .sum::<f32>()
            / cpu.len() as f32;
        assert!(mae < 1.0e-5, "CPU/GPU MAE {mae}");
    }

    #[test]
    fn gpu_grain_is_deterministic_for_static_seed() {
        let mut a = vec![0.25f32; 32 * 24 * 4];
        for pixel in a.chunks_exact_mut(4) {
            pixel[3] = 1.0;
        }
        let mut b = a.clone();
        let p = GrainParams {
            seed: 42,
            mono: 0,
            cell: 3.5,
            amount: 0.5,
            clump: 0.4,
            shadows: 1.2,
            mids: 1.0,
            highlights: 0.55,
            grain_base: 0.85,
        };
        grain_rgba(&mut a, 32, 24, p).expect("Metal grain");
        grain_rgba(&mut b, 32, 24, p).expect("Metal grain");
        assert_eq!(a, b);
        assert!(a
            .iter()
            .zip(std::iter::repeat(0.25))
            .any(|(x, y)| (*x - y).abs() > 1.0e-5));
    }
}
