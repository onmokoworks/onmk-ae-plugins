use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use wgpu::*;

#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct GpuBand {
    pub inner_px: f32,
    pub outer_px: f32,
    pub opacity: f32,
    pub edge_soft_px: f32,
    pub color: [f32; 4],
}

#[repr(C, align(16))]
#[derive(Clone, Copy, Debug)]
struct GpuParams {
    out_w: u32,
    out_h: u32,
    in_w: u32,
    in_h: u32,
    ox0: i32,
    oy0: i32,
    plane_left: i32,
    plane_top: i32,
    copies: u32,
    band_count: u32,
    distribution: u32,
    global_blend: u32,
    preserve_alpha: u32,
    view: u32,
    use_jfa: u32,
    jfa_step: u32,
    read_from_a: u32,
    source_mode: u32,
    _pad0: [u32; 3],
    amount: f32,
    max_d: f32,
    rotation_step: f32,
    scale_step: f32,
    ring_radius: f32,
    line_x: f32,
    line_y: f32,
    copy_opacity_step: f32,
    center_x: f32,
    center_y: f32,
    luma_t: f32,
    luma_s: f32,
    color_tolerance: f32,
    _pad1: [f32; 6],
    source_color: [f32; 4],
    bands: [GpuBand; 8],
}

pub(crate) struct RenderInput<'a> {
    pub rgba_in: &'a [f32],
    pub dist: &'a [f32],
    pub soft: &'a [f32],
    pub out_w: usize,
    pub out_h: usize,
    pub in_w: usize,
    pub in_h: usize,
    pub ox0: i32,
    pub oy0: i32,
    pub plane_left: i32,
    pub plane_top: i32,
    pub copies: usize,
    pub band_count: usize,
    pub distribution: i32,
    pub global_blend: i32,
    pub preserve_alpha: bool,
    pub view: i32,
    pub source_mode: i32,
    pub source_color: [f32; 4],
    pub color_tolerance: f32,
    pub luma_t: f32,
    pub luma_s: f32,
    pub amount: f32,
    pub max_d: f32,
    pub rotation_step: f32,
    pub scale_step: f32,
    pub ring_radius: f32,
    pub line_x: f32,
    pub line_y: f32,
    pub copy_opacity_step: f32,
    pub center_x: f32,
    pub center_y: f32,
    pub bands: [GpuBand; 8],
}

struct BufferState {
    out_w: u32,
    out_h: u32,
    in_w: u32,
    in_h: u32,
    source_buf: Buffer,
    dist_buf: Buffer,
    soft_buf: Buffer,
    output_buf: Buffer,
    staging_buf: Buffer,
    params_buf: Buffer,
    _seed_a_buf: Buffer,
    _seed_b_buf: Buffer,
    bind_group: BindGroup,
    last_access: AtomicUsize,
}

pub(crate) struct GpuProcessor {
    device: Device,
    queue: Queue,
    init_pipeline: ComputePipeline,
    jfa_pipeline: ComputePipeline,
    render_pipeline: ComputePipeline,
    bind_group_layout: BindGroupLayout,
    state: RwLock<HashMap<std::thread::ThreadId, BufferState>>,
    cache_budget: u64,
}

impl GpuProcessor {
    fn readback(&self, staging: &Buffer, channels: usize) -> Option<Vec<f32>> {
        let bytes = channels.checked_mul(4)?;
        if staging.size() != u64::try_from(bytes).ok()? { return None; }
        let mut output = Vec::new();
        output.try_reserve_exact(channels).ok()?;
        with_cleanup(|| {
            let slice = staging.slice(..);
            let (tx, rx) = futures_intrusive::channel::shared::oneshot_channel();
            slice.map_async(MapMode::Read, move |result| { let _ = tx.send(result); });
            self.device.poll(PollType::Wait).ok()?;
            pollster::block_on(rx.receive())?.ok()?;
            let data = slice.get_mapped_range();
            if data.len() != bytes { return None; }
            output.extend(data.chunks_exact(4).map(|chunk|
                f32::from_ne_bytes(chunk.try_into().unwrap())));
            Some(output)
        }, || staging.unmap())
    }

