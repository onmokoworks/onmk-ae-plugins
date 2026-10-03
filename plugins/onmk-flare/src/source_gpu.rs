//! Opt-in GPU source optics. The CPU implementation remains the reference and
//! fallback. No AE flags or pixel format contracts are changed by this path.
use crate::{physical::BrightSource, EffectParams};
use wgpu::util::DeviceExt;
use wgpu::*;

pub struct SourceGpu {
    pipeline: ComputePipeline,
    layout: BindGroupLayout,
    // Serialize error scopes and submissions for this optional stage. Buffers
    // are invocation-local, so a failed frame cannot poison a later cache hit.
    guard: parking_lot::Mutex<()>,
    #[cfg(test)]
    pub completed: std::sync::atomic::AtomicUsize,
}

impl SourceGpu {
    pub fn new(device: &Device) -> Option<Self> {
        device.push_error_scope(ErrorFilter::Validation);
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("onmk source optics (experimental)"),
            source: ShaderSource::Wgsl(include_str!("../source_optics.wgsl").into()),
        });
        let pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("onmk source optics"),
            layout: None,
            module: &shader,
            entry_point: Some("source_pass"),
            compilation_options: Default::default(),
            cache: None,
        });
        if pollster::block_on(device.pop_error_scope()).is_some() {
            return None;
        }
        let layout = pipeline.get_bind_group_layout(0);
        Some(Self {
            pipeline,
            layout,
            guard: parking_lot::Mutex::new(()),
            #[cfg(test)]
            completed: std::sync::atomic::AtomicUsize::new(0),
        })
    }

    // Caller owns validation/error scopes and submission. No image readback.
    pub(crate) fn encode(
        &self,
        device: &Device,
        encoder: &mut CommandEncoder,
        data: &[[f32; 4]],
        w: usize,
        h: usize,
    ) -> Buffer {
        self.encode_pooled(device, encoder, data, w, h, None)
    }

    pub(crate) fn encode_pooled(
        &self,
        device: &Device,
        encoder: &mut CommandEncoder,
        data: &[[f32; 4]],
        w: usize,
        h: usize,
        pool: Option<&mut crate::scratch::Scratch>,
    ) -> Buffer {
        let uniform = device.create_buffer_init(&util::BufferInitDescriptor {
            label: Some("source optical parameters"),
            contents: as_bytes(data),
            usage: BufferUsages::UNIFORM,
        });
        let output = crate::scratch::buffer(
            pool,
            device,
            "source",
            (w * h * 16) as u64,
            BufferUsages::STORAGE | BufferUsages::COPY_SRC,
        );
        let bind = device.create_bind_group(&BindGroupDescriptor {
            label: None,
            layout: &self.layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: output.as_entire_binding(),
                },
            ],
        });
        {
            let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor::default());
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups(w.div_ceil(16) as u32, h.div_ceil(16) as u32, 1);
        }
        output
    }

    pub fn render(
        &self,
        device: &Device,
        queue: &Queue,
        ep: &EffectParams,
        sources: &[BrightSource],
        w: usize,
        h: usize,
    ) -> Option<Vec<[f32; 3]>> {
        let n = w.checked_mul(h)?;
        let bytes = n.checked_mul(16)? as u64;
        let limits = device.limits();
        // Bound temporary GPU memory to 128 MiB (output + staging). Large
        // frames and unsupported devices use the established CPU path.
        if n == 0
            || bytes > 64 * 1024 * 1024
            || bytes > limits.max_storage_buffer_binding_size as u64
            || bytes > limits.max_buffer_size
            || w.div_ceil(16) > limits.max_compute_workgroups_per_dimension as usize
            || h.div_ceil(16) > limits.max_compute_workgroups_per_dimension as usize
        {
            return None;
        }
        let data = parameters(ep, sources, w, h)?;
        let _guard = self.guard.lock();
        device.push_error_scope(ErrorFilter::OutOfMemory);
        device.push_error_scope(ErrorFilter::Validation);
        let staging = device.create_buffer(&BufferDescriptor {
            label: None,
            size: bytes,
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
        let output = self.encode(device, &mut encoder, &data, w, h);
        encoder.copy_buffer_to_buffer(&output, 0, &staging, 0, bytes);
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
        let mut result = Vec::with_capacity(n);
        for pixel in mapped.chunks_exact(16) {
            result.push(std::array::from_fn(|c| {
                f32::from_le_bytes(pixel[c * 4..c * 4 + 4].try_into().unwrap())
            }));
        }
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
    // f32 arrays have no padding; lifetime is bounded by the source slice.
    unsafe { std::slice::from_raw_parts(data.as_ptr().cast(), std::mem::size_of_val(data)) }
}

pub(crate) fn parameters(
    ep: &EffectParams,
    sources: &[BrightSource],
    w: usize,
    h: usize,
) -> Option<[[f32; 4]; 27]> {
    let energy = |s: &BrightSource| 0.2126 * s.rgb[0] + 0.7152 * s.rgb[1] + 0.0722 * s.rgb[2];
    if sources.iter().any(|s| {
        !s.angle_x.is_finite() || !s.angle_y.is_finite() || s.rgb.iter().any(|v| !v.is_finite())
    }) {
        return None;
    }
    let mut selected = sources.to_vec();
    selected.sort_by(|a, b| energy(b).total_cmp(&energy(a)));
    selected.truncate(8);
    let mut data = [[0.0; 4]; 27];
    let scale = ep.global_scale.max(0.1) as f32;
    let diffraction = ep.unified_diffraction.clamp(0.0, 1.0) as f32;
    data[0] = [w as f32, h as f32, selected.len() as f32, 0.0];
    data[1] = [
        diffraction,
        scale,
        ep.starburst_blades.clamp(3, 16) as f32,
        (ep.flare_angle as f32).to_radians(),
    ];
    data[2][0] = ep.bloom_strength;
    let fov_h = 50_f64.to_radians();
    let fov_v = 2.0 * ((h as f64 / w as f64) * (fov_h * 0.5).tan()).atan();
    let diagonal = ((w * w + h * h) as f32).sqrt();
    for (i, s) in selected.iter().enumerate() {
        let sx = ((0.5 + s.angle_x / fov_h) * w as f64) as f32;
        let sy = ((0.5 + s.angle_y / fov_v) * h as f64) as f32;
        let e = energy(s).max(0.02);
        data[3 + i * 3] = [sx, sy, e, (2.8 + ep.hotspot_intensity as f32 * 2.4) * e];
        let cx = w as f32 * 0.5 - sx;
        let cy = h as f32 * 0.5 - sy;
        let length = (cx * cx + cy * cy).sqrt().max(1.0);
        data[4 + i * 3] = [
            diagonal * (0.10 + 0.25 * diffraction) * scale.sqrt(),
            diagonal * (0.075 + 0.11 * ep.bloom_radius.max(0.005)),
            cx / length,
            cy / length,
        ];
        data[5 + i * 3] = [s.rgb[0], s.rgb[1], s.rgb[2], 0.0];
    }
    data.iter().flatten().all(|v| v.is_finite()).then_some(data)
}
