use wgpu::*;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::AtomicUsize;

use crate::EffectParams;
use crate::colormap;

const DIRECTION_ORDER: [(i32, i32, bool); 8] = [
    (0,-1,false), (0,1,false), (-1,0,false), (1,0,false),
    (-1,-1,true), (1,-1,true), (-1,1,true), (1,1,true),
];

/// Must match shader.wgsl Params struct exactly
#[repr(C, align(16))]
#[derive(Clone, Copy)]
struct GpuParams {
    width: u32,
    height: u32,
    input_channel: u32,
    threshold: f32,
    threshold_soft: f32,
    boost_light: f32,
    dx: i32,
    dy: i32,
    step: u32,
    decay: f32,
    shimmer_amount: f32,
    shimmer_detail: f32,
    shimmer_phase: f32,
    source_opacity: f32,
    starglow_opacity: f32,
    transfer_mode: u32,
    cm0: [f32; 4],
    cm1: [f32; 4],
    cm2: [f32; 4],
    cm3: [f32; 4],
    cm4: [f32; 4],
    read_from_a: u32,
    has_map: u32,
    spectrum_mode: u32,
    spectrum_offset: f32,
    spectrum_density: f32,
    spectrum_random: f32,
    _pad: [u32; 2],
}

struct BufferState {
    width: u32,
    height: u32,
    source_buf: Buffer,
    buf_a: Buffer,
    buf_b: Buffer,
    accum_buf: Buffer,
    output_buf: Buffer,
    staging_buf: Buffer,
    params_buf: Buffer,
    map_buf: Buffer,
    bind_group: BindGroup,
    last_access: AtomicUsize,
}

pub struct GpuProcessor {
    device: Device,
    queue: Queue,
    threshold_pipeline: ComputePipeline,
    streak_pipeline: ComputePipeline,
    accumulate_pipeline: ComputePipeline,
    composite_pipeline: ComputePipeline,
    bind_group_layout: BindGroupLayout,
    state: RwLock<HashMap<std::thread::ThreadId, BufferState>>,
}