    pub(crate) fn new() -> Self {
        let instance = Instance::new(&InstanceDescriptor::default());
        let adapter = pollster::block_on(instance.request_adapter(&RequestAdapterOptions {
            power_preference: PowerPreference::HighPerformance,
            ..Default::default()
        }))
        .expect("No GPU adapter found");

        let (device, queue) = pollster::block_on(adapter.request_device(&DeviceDescriptor {
            label: None,
            required_features: Features::empty(),
            required_limits: Limits::default(),
            memory_hints: MemoryHints::Performance,
            trace: Trace::Off,
        }))
        .expect("Failed to create GPU device");

        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("celglow"),
            source: ShaderSource::Wgsl(std::borrow::Cow::Borrowed(include_str!("../shader.wgsl"))),
        });
        let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("celglow_bgl"),
            entries: &[
                bgl_uniform(0),
                bgl_storage_ro(1),
                bgl_storage_ro(2),
                bgl_storage_rw(3),
                bgl_storage_rw(4),
                bgl_storage_rw(5),
                bgl_storage_rw(6),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("celglow_pl"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        let mk_pipeline = |entry: &str| {
            device.create_compute_pipeline(&ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: Default::default(),
            })
        };
        let init_pipeline = mk_pipeline("init_mask");
        let jfa_pipeline = mk_pipeline("jfa_pass");
        let render_pipeline = mk_pipeline("render");

        Self {
            device,
            queue,
            init_pipeline,
            jfa_pipeline,
            render_pipeline,
            bind_group_layout,
            state: RwLock::new(HashMap::new()),
            cache_budget: 1024 * 1024 * 1024,
        }
    }

    fn create_buffers(&self, out_w: u32, out_h: u32, in_w: u32, in_h: u32) -> BufferState {
        let out_n = (out_w * out_h) as u64;
        let in_n = (in_w * in_h) as u64;
        let mk = |size: u64, usage: BufferUsages| {
            self.device.create_buffer(&BufferDescriptor {
                label: None,
                size: size.max(4),
                usage,
                mapped_at_creation: false,
            })
        };
        let ro = BufferUsages::STORAGE | BufferUsages::COPY_DST;
        let source_buf = mk(in_n * 16, ro);
        let dist_buf = mk(in_n * 4, ro);
        let soft_buf = mk(in_n * 4, ro);
        let output_buf = mk(out_n * 16, BufferUsages::STORAGE | BufferUsages::COPY_SRC);
        let staging_buf = mk(out_n * 16, BufferUsages::MAP_READ | BufferUsages::COPY_DST);
        let seed_a_buf = mk(in_n * 16, BufferUsages::STORAGE | BufferUsages::COPY_DST);
        let seed_b_buf = mk(in_n * 16, BufferUsages::STORAGE | BufferUsages::COPY_DST);
        let params_buf = mk(
            std::mem::size_of::<GpuParams>() as u64,
            BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        );
        let bind_group = self.device.create_bind_group(&BindGroupDescriptor {
            label: Some("celglow_bg"),
            layout: &self.bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: params_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: source_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: dist_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: soft_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 4,
                    resource: output_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 5,
                    resource: seed_a_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 6,
                    resource: seed_b_buf.as_entire_binding(),
                },
            ],
        });
        BufferState {
            out_w,
            out_h,
            in_w,
            in_h,
            source_buf,
            dist_buf,
            soft_buf,
            output_buf,
            staging_buf,
            params_buf,
            _seed_a_buf: seed_a_buf,
            _seed_b_buf: seed_b_buf,
            bind_group,
            last_access: AtomicUsize::new(ts()),
        }
    }

    fn get_state(
        &self,
        out_w: u32,
        out_h: u32,
        in_w: u32,
        in_h: u32,
    ) -> Option<parking_lot::RwLockUpgradableReadGuard<'_, HashMap<std::thread::ThreadId, BufferState>>>
    {
        let requested = buffer_bytes(out_w, out_h, in_w, in_h)?;
        if requested > self.cache_budget { return None; }
        let mut lock = self.state.upgradable_read();
        let tid = std::thread::current().id();
        let stale = lock
            .get(&tid)
            .map(|s| s.out_w != out_w || s.out_h != out_h || s.in_w != in_w || s.in_h != in_h)
            .unwrap_or(true);
        if stale {
            lock.with_upgraded(|x| {
                // Remove the old resolution before creating its replacement.
                x.remove(&tid);
                let mut retained: u64 = x.values().map(|s|
                    buffer_bytes(s.out_w, s.out_h, s.in_w, s.in_h).unwrap()).sum();
                while x.len() >= 8 || retained > self.cache_budget - requested {
                    let oldest = x.iter().min_by_key(|(_, s)|
                        s.last_access.load(Ordering::Relaxed)).map(|(key, _)| *key);
                    if let Some(key) = oldest {
                        let old = x.remove(&key).unwrap();
                        retained -= buffer_bytes(old.out_w, old.out_h, old.in_w, old.in_h).unwrap();
                    } else { break; }
                }
                x.insert(tid, self.create_buffers(out_w, out_h, in_w, in_h));
            });
        }
        lock.get(&tid)
            .unwrap()
            .last_access
            .store(ts(), Ordering::Relaxed);
        Some(lock)
    }

    pub(crate) fn process(&self, input: &RenderInput<'_>) -> Option<Vec<f32>> {
        let [out_w, out_h, in_w, in_h] = checked_input(input, &self.device.limits(), false)?;
        let lock = self.get_state(out_w, out_h, in_w, in_h)?;
        let state = lock.get(&std::thread::current().id()).unwrap();
        self.queue
            .write_buffer(&state.source_buf, 0, as_bytes_slice(input.rgba_in));
        self.queue
            .write_buffer(&state.dist_buf, 0, as_bytes_slice(input.dist));
        self.queue
            .write_buffer(&state.soft_buf, 0, as_bytes_slice(input.soft));
        let params = params_from_input(input, input.max_d, 0, 0, 1);
        self.queue
            .write_buffer(&state.params_buf, 0, as_bytes(&params));

        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor::default());
            pass.set_pipeline(&self.render_pipeline);
            pass.set_bind_group(0, &state.bind_group, &[]);
            pass.dispatch_workgroups((out_w + 15) / 16, (out_h + 15) / 16, 1);
        }
        encoder.copy_buffer_to_buffer(
            &state.output_buf,
            0,
            &state.staging_buf,
            0,
            (input.out_w * input.out_h * 16) as u64,
        );
        self.queue.submit(Some(encoder.finish()));

        self.readback(&state.staging_buf, input.out_w.checked_mul(input.out_h)?.checked_mul(4)?)
    }

    pub(crate) fn process_with_gpu_edt(&self, input: &RenderInput<'_>) -> Option<Vec<f32>> {
        let [out_w, out_h, in_w, in_h] = checked_input(input, &self.device.limits(), true)?;
        let lock = self.get_state(out_w, out_h, in_w, in_h)?;
        let state = lock.get(&std::thread::current().id()).unwrap();
        self.queue
            .write_buffer(&state.source_buf, 0, as_bytes_slice(input.rgba_in));

        let wg_x = (in_w + 15) / 16;
        let wg_y = (in_h + 15) / 16;
        let out_wg_x = (out_w + 15) / 16;
        let out_wg_y = (out_h + 15) / 16;
        let diag = ((input.in_w * input.in_w + input.in_h * input.in_h) as f32).sqrt();
        let mut params = params_from_input(input, diag.max(1.0), 1, 0, 1);
        self.queue
            .write_buffer(&state.params_buf, 0, as_bytes(&params));

        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor::default());
            pass.set_pipeline(&self.init_pipeline);
            pass.set_bind_group(0, &state.bind_group, &[]);
            pass.dispatch_workgroups(wg_x, wg_y, 1);
        }
        self.queue.submit(Some(encoder.finish()));

        let max_dim = in_w.max(in_h).max(1);
        let mut step = max_dim.next_power_of_two() / 2;
        let mut current_is_a = true;
        while step >= 1 {
            params = params_from_input(input, diag.max(1.0), 1, step, u32::from(current_is_a));
            self.queue
                .write_buffer(&state.params_buf, 0, as_bytes(&params));
            let mut encoder = self
                .device
                .create_command_encoder(&CommandEncoderDescriptor { label: None });
            {
                let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor::default());
                pass.set_pipeline(&self.jfa_pipeline);
                pass.set_bind_group(0, &state.bind_group, &[]);
                pass.dispatch_workgroups(wg_x, wg_y, 1);
            }
            self.queue.submit(Some(encoder.finish()));
            current_is_a = !current_is_a;
            if step == 1 {
                break;
            }
            step /= 2;
        }

        params = params_from_input(input, diag.max(1.0), 1, 0, u32::from(current_is_a));
        self.queue
            .write_buffer(&state.params_buf, 0, as_bytes(&params));
        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor::default());
            pass.set_pipeline(&self.render_pipeline);
            pass.set_bind_group(0, &state.bind_group, &[]);
            pass.dispatch_workgroups(out_wg_x, out_wg_y, 1);
        }
        encoder.copy_buffer_to_buffer(
            &state.output_buf,
            0,
            &state.staging_buf,
            0,
            (input.out_w * input.out_h * 16) as u64,
        );
        self.queue.submit(Some(encoder.finish()));

        self.readback(&state.staging_buf, input.out_w.checked_mul(input.out_h)?.checked_mul(4)?)
    }
}

