//! GPU-resident optical layers. Optional ghost raster/blur stays resident too;
//! f64 ray tracing and ocular generation remain on CPU.
use crate::{bloom_gpu::BloomGpu, physical, source_gpu::SourceGpu, EffectParams};
use wgpu::{util::DeviceExt, *};

pub struct ResidentGpu {
    pub(crate) scratch: parking_lot::Mutex<crate::scratch::Scratch>,
    retry_at: parking_lot::Mutex<Option<std::time::Instant>>,
    pub ghosts: Option<crate::ghost_gpu::GhostGpu>,
    source: SourceGpu,
    bloom: BloomGpu,
    combine: ComputePipeline,
    #[cfg(test)]
    pub frames: std::sync::atomic::AtomicUsize,
    #[cfg(test)]
    pub ghost_frames: std::sync::atomic::AtomicUsize,
    #[cfg(test)]
    pub status_readback_bytes: std::sync::atomic::AtomicUsize,
    #[cfg(test)]
    pub force_failure: std::sync::atomic::AtomicBool,
}
impl ResidentGpu {
    pub fn new(device: &Device) -> Option<Self> {
        let source = SourceGpu::new(device)?;
        let bloom = BloomGpu::new(device)?;
        let ghosts = if std::env::var("ONMK_GPU_GHOSTS").as_deref() == Ok("1") {
            crate::ghost_gpu::GhostGpu::new(device)
        } else {
            None
        };
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
            scratch: parking_lot::Mutex::new(crate::scratch::Scratch::default()),
            retry_at: parking_lot::Mutex::new(None),
            ghosts,
            source,
            bloom,
            combine,
            #[cfg(test)]
            frames: std::sync::atomic::AtomicUsize::new(0),
            #[cfg(test)]
            ghost_frames: std::sync::atomic::AtomicUsize::new(0),
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
        if self
            .retry_at
            .lock()
            .is_some_and(|t| std::time::Instant::now() < t)
        {
            return None;
        }
        let mut pool = self.scratch.lock();
        pool.begin(w, h);
        let mut attempted = false;
        let result = self.render_pooled(
            device,
            queue,
            output,
            ep,
            sources,
            w,
            h,
            cfg,
            &mut pool,
            &mut attempted,
        );
        if result.is_some() {
            pool.finish();
        } else {
            pool.clear();
            if attempted {
                *self.retry_at.lock() =
                    Some(std::time::Instant::now() + std::time::Duration::from_secs(2));
            }
        }
        result
    }

    pub(crate) fn release(&self) {
        self.scratch.lock().clear();
        *self.retry_at.lock() = None;
    }

    fn render_pooled(
        &self,
        device: &Device,
        queue: &Queue,
        output: &Buffer,
        ep: &EffectParams,
        sources: &[physical::BrightSource],
        w: usize,
        h: usize,
        cfg: physical::Config,
        pool: &mut crate::scratch::Scratch,
        attempted: &mut bool,
    ) -> Option<()> {
        let n = w.checked_mul(h)?;
        let bytes = n.checked_mul(16)? as u64;
        let limits = device.limits();
        // Budget seven full-frame buffers, or ten with GPU ghost raster/blur,
        // beyond destination/frame cache and bounded mesh buffers. Guard early.
        if n == 0
            || bytes
                > if self.ghosts.is_some() {
                    384 * 1024 * 1024 / 10
                } else {
                    256 * 1024 * 1024 / 7
                }
            || bytes > limits.max_storage_buffer_binding_size as u64
            || bytes > limits.max_buffer_size
            || w.div_ceil(16) > limits.max_compute_workgroups_per_dimension as usize
            || h.div_ceil(16) > limits.max_compute_workgroups_per_dimension as usize
            || !cfg.bloom_radius.is_finite()
            || !cfg.bloom_strength.is_finite()
            || cfg.bloom_passes > 16
            || !ep.ghost_intensity.is_finite()
            || (self.ghosts.is_some()
                && (!cfg.ghost_blur.is_finite() || cfg.ghost_blur_passes > 16))
            || sources.iter().any(|s| {
                s.rgb
                    .iter()
                    .any(|v| !v.is_finite() || v.is_sign_negative() || *v > 1e20)
            })
        {
            return None;
        }
        let data = crate::source_gpu::parameters(ep, sources, w, h)?;
        *attempted = true;
        #[cfg(test)]
        let timer = std::time::Instant::now();
        device.push_error_scope(ErrorFilter::OutOfMemory);
        device.push_error_scope(ErrorFilter::Validation);
        enum GhostResult {
            Cpu(Vec<[f32; 3]>),
            Mesh(physical::GhostMesh),
        }
        let (source, bloom, ghosts, ocular) = std::thread::scope(|scope| {
            let ghost_job = scope.spawn(|| {
                if ep.physical_enabled {
                    let lens = physical::bundled_lens(ep.lens_preset);
                    // The dense Canon reference exceeded our 2/32768 final
                    // pixel tolerance with GPU rasterization (640x360). Keep
                    // its canonical CPU ghost layer; GPU composition/cache
                    // and the other reference lenses remain enabled.
                    if self.ghosts.is_some() && ep.style_preset != 17 {
                        if let Some(mesh) = physical::ghost_mesh(&lens, sources, w, h, cfg) {
                            return GhostResult::Mesh(mesh);
                        }
                    }
                    GhostResult::Cpu(physical::render(&lens, sources, w, h, cfg))
                } else {
                    GhostResult::Cpu(Vec::new())
                }
            });
            let ocular_job = ep
                .ocular_enabled
                .then(|| scope.spawn(|| crate::ocular::render(ep, sources, w, h)));
            let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
            let source = self
                .source
                .encode_pooled(device, &mut encoder, &data, w, h, Some(pool));
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
                    .encode_pooled(device, &mut encoder, &input_buf, w, h, cfg, Some(pool))
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
                ocular_job
                    .map(|job| job.join().expect("resident ocular worker"))
                    .unwrap_or_default(),
            )
        });
        #[cfg(test)]
        let jobs_time = timer.elapsed();
        let (ghosts, ghost_stride) = match ghosts {
            GhostResult::Cpu(pixels) => (upload(device, &pixels, BufferUsages::STORAGE), 3),
            GhostResult::Mesh(mesh) => {
                let mut encoder =
                    device.create_command_encoder(&CommandEncoderDescriptor::default());
                let buffer = self.ghosts.as_ref().unwrap().encode_pooled(
                    device,
                    &mut encoder,
                    &mesh,
                    w,
                    h,
                    cfg,
                    &self.bloom,
                    Some(pool),
                );
                queue.submit(Some(encoder.finish()));
                (buffer, 4)
            }
        };
        let ocular = upload(device, &ocular, BufferUsages::STORAGE);
        let config = [
            w as u32,
            h as u32,
            (ep.ghost_intensity as f32).to_bits(),
            (if ep.style_preset < 6 { 1.0_f32 } else { 0.0 }).to_bits(),
            u32::from(ep.physical_enabled),
            u32::from(ep.ocular_enabled),
            ghost_stride,
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
        if ghost_stride == 4 {
            self.ghost_frames
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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