impl GpuProcessor {
    pub fn new() -> Self {
        let mut instance_desc = InstanceDescriptor::default();
        if instance_desc.backends.contains(Backends::DX12) && instance_desc.flags.contains(InstanceFlags::VALIDATION) {
            instance_desc.backends.remove(Backends::DX12);
        }

        let instance = Instance::new(&instance_desc);
        let adapter = pollster::block_on(
            instance.request_adapter(&RequestAdapterOptions {
                power_preference: PowerPreference::HighPerformance,
                ..Default::default()
            })
        ).expect("No GPU adapter found");

        let (device, queue) = pollster::block_on(
            adapter.request_device(&DeviceDescriptor {
                label: None,
                required_features: adapter.features(),
                required_limits: adapter.limits(),
                memory_hints: MemoryHints::Performance,
                trace: Trace::Off,
            })
        ).expect("Failed to create GPU device");

        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("starglow"),
            source: ShaderSource::Wgsl(std::borrow::Cow::Borrowed(include_str!("../shader.wgsl"))),
        });

        let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                bgl_uniform(0),
                bgl_storage_rw(1), // source
                bgl_storage_rw(2), // buf_a
                bgl_storage_rw(3), // buf_b
                bgl_storage_rw(4), // accum
                bgl_storage_rw(5), // output
                bgl_storage_ro(6), // luma_map
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let mk = |entry: &str| {
            device.create_compute_pipeline(&ComputePipelineDescriptor {
                module: &shader, entry_point: Some(entry), label: None,
                layout: Some(&pipeline_layout),
                compilation_options: Default::default(), cache: Default::default(),
            })
        };

        Self {
            threshold_pipeline: mk("threshold_pass"),
            streak_pipeline: mk("streak_pass"),
            accumulate_pipeline: mk("accumulate_pass"),
            composite_pipeline: mk("composite_pass"),
            device, queue, bind_group_layout,
            state: RwLock::new(HashMap::new()),
        }
    }

    fn create_buffers(&self, w: u32, h: u32) -> BufferState {
        let n = (w * h) as u64;
        let mk = |sz: u64, usage: BufferUsages| {
            self.device.create_buffer(&BufferDescriptor { size: sz, usage, label: None, mapped_at_creation: false })
        };

        let rw = BufferUsages::STORAGE | BufferUsages::COPY_DST;
        let source_buf = mk(n * 4, rw | BufferUsages::COPY_SRC);
        let buf_a = mk(n * 16, rw);
        let buf_b = mk(n * 16, rw);
        let accum_buf = mk(n * 16, rw);
        let output_buf = mk(n * 4, rw | BufferUsages::COPY_SRC);
        let staging_buf = mk(n * 4, BufferUsages::MAP_READ | BufferUsages::COPY_DST);
        let params_buf = mk(std::mem::size_of::<GpuParams>() as u64, BufferUsages::UNIFORM | BufferUsages::COPY_DST);
        let map_buf = mk((n * 4).max(4), BufferUsages::STORAGE | BufferUsages::COPY_DST); // f32 per pixel

        let bind_group = self.device.create_bind_group(&BindGroupDescriptor {
            label: None, layout: &self.bind_group_layout,
            entries: &[
                BindGroupEntry { binding: 0, resource: params_buf.as_entire_binding() },
                BindGroupEntry { binding: 1, resource: source_buf.as_entire_binding() },
                BindGroupEntry { binding: 2, resource: buf_a.as_entire_binding() },
                BindGroupEntry { binding: 3, resource: buf_b.as_entire_binding() },
                BindGroupEntry { binding: 4, resource: accum_buf.as_entire_binding() },
                BindGroupEntry { binding: 5, resource: output_buf.as_entire_binding() },
                BindGroupEntry { binding: 6, resource: map_buf.as_entire_binding() },
            ],
        });

        BufferState {
            width: w, height: h,
            source_buf, buf_a, buf_b, accum_buf, output_buf, staging_buf, params_buf, map_buf,
            bind_group, last_access: AtomicUsize::new(ts()),
        }
    }

    fn get_state(&self, w: u32, h: u32) -> parking_lot::RwLockUpgradableReadGuard<'_, HashMap<std::thread::ThreadId, BufferState>> {
        let mut lock = self.state.upgradable_read();
        let tid = std::thread::current().id();
        if lock.get(&tid).map(|s| s.width != w || s.height != h).unwrap_or(true) {
            lock.with_upgraded(|x| {
                let max = num_cpus::get().max(2) - 1;
                if x.len() > max {
                    let mut keys: Vec<_> = x.iter().map(|(k, v)| (*k, v.last_access.load(std::sync::atomic::Ordering::Relaxed))).collect();
                    keys.sort_by_key(|a| a.1);
                    for (k, _) in keys.iter().take(x.len() - max) { x.remove(k); }
                }
                x.insert(tid, self.create_buffers(w, h));
            });
        }
        lock.get(&tid).unwrap().last_access.store(ts(), std::sync::atomic::Ordering::Relaxed);
        lock
    }

    /// Submit one compute dispatch. Each call does write_buffer + encode + submit,
    /// ensuring the GPU sees correct params for every pass.
    fn dispatch(&self, state: &BufferState, pipeline: &ComputePipeline, p: &GpuParams, wg_x: u32, wg_y: u32) {
        self.queue.write_buffer(&state.params_buf, 0, as_bytes(p));
        let mut enc = self.device.create_command_encoder(&CommandEncoderDescriptor { label: None });
        {
            let mut cp = enc.begin_compute_pass(&ComputePassDescriptor::default());
            cp.set_pipeline(pipeline);
            cp.set_bind_group(0, &state.bind_group, &[]);
            cp.dispatch_workgroups(wg_x, wg_y, 1);
        }
        self.queue.submit(Some(enc.finish()));
    }

    /// Process starglow on GPU with feature parity to CPU.
    pub fn process(&self, ep: &EffectParams, src: &[u8], w: usize, h: usize, luma_map: Option<&[f64]>) -> Vec<u8> {
        let (ww, hh, n) = (w as u32, h as u32, w * h);
        let lock = self.get_state(ww, hh);
        let state = lock.get(&std::thread::current().id()).unwrap();

        let cmap = colormap::get_colormap(ep);

        // Upload source (ARGB u8 → packed u32)
        let src_packed: Vec<u32> = (0..n).map(|i| {
            let o = i * 4;
            ((src[o] as u32) << 24) | ((src[o+1] as u32) << 16) | ((src[o+2] as u32) << 8) | (src[o+3] as u32)
        }).collect();
        self.queue.write_buffer(&state.source_buf, 0, as_bytes_slice(&src_packed));

        // Upload map (f64 → f32)
        let has_map = luma_map.is_some();
        if let Some(map) = luma_map {
            let map_f32: Vec<f32> = map.iter().map(|&v| v as f32).collect();
            self.queue.write_buffer(&state.map_buf, 0, as_bytes_slice(&map_f32));
        }

        let wg_x = (ww + 15) / 16;
        let wg_y = (hh + 15) / 16;
        let is_spectrum = ep.colormap_preset == 13;

        let base = GpuParams {
            width: ww, height: hh,
            input_channel: ep.input_channel as u32,
            threshold: ep.threshold as f32,
            threshold_soft: ep.threshold_soft as f32,
            boost_light: ep.boost_light as f32,
            dx: 0, dy: 0, step: 0, decay: 0.0,
            shimmer_amount: ep.shimmer_amount as f32,
            shimmer_detail: ep.shimmer_detail as f32,
            shimmer_phase: ep.shimmer_phase as f32,
            source_opacity: ep.source_opacity as f32,
            starglow_opacity: ep.starglow_opacity as f32,
            transfer_mode: ep.transfer_mode as u32,
            cm0: [cmap[0][0], cmap[0][1], cmap[0][2], 0.0],
            cm1: [cmap[1][0], cmap[1][1], cmap[1][2], 0.0],
            cm2: [cmap[2][0], cmap[2][1], cmap[2][2], 0.0],
            cm3: [cmap[3][0], cmap[3][1], cmap[3][2], 0.0],
            cm4: [cmap[4][0], cmap[4][1], cmap[4][2], 0.0],
            read_from_a: 1,
            has_map: if has_map { 1 } else { 0 },
            spectrum_mode: if is_spectrum { 1 } else { 0 },
            spectrum_offset: ep.spectrum_offset as f32,
            spectrum_density: ep.spectrum_density as f32,
            spectrum_random: ep.spectrum_random as f32,
            _pad: [0; 2],
        };

        let weights = shape_weights(ep.glow_shape);

        // Pass 1: Threshold (first call clears accumulator)
        self.dispatch(state, &self.threshold_pipeline, &base, wg_x, wg_y);

        // Pass 2: Streak each active direction
        for (dir_idx, &(dx, dy, is_diag)) in DIRECTION_ORDER.iter().enumerate() {
            let shape_w = if ep.glow_shape == 9 { 1.0 } else { weights[dir_idx] };
            let indiv = ep.lengths[dir_idx];
            let total_weight = shape_w * indiv;
            if total_weight <= 0.0 { continue; }

            let length = (ep.streak_length * total_weight).max(0.0) as usize;
            if length == 0 { continue; }

            // Diagonal correction + decay rate
            let step_dist = if is_diag { std::f64::consts::SQRT_2 } else { 1.0 };
            let eff_len = length as f64 / step_dist;
            let decay = if eff_len > 1.0 {
                (0.01f64).powf(ep.decay_rate / eff_len) as f32
            } else { 0.5f32 };

            // Re-run threshold to reset buf_a (don't clear accum)
            if dir_idx > 0 || weights[..dir_idx].iter().any(|&w| w > 0.0) {
                let mut tp = base;
                tp.read_from_a = 0; // don't clear accum
                self.dispatch(state, &self.threshold_pipeline, &tp, wg_x, wg_y);
            }

            // Parallel prefix scan: log2(length) passes
            let num_passes = ((length as f32).log2().ceil() as u32).max(1).min(14);
            let mut read_a = true;

            for pass in 0..num_passes {
                let step = 1u32 << pass;
                let mut p = base;
                p.dx = dx; p.dy = dy;
                p.step = step;
                p.decay = decay.powi(step as i32);
                p.read_from_a = if read_a { 1 } else { 0 };
                self.dispatch(state, &self.streak_pipeline, &p, wg_x, wg_y);
                read_a = !read_a;
            }

            // Accumulate
            let mut ap = base;
            ap.read_from_a = if read_a { 1 } else { 0 };
            self.dispatch(state, &self.accumulate_pipeline, &ap, wg_x, wg_y);
        }

        // Pass 3: Composite
        self.dispatch(state, &self.composite_pipeline, &base, wg_x, wg_y);

        // Readback
        let mut enc = self.device.create_command_encoder(&CommandEncoderDescriptor { label: None });
        enc.copy_buffer_to_buffer(&state.output_buf, 0, &state.staging_buf, 0, (n * 4) as u64);
        self.queue.submit(Some(enc.finish()));

        let slice = state.staging_buf.slice(..);
        let (tx, rx) = futures_intrusive::channel::shared::oneshot_channel();
        slice.map_async(MapMode::Read, move |v| tx.send(v).unwrap());
        let _ = self.device.poll(PollType::Wait);

        let mut result = vec![0u8; n * 4];
        if let Some(Ok(())) = pollster::block_on(rx.receive()) {
            let data = slice.get_mapped_range();
            let packed: &[u32] = unsafe { std::slice::from_raw_parts(data.as_ptr() as *const u32, n) };
            for i in 0..n {
                let p = packed[i];
                result[i*4]   = ((p >> 24) & 0xFF) as u8;
                result[i*4+1] = ((p >> 16) & 0xFF) as u8;
                result[i*4+2] = ((p >> 8)  & 0xFF) as u8;
                result[i*4+3] = ( p        & 0xFF) as u8;
            }
            drop(data);
            state.staging_buf.unmap();
        }
        result
    }
}