fn params_from_input(
    input: &RenderInput<'_>,
    max_d: f32,
    use_jfa: u32,
    jfa_step: u32,
    read_from_a: u32,
) -> GpuParams {
    GpuParams {
        out_w: input.out_w as u32,
        out_h: input.out_h as u32,
        in_w: input.in_w as u32,
        in_h: input.in_h as u32,
        ox0: input.ox0,
        oy0: input.oy0,
        plane_left: input.plane_left,
        plane_top: input.plane_top,
        copies: input.copies as u32,
        distribution: input.distribution as u32,
        band_count: input.band_count as u32,
        global_blend: input.global_blend as u32,
        preserve_alpha: u32::from(input.preserve_alpha),
        view: input.view as u32,
        use_jfa,
        jfa_step,
        read_from_a,
        source_mode: input.source_mode as u32,
        _pad0: [0; 3],
        amount: input.amount,
        max_d,
        rotation_step: input.rotation_step,
        scale_step: input.scale_step,
        ring_radius: input.ring_radius,
        line_x: input.line_x,
        line_y: input.line_y,
        copy_opacity_step: input.copy_opacity_step,
        center_x: input.center_x,
        center_y: input.center_y,
        luma_t: input.luma_t,
        luma_s: input.luma_s,
        color_tolerance: input.color_tolerance,
        _pad1: [0.0; 6],
        source_color: input.source_color,
        bands: input.bands,
    }
}

