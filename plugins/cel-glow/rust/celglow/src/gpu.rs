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
}

impl GpuProcessor {
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
    ) -> parking_lot::RwLockUpgradableReadGuard<'_, HashMap<std::thread::ThreadId, BufferState>>
    {
        let mut lock = self.state.upgradable_read();
        let tid = std::thread::current().id();
        let stale = lock
            .get(&tid)
            .map(|s| s.out_w != out_w || s.out_h != out_h || s.in_w != in_w || s.in_h != in_h)
            .unwrap_or(true);
        if stale {
            lock.with_upgraded(|x| {
                if x.len() > 8 {
                    let mut keys: Vec<_> = x
                        .iter()
                        .map(|(k, v)| (*k, v.last_access.load(Ordering::Relaxed)))
                        .collect();
                    keys.sort_by_key(|a| a.1);
                    for (k, _) in keys.iter().take(x.len() - 8) {
                        x.remove(k);
                    }
                }
                x.insert(tid, self.create_buffers(out_w, out_h, in_w, in_h));
            });
        }
        lock.get(&tid)
            .unwrap()
            .last_access
            .store(ts(), Ordering::Relaxed);
        lock
    }

    pub(crate) fn process(&self, input: &RenderInput<'_>) -> Option<Vec<f32>> {
        let out_w = input.out_w as u32;
        let out_h = input.out_h as u32;
        let in_w = input.in_w as u32;
        let in_h = input.in_h as u32;
        if out_w == 0 || out_h == 0 || in_w == 0 || in_h == 0 {
            return None;
        }
        let lock = self.get_state(out_w, out_h, in_w, in_h);
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

        let slice = state.staging_buf.slice(..);
        let (tx, rx) = futures_intrusive::channel::shared::oneshot_channel();
        slice.map_async(MapMode::Read, move |v| {
            let _ = tx.send(v);
        });
        let _ = self.device.poll(PollType::Wait);
        match pollster::block_on(rx.receive()) {
            Some(Ok(())) => {
                let data = slice.get_mapped_range();
                let n = input.out_w * input.out_h * 4;
                let out =
                    unsafe { std::slice::from_raw_parts(data.as_ptr() as *const f32, n).to_vec() };
                drop(data);
                state.staging_buf.unmap();
                Some(out)
            }
            _ => None,
        }
    }

    pub(crate) fn process_with_gpu_edt(&self, input: &RenderInput<'_>) -> Option<Vec<f32>> {
        let out_w = input.out_w as u32;
        let out_h = input.out_h as u32;
        let in_w = input.in_w as u32;
        let in_h = input.in_h as u32;
        if out_w == 0 || out_h == 0 || in_w == 0 || in_h == 0 {
            return None;
        }
        let lock = self.get_state(out_w, out_h, in_w, in_h);
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

        let slice = state.staging_buf.slice(..);
        let (tx, rx) = futures_intrusive::channel::shared::oneshot_channel();
        slice.map_async(MapMode::Read, move |v| {
            let _ = tx.send(v);
        });
        let _ = self.device.poll(PollType::Wait);
        match pollster::block_on(rx.receive()) {
            Some(Ok(())) => {
                let data = slice.get_mapped_range();
                let n = input.out_w * input.out_h * 4;
                let out =
                    unsafe { std::slice::from_raw_parts(data.as_ptr() as *const f32, n).to_vec() };
                drop(data);
                state.staging_buf.unmap();
                Some(out)
            }
            _ => None,
        }
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