// Shape presets — mirrors glow.rs
fn shape_weights(shape: i32) -> [f64; 8] {
    match shape {
        1 => [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
        2 => [1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0],
        3 => [0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0],
        4 => [0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0],
        5 => [1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        6 => [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0],
        7 => [0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0],
        8 => [1.0, 1.0, 1.0, 1.0, 0.5, 0.5, 0.5, 0.5],
        9 => [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
        _ => [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
    }
}

// ---- Helpers ----

fn bgl_uniform(binding: u32) -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding, visibility: ShaderStages::COMPUTE, count: None,
        ty: BindingType::Buffer { ty: BufferBindingType::Uniform, has_dynamic_offset: false,
            min_binding_size: BufferSize::new(std::mem::size_of::<GpuParams>() as _) },
    }
}
fn bgl_storage_rw(binding: u32) -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding, visibility: ShaderStages::COMPUTE, count: None,
        ty: BindingType::Buffer { ty: BufferBindingType::Storage { read_only: false }, has_dynamic_offset: false, min_binding_size: None },
    }
}
fn bgl_storage_ro(binding: u32) -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding, visibility: ShaderStages::COMPUTE, count: None,
        ty: BindingType::Buffer { ty: BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None },
    }
}

fn as_bytes<T: Sized>(val: &T) -> &[u8] {
    unsafe { std::slice::from_raw_parts(val as *const T as *const u8, std::mem::size_of::<T>()) }
}
fn as_bytes_slice<T: Sized>(slice: &[T]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(slice.as_ptr() as *const u8, std::mem::size_of_val(slice)) }
}

fn ts() -> usize {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as usize
}
