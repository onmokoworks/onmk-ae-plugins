//! Experimental full-resolution Bloom. No sample/pass/octave reduction.
use crate::physical::Config;
use wgpu::{util::DeviceExt, *};
pub struct BloomGpu {
    pipelines: Vec<ComputePipeline>,
    layout: BindGroupLayout,
    guard: parking_lot::Mutex<()>,
    #[cfg(test)]
    pub completed: std::sync::atomic::AtomicUsize,
}
impl BloomGpu {
    pub fn new(device: &Device) -> Option<Self> {
        device.push_error_scope(ErrorFilter::Validation);
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("experimental Bloom"),
            source: ShaderSource::Wgsl(include_str!("../bloom.wgsl").into()),
        });
        let entries: Vec<_> = (0..4)
            .map(|binding| BindGroupLayoutEntry {
                binding,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: if binding == 0 {
                        BufferBindingType::Uniform
                    } else {
                        BufferBindingType::Storage { read_only: false }
                    },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            })
            .collect();
        let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: None,
            entries: &entries,
        });
        let pl = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipelines = [
            "prefix_rows",
            "apply_rows",
            "prefix_columns",
            "apply_columns",
            "accumulate",
        ]
        .iter()
        .map(|entry| {
            device.create_compute_pipeline(&ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&pl),
                module: &shader,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        })
        .collect();
        if pollster::block_on(device.pop_error_scope()).is_some() {
            return None;
        }
        Some(Self {
            pipelines,
            layout,
            guard: parking_lot::Mutex::new(()),
            #[cfg(test)]
            completed: std::sync::atomic::AtomicUsize::new(0),
        })
    }
    // Caller validates limits and owns submission/error handling.
    pub(crate) fn encode(
        &self,
        device: &Device,
        encoder: &mut CommandEncoder,
        input_buf: &Buffer,
        w: usize,
        h: usize,
        cfg: Config,
    ) -> Buffer {
        self.encode_pooled(device, encoder, input_buf, w, h, cfg, None)
    }

    pub(crate) fn encode_pooled(
        &self,
        device: &Device,
        encoder: &mut CommandEncoder,
        input_buf: &Buffer,
        w: usize,
        h: usize,
        cfg: Config,
        mut pool: Option<&mut crate::scratch::Scratch>,
    ) -> Buffer {
        let bytes = (w * h * 16) as u64;
        let mut mk =
            |slot, usage| crate::scratch::buffer(pool.as_deref_mut(), device, slot, bytes, usage);
        let working = mk(
            "bloom working",
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        );
        let prefix = mk("bloom prefix", BufferUsages::STORAGE);
        let total = mk(
            "bloom total",
            BufferUsages::STORAGE | BufferUsages::COPY_SRC | BufferUsages::COPY_DST,
        );
        encoder.clear_buffer(&total, 0, None);
        let diag = ((w * w + h * h) as f32).sqrt();
        let chroma = [
            [1.0, 1.0, 1.0, 0.0],
            [1.0, 0.88, 0.60, 0.0],
            [1.0, 0.62, 0.28, 0.0],
            [1.0, 0.35, 0.10, 0.0],
            [0.9, 0.16, 0.04, 0.0],
            [0.7, 0.06, 0.01, 0.0],
        ];
        for octave in 0..cfg.bloom_octaves.clamp(1, 6) {
            // Clamping radius beyond the entire frame changes no window.
            let r = ((cfg.bloom_radius * diag * 2_f32.powi(octave as i32)).max(1.0) as usize)
                .min(w.max(h));
            let tint = if cfg.bloom_chromatic {
                chroma[octave]
            } else {
                [1.0, 1.0, 1.0, 0.0]
            };
            let params = [
                [
                    f32::from_bits(w as u32),
                    f32::from_bits(h as u32),
                    f32::from_bits(r as u32),
                    0.0,
                ],
                tint,
                [
                    cfg.bloom_strength,
                    1.0 / 2_f32.powi(octave as i32),
                    0.0,
                    0.0,
                ],
            ];
            let uniform = device.create_buffer_init(&util::BufferInitDescriptor {
                label: None,
                contents: as_bytes(&params),
                usage: BufferUsages::UNIFORM,
            });
            let buffers = [&uniform, &working, &prefix, &total];
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
                layout: &self.layout,
                entries: &entries,
            });
            encoder.copy_buffer_to_buffer(input_buf, 0, &working, 0, bytes);
            for _ in 0..cfg.bloom_passes.max(1) {
                for stage in 0..4 {
                    let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor::default());
                    pass.set_pipeline(&self.pipelines[stage]);
                    pass.set_bind_group(0, &bind, &[]);
                    match stage {
                        0 => pass.dispatch_workgroups(h.div_ceil(64) as u32, 1, 1),
                        2 => pass.dispatch_workgroups(w.div_ceil(64) as u32, 1, 1),
                        _ => pass.dispatch_workgroups(
                            w.div_ceil(16) as u32,
                            h.div_ceil(16) as u32,
                            1,
                        ),
                    };
                }
            }
            let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor::default());
            pass.set_pipeline(&self.pipelines[4]);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups(w.div_ceil(16) as u32, h.div_ceil(16) as u32, 1);
        }
        total
    }

    /// Ghosts need only the box filter, not an octave accumulator or input copy.
    pub(crate) fn encode_ghost_blur(
        &self,
        device: &Device,
        encoder: &mut CommandEncoder,
        pixels: Buffer,
        w: usize,
        h: usize,
        cfg: Config,
        pool: Option<&mut crate::scratch::Scratch>,
    ) -> Buffer {
        let radius = ((cfg.ghost_blur * ((w * w + h * h) as f32).sqrt()) as usize).min(w.max(h));
        let params = [
            [
                f32::from_bits(w as u32),
                f32::from_bits(h as u32),
                f32::from_bits(radius as u32),
                0.0,
            ],
            [1.0; 4],
            [1.0; 4],
        ];
        let uniform = device.create_buffer_init(&util::BufferInitDescriptor {
            label: None,
            contents: as_bytes(&params),
            usage: BufferUsages::UNIFORM,
        });
        let prefix = crate::scratch::buffer(
            pool,
            device,
            "ghost prefix",
            (w * h * 16) as u64,
            BufferUsages::STORAGE,
        );
        // The shared layout includes an accumulator, but blur stages never use it.
        let unused = device.create_buffer(&BufferDescriptor {
            label: None,
            size: 16,
            usage: BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let buffers = [&uniform, &pixels, &prefix, &unused];
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
            layout: &self.layout,
            entries: &entries,
        });
        for _ in 0..cfg.ghost_blur_passes.max(1) {
            for stage in 0..4 {
                let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor::default());
                pass.set_pipeline(&self.pipelines[stage]);
                pass.set_bind_group(0, &bind, &[]);
                match stage {
                    0 => pass.dispatch_workgroups(h.div_ceil(64) as u32, 1, 1),
                    2 => pass.dispatch_workgroups(w.div_ceil(64) as u32, 1, 1),
                    _ => pass.dispatch_workgroups(w.div_ceil(16) as u32, h.div_ceil(16) as u32, 1),
                }
            }
        }
        pixels
    }

    pub fn render(
        &self,
        device: &Device,
        queue: &Queue,
        input: &[[f32; 3]],
        w: usize,
        h: usize,
        cfg: Config,
    ) -> Option<Vec<[f32; 3]>> {
        let n = w.checked_mul(h)?;
        let bytes = n.checked_mul(16)? as u64;
        // Five frame buffers <= 192 MiB. Fallback before allocating on large
        // frames, exceptional values, or unsupported resource limits.
        let limits = device.limits();
        if n == 0
            || input.len() != n
            || bytes > 192 * 1024 * 1024 / 5
            || bytes > limits.max_storage_buffer_binding_size as u64
            || bytes > limits.max_buffer_size
            || w.div_ceil(16) > limits.max_compute_workgroups_per_dimension as usize
            || h.div_ceil(16) > limits.max_compute_workgroups_per_dimension as usize
            || !cfg.bloom_radius.is_finite()
            || !cfg.bloom_strength.is_finite()
            || cfg.bloom_passes > 16
            || input
                .iter()
                .flatten()
                .any(|v| !v.is_finite() || v.is_sign_negative() || *v > f32::MAX / (2.0 * n as f32))
        {
            return None;
        }
        if cfg.bloom_strength <= 0.0 {
            return Some(vec![[0.0; 3]; n]);
        }
        let data: Vec<[f32; 4]> = input.iter().map(|v| [v[0], v[1], v[2], 0.0]).collect();
        let _guard = self.guard.lock();
        device.push_error_scope(ErrorFilter::OutOfMemory);
        device.push_error_scope(ErrorFilter::Validation);
        let input_buf = device.create_buffer_init(&util::BufferInitDescriptor {
            label: None,
            contents: as_bytes(&data),
            usage: BufferUsages::COPY_SRC,
        });
        let mk = |usage| {
            device.create_buffer(&BufferDescriptor {
                label: None,
                size: bytes,
                usage,
                mapped_at_creation: false,
            })
        };
        let staging = mk(BufferUsages::COPY_DST | BufferUsages::MAP_READ);
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
        let total = self.encode(device, &mut encoder, &input_buf, w, h, cfg);
        encoder.copy_buffer_to_buffer(&total, 0, &staging, 0, bytes);
        queue.submit(Some(encoder.finish()));
        let slice = staging.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
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
        let result: Vec<[f32; 3]> = mapped
            .chunks_exact(16)
            .map(|p| {
                std::array::from_fn(|c| f32::from_le_bytes(p[c * 4..c * 4 + 4].try_into().unwrap()))
            })
            .collect();
        drop(mapped);
        staging.unmap();
        if result.iter().flatten().any(|v| !v.is_finite()) {
            return None;
        }
        #[cfg(test)]
        self.completed
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Some(result)
    }
}
fn as_bytes(data: &[[f32; 4]]) -> &[u8] {
    // Arrays of f32 have no padding.
    unsafe { std::slice::from_raw_parts(data.as_ptr().cast(), std::mem::size_of_val(data)) }
}
