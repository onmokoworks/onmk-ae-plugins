//! GPU-resident optical layers. CPU ghost/ocular uploads remain, but no HDR
//! intermediate image is read back or uploaded again after GPU generation.
use crate::{bloom_gpu::BloomGpu, physical, source_gpu::SourceGpu, EffectParams};
use wgpu::{util::DeviceExt, *};

pub struct ResidentGpu {
    source: SourceGpu,
    bloom: BloomGpu,
    combine: ComputePipeline,
    #[cfg(test)]
    pub frames: std::sync::atomic::AtomicUsize,
    #[cfg(test)]
    pub status_readback_bytes: std::sync::atomic::AtomicUsize,
    #[cfg(test)]
    pub force_failure: std::sync::atomic::AtomicBool,
}
impl ResidentGpu {
    pub fn new(device: &Device) -> Option<Self> {
        let source = SourceGpu::new(device)?;
        let bloom = BloomGpu::new(device)?;
        device.push_error_scope(ErrorFilter::Validation);
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("resident optical combination"),
            source: ShaderSource::Wgsl(include_str!("../resident_combine.wgsl").into()),
        });
        let combine = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("resident optical combination"),
            layout: None,
            module: &shader,
            entry_point: Some("combine"),
            compilation_options: Default::default(),
            cache: None,
        });
        if pollster::block_on(device.pop_error_scope()).is_some() {
            return None;
        }
        Some(Self {
            source,
            bloom,
            combine,
            #[cfg(test)]
            frames: std::sync::atomic::AtomicUsize::new(0),
            #[cfg(test)]
            status_readback_bytes: std::sync::atomic::AtomicUsize::new(0),
            #[cfg(test)]
            force_failure: std::sync::atomic::AtomicBool::new(false),
        })
    }

    // GpuProcessor's frame/state lock and optical_gpu_guard protect shared
    // queue operations and device error scopes throughout this call.
    pub fn render(
        &self,
        device: &Device,
        queue: &Queue,
        output: &Buffer,
        ep: &EffectParams,
        sources: &[physical::BrightSource],
        w: usize,
        h: usize,
        cfg: physical::Config,
    ) -> Option<()> {
        let n = w.checked_mul(h)?;
        let bytes = n.checked_mul(16)? as u64;
        let limits = device.limits();
        // At most seven temporary full-frame GPU buffers, beyond the existing
        // destination/frame cache. Guard before any allocations or CPU work.
        if n == 0
            || bytes > 256 * 1024 * 1024 / 7
            || bytes > limits.max_storage_buffer_binding_size as u64
            || bytes > limits.max_buffer_size
            || w.div_ceil(16) > limits.max_compute_workgroups_per_dimension as usize
            || h.div_ceil(16) > limits.max_compute_workgroups_per_dimension as usize
            || !cfg.bloom_radius.is_finite()
            || !cfg.bloom_strength.is_finite()
            || cfg.bloom_passes > 16
            || !ep.ghost_intensity.is_finite()
            || sources.iter().any(|s| {
                s.rgb
                    .iter()
                    .any(|v| !v.is_finite() || v.is_sign_negative() || *v > 1e20)
            })
        {
            return None;
        }
        let data = crate::source_gpu::parameters(ep, sources, w, h)?;
        #[cfg(test)]
        let timer = std::time::Instant::now();
        device.push_error_scope(ErrorFilter::OutOfMemory);
        device.push_error_scope(ErrorFilter::Validation);
        let (source, bloom, ghosts, ocular) = std::thread::scope(|scope| {
            let ghost_job = scope.spawn(|| {
                if ep.physical_enabled {
                    physical::render(&physical::bundled_lens(ep.lens_preset), sources, w, h, cfg)
                } else {
                    Vec::new()
                }
            });
            let ocular_job = scope.spawn(|| {
                if ep.ocular_enabled {
                    crate::ocular::render(ep, sources, w, h)
                } else {
                    Vec::new()
                }
            });
            let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
            let source = self.source.encode(device, &mut encoder, &data, w, h);
            let bloom = if ep.physical_enabled && cfg.bloom_strength > 0.0 {
                // Bloom starts with a handful of impulses, not a dense CPU
                // image. WGPU initializes unwritten buffer bytes to zero.
                let input_buf = device.create_buffer(&BufferDescriptor {
                    label: None,
                    size: bytes,
                    usage: BufferUsages::COPY_SRC | BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let fov_h = 50_f64.to_radians();
                let fov_v = 2.0 * ((h as f64 / w as f64) * (fov_h * 0.5).tan()).atan();
                let mut impulses: Vec<(usize, [f32; 4])> = Vec::new();
                for s in sources {
                    let x = ((s.angle_x / fov_h + 0.5) * w as f64).round() as isize;
                    let y = ((s.angle_y / fov_v + 0.5) * h as f64).round() as isize;
                    if x >= 0 && y >= 0 && x < w as isize && y < h as isize {
                        let index = y as usize * w + x as usize;
                        if let Some((_, p)) = impulses.iter_mut().find(|(i, _)| *i == index) {
                            for c in 0..3 {
                                p[c] += s.rgb[c];
                            }
                        } else {
                            impulses.push((index, [s.rgb[0], s.rgb[1], s.rgb[2], 0.0]));
                        }
                    }
                }
                for (i, p) in impulses {
                    queue.write_buffer(&input_buf, (i * 16) as u64, bytes_of(&p));
                }
                self.bloom
                    .encode(device, &mut encoder, &input_buf, w, h, cfg)
            } else {
                device.create_buffer(&BufferDescriptor {
                    label: None,
                    size: bytes,
                    usage: BufferUsages::STORAGE,
                    mapped_at_creation: false,
                })
            };
            // Submit while the exact CPU ghost/ocular jobs are still running.
            queue.submit(Some(encoder.finish()));
            (
                source,
                bloom,
                ghost_job.join().expect("resident ghost worker"),
                ocular_job.join().expect("resident ocular worker"),
            )
        });
        #[cfg(test)]
        let jobs_time = timer.elapsed();
        let ghosts = upload(device, &ghosts, BufferUsages::STORAGE);
        let ocular = upload(device, &ocular, BufferUsages::STORAGE);
        let config = [
            w as u32,
            h as u32,
            (ep.ghost_intensity as f32).to_bits(),
            (if ep.style_preset < 6 { 1.0_f32 } else { 0.0 }).to_bits(),
            u32::from(ep.physical_enabled),
            u32::from(ep.ocular_enabled),
            0,
            0,
        ];
        let uniform = device.create_buffer_init(&util::BufferInitDescriptor {
            label: None,
            contents: bytes_of(&config),
            usage: BufferUsages::UNIFORM,
        });
        let status = device.create_buffer(&BufferDescriptor {
            label: None,
            size: 4,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = device.create_buffer(&BufferDescriptor {
            label: None,
            size: 4,
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let buffers = [&uniform, &ghosts, &bloom, &ocular, &source, output, &status];
        let entries: Vec<_> = buffers
            .iter()
            .enumerate()
            .map(|(i, b)| BindGroupEntry {
                binding: i as u32,
                resource: b.as_entire_binding(),
            })
            .collect();
        let bind = device.create_bind_group(&BindGroupDescriptor {
            label: None,
            layout: &self.combine.get_bind_group_layout(0),
            entries: &entries,
        });
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor::default());
            pass.set_pipeline(&self.combine);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups(w.div_ceil(16) as u32, h.div_ceil(16) as u32, 1);
        }
        encoder.copy_buffer_to_buffer(&status, 0, &readback, 0, 4);
        queue.submit(Some(encoder.finish()));
        #[cfg(test)]
        let upload_time = timer.elapsed();
        let (tx, rx) = std::sync::mpsc::channel();
        let slice = readback.slice(..);
        slice.map_async(MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        let polled = device.poll(PollType::Wait).is_ok();
        let validation = pollster::block_on(device.pop_error_scope());
        let memory = pollster::block_on(device.pop_error_scope());
        if !polled
            || validation.is_some()
            || memory.is_some()
            || !matches!(
                rx.recv_timeout(std::time::Duration::from_secs(5)),
                Ok(Ok(()))
            )
        {
            return None;
        }
        let mapped = slice.get_mapped_range();
        let ok = u32::from_le_bytes(mapped[..4].try_into().unwrap()) == 0;
        drop(mapped);
        readback.unmap();
        #[cfg(test)]
        self.status_readback_bytes
            .fetch_add(4, std::sync::atomic::Ordering::Relaxed);
        #[cfg(test)]
        if self
            .force_failure
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            return None;
        }
        if !ok {
            return None;
        }
        #[cfg(test)]
        eprintln!(
            "resident stages: jobs={jobs_time:?} submitted={upload_time:?} completed={:?}",
            timer.elapsed()
        );
        #[cfg(test)]
        self.frames
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Some(())
    }
}
fn upload(device: &Device, pixels: &[[f32; 3]], usage: BufferUsages) -> Buffer {
    if pixels.is_empty() {
        return device.create_buffer(&BufferDescriptor {
            label: None,
            size: 4,
            usage,
            mapped_at_creation: false,
        });
    }
    device.create_buffer_init(&util::BufferInitDescriptor {
        label: None,
        contents: bytes_of(pixels),
        usage,
    })
}
fn bytes_of<T>(data: &[T]) -> &[u8] {
    // Only POD arrays/u32 used in this module; no padding or references.
    unsafe { std::slice::from_raw_parts(data.as_ptr().cast(), std::mem::size_of_val(data)) }
}