fn bgl_uniform(binding: u32) -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding,
        visibility: ShaderStages::COMPUTE,
        count: None,
        ty: BindingType::Buffer {
            ty: BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: BufferSize::new(std::mem::size_of::<GpuParams>() as _),
        },
    }
}

fn bgl_storage_ro(binding: u32) -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding,
        visibility: ShaderStages::COMPUTE,
        count: None,
        ty: BindingType::Buffer {
            ty: BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
    }
}

fn bgl_storage_rw(binding: u32) -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding,
        visibility: ShaderStages::COMPUTE,
        count: None,
        ty: BindingType::Buffer {
            ty: BufferBindingType::Storage { read_only: false },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
    }
}

fn as_bytes<T: Sized>(val: &T) -> &[u8] {
    unsafe { std::slice::from_raw_parts(val as *const T as *const u8, std::mem::size_of::<T>()) }
}

fn as_bytes_slice<T: Sized>(slice: &[T]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(slice.as_ptr() as *const u8, std::mem::size_of_val(slice)) }
}

fn ts() -> usize {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as usize
}

// Requested buffer bytes, not driver allocation size or VRAM usage.
fn buffer_bytes(out_w: u32, out_h: u32, in_w: u32, in_h: u32) -> Option<u64> {
    let input = u64::from(in_w).checked_mul(u64::from(in_h))?.checked_mul(56)?;
    let output = u64::from(out_w).checked_mul(u64::from(out_h))?.checked_mul(32)?;
    input.checked_add(output)?.checked_add(std::mem::size_of::<GpuParams>() as u64)
}

fn checked_gpu_shape(dims: [usize; 4], limits: &Limits) -> Option<[u32; 4]> {
    let mut result = [0; 4];
    for (value, dest) in dims.into_iter().zip(result.iter_mut()) {
        let value = u32::try_from(value).ok()?;
        if value == 0 || value > i32::MAX as u32 / 2
            || value.div_ceil(16) > limits.max_compute_workgroups_per_dimension {
            return None;
        }
        *dest = value;
    }
    for pair in result.chunks_exact(2) {
        let pixels = u64::from(pair[0]) * u64::from(pair[1]);
        let bytes = pixels.checked_mul(16)?;
        // Shader indexing multiplies pixel indices by four in u32 arithmetic.
        if pixels > u64::from(u32::MAX / 4)
            || bytes > limits.max_buffer_size
            || bytes > u64::from(limits.max_storage_buffer_binding_size)
            || bytes > isize::MAX as u64 { return None; }
    }
    Some(result)
}

fn checked_input(input: &RenderInput<'_>, limits: &Limits, gpu_edt: bool) -> Option<[u32; 4]> {
    let shape = checked_gpu_shape([input.out_w, input.out_h, input.in_w, input.in_h], limits)?;
    let pixels = input.in_w.checked_mul(input.in_h)?;
    if input.rgba_in.len() != pixels.checked_mul(4)?
        || (!gpu_edt && (input.dist.len() != pixels || input.soft.len() != pixels))
        || !(1..=8).contains(&input.band_count)
        || u32::try_from(input.copies).is_err() { return None; }
    for (origin, plane, extent) in [(input.ox0, input.plane_left, input.out_w),
        (input.oy0, input.plane_top, input.out_h)] {
        let last = i64::from(origin) + extent as i64 - 1;
        i32::try_from(last).ok()?;
        i32::try_from(i64::from(origin) - i64::from(plane)).ok()?;
        i32::try_from(last - i64::from(plane)).ok()?;
    }
    Some(shape)
}

// Keep cleanup outside the work closure so mapped views drop before unmapping,
// including when polling, mapping, decoding, or unwinding fails.
fn with_cleanup<T>(work: impl FnOnce() -> T, cleanup: impl FnOnce()) -> T {
    struct Cleanup<F: FnOnce()>(Option<F>);
    impl<F: FnOnce()> Drop for Cleanup<F> {
        fn drop(&mut self) { if let Some(cleanup) = self.0.take() { cleanup(); } }
    }
    let _cleanup = Cleanup(Some(cleanup));
    work()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_byte_accounting_is_checked() {
        assert_eq!(buffer_bytes(2, 3, 5, 7), Some(6 * 32 + 35 * 56 + std::mem::size_of::<GpuParams>() as u64));
        assert!(buffer_bytes(u32::MAX, u32::MAX, 1, 1).is_none());
        assert!(buffer_bytes(1, 1, u32::MAX, u32::MAX).is_none());
    }

    #[test]
    #[ignore = "requires actual GPU buffers; bounded small allocations"]
    fn gpu_cache_respects_budget_and_entry_limit() {
        let mut gpu = GpuProcessor::new();
        let one = buffer_bytes(17, 31, 17, 31).unwrap();
        gpu.cache_budget = one * 2;
        for _ in 0..10 {
            std::thread::scope(|scope| {
                scope.spawn(|| { assert!(gpu.get_state(17, 31, 17, 31).is_some()); }).join().unwrap();
            });
            let cache = gpu.state.read();
            assert!(cache.len() <= 2);
            let actual: u64 = cache.values().map(|s| s.source_buf.size() + s.dist_buf.size()
                + s.soft_buf.size() + s.output_buf.size() + s.staging_buf.size()
                + s.params_buf.size() + s._seed_a_buf.size() + s._seed_b_buf.size()).sum();
            assert!(actual <= gpu.cache_budget);
        }
        assert!(gpu.get_state(100, 100, 100, 100).is_none());
        assert!(gpu.get_state(17, 31, 17, 31).is_some());
        assert!(gpu.get_state(1, 1, 1, 1).is_some());
        gpu.state.write().clear();
        gpu.cache_budget = one * 20;
        for _ in 0..12 {
            std::thread::scope(|scope| {
                scope.spawn(|| { assert!(gpu.get_state(17, 31, 17, 31).is_some()); }).join().unwrap();
            });
            assert!(gpu.state.read().len() <= 8);
        }
        assert_eq!(gpu.state.read().len(), 8);
    }

    #[test]
    fn gpu_shape_rejects_overflow_and_device_limits_without_allocation() {
        let mut limits = Limits::default();
        assert_eq!(checked_gpu_shape([17, 31, 1, 1], &limits), Some([17, 31, 1, 1]));
        for dims in [[0, 1, 1, 1], [usize::MAX, 1, 1, 1],
            [65536, 65536, 1, 1], [1, 1, 65536, 65536]] {
            assert!(checked_gpu_shape(dims, &limits).is_none());
        }
        limits.max_buffer_size = 255;
        assert!(checked_gpu_shape([4, 4, 1, 1], &limits).is_none());
        limits.max_buffer_size = 256;
        limits.max_storage_buffer_binding_size = 255;
        assert!(checked_gpu_shape([1, 1, 4, 4], &limits).is_none());
        limits.max_storage_buffer_binding_size = 256;
        assert!(checked_gpu_shape([4, 4, 4, 4], &limits).is_some());
        limits.max_compute_workgroups_per_dimension = 1;
        assert!(checked_gpu_shape([17, 1, 1, 1], &limits).is_none());
    }

    #[test]
    fn readback_cleanup_runs_on_success_failure_and_unwind() {
        use std::cell::Cell;
        for outcome in 0..3 {
            let calls = Cell::new(0);
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(||
                with_cleanup(|| match outcome {
                    0 => Some(7),
                    1 => None,
                    _ => panic!("simulated readback failure"),
                }, || calls.set(calls.get() + 1))));
            assert_eq!(calls.get(), 1);
            match outcome {
                0 => assert_eq!(result.unwrap(), Some(7)),
                1 => assert_eq!(result.unwrap(), None),
                _ => assert!(result.is_err()),
            }
        }
    }

    #[test]
    #[ignore = "requires an actual GPU adapter; no CPU fallback"]
    fn gpu_single_seed_distance_and_resize() {
        let gpu = GpuProcessor::new();
        // A/B/A at the same dimensions exercises reused buffers, not only
        // replacement allocations during resize.
        for (width, height, alternate) in [(1, 1, false), (1, 31, false),
            (37, 1, false), (17, 31, false), (17, 31, true),
            (17, 31, false), (33, 19, false), (1, 1, false)] {
            let sx = if alternate { width * 2 / 3 } else { width / 3 };
            let sy = height / 2;
            let count = width * height;
            let mut source = vec![0.0; count * 4];
            for pixel in source.chunks_exact_mut(4) { pixel[0] = 1.0; }
            source[(sy * width + sx) * 4 + 1..(sy * width + sx) * 4 + 4].fill(1.0);
            let mut mask = vec![0.0; count];
            mask[sy * width + sx] = 1.0;
            let mut distance = vec![0.0; count];
            crate::edt::euclidean_dt(&mask, width, height, &mut distance).unwrap();
            let max_d = ((width * width + height * height) as f32).sqrt();
            let mut input = RenderInput {
                rgba_in: &source, dist: &distance, soft: &mask,
                out_w: width, out_h: height, in_w: width, in_h: height,
                ox0: 0, oy0: 0, plane_left: 0, plane_top: 0,
                copies: 1, band_count: 1, distribution: 1, global_blend: 1,
                preserve_alpha: true, view: 4, source_mode: 1,
                source_color: [1.0; 4], color_tolerance: 0.0,
                luma_t: 0.5, luma_s: 0.0, amount: 1.0, max_d,
                rotation_step: 0.0, scale_step: 1.0, ring_radius: 0.0,
                line_x: 0.0, line_y: 0.0, copy_opacity_step: 0.0,
                center_x: 0.0, center_y: 0.0, bands: [GpuBand::default(); 8],
            };
            let valid_source = input.rgba_in;
            input.rgba_in = &[];
            assert!(gpu.process(&input).is_none());
            assert!(gpu.process_with_gpu_edt(&input).is_none());
            input.rgba_in = valid_source;
            input.band_count = 0;
            assert!(gpu.process(&input).is_none());
            input.band_count = 9;
            assert!(gpu.process_with_gpu_edt(&input).is_none());
            input.band_count = 1;
            input.ox0 = i32::MAX;
            input.plane_left = -1;
            assert!(gpu.process(&input).is_none());
            input.ox0 = 0;
            input.plane_left = 0;
            let valid_dist = input.dist;
            input.dist = &[];
            assert!(gpu.process(&input).is_none());
            input.dist = valid_dist;
            let cpu_distance_gpu_render = gpu.process(&input).expect("GPU render failed");
            // Updating the normalization parameter must affect this render,
            // and restoring it must recover the original image exactly.
            input.max_d = max_d * 2.0;
            let scaled = gpu.process(&input).expect("GPU parameter update failed");
            for (original, updated) in cpu_distance_gpu_render.chunks_exact(4).zip(scaled.chunks_exact(4)) {
                assert_eq!(original[0], updated[0]);
                for channel in 1..4 {
                    assert!((updated[channel] - original[channel] * 0.5).abs() < 2e-6);
                }
            }
            input.max_d = max_d;
            assert_eq!(gpu.process(&input).expect("GPU parameter restore failed"), cpu_distance_gpu_render);
            let gpu_distance_gpu_render = gpu.process_with_gpu_edt(&input).expect("GPU EDT failed");
            for (label, output) in [("CPU distance", cpu_distance_gpu_render), ("GPU distance", gpu_distance_gpu_render)] {
                assert_eq!(output.len(), count * 4);
                for (i, pixel) in output.chunks_exact(4).enumerate() {
                    let dx = (i % width) as f32 - sx as f32;
                    let dy = (i / width) as f32 - sy as f32;
                    let expected = (dx * dx + dy * dy).sqrt() / max_d;
                    assert_eq!(pixel[0], 1.0);
                    for &value in &pixel[1..] {
                        assert!((value - expected).abs() < 2e-6,
                            "{label} {width}x{height} pixel {i}: {value} != {expected}");
                    }
                }
            }
        }
    }
}
