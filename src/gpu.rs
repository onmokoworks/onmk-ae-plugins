use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::AtomicUsize;
use wgpu::*;

use crate::SlitScanParams;

#[repr(C, align(16))]
#[derive(Clone, Copy)]
struct GpuParams {
    width: u32,
    height: u32,
    num_frames: u32,
    interpolation: u32,
    mix: f32,
    _pad: [u32; 3],
}

struct BufferState {
    width: u32,
    height: u32,
    max_frames: u32,
    frames_buf: Buffer,
    original_buf: Buffer,
    map_buf: Buffer,
    output_buf: Buffer,
    staging_buf: Buffer,
    params_buf: Buffer,
    bind_group: BindGroup,
    last_access: AtomicUsize,
}

pub struct GpuProcessor {
    device: Device,
    queue: Queue,
    pipeline: ComputePipeline,
    bind_group_layout: BindGroupLayout,
    state: RwLock<HashMap<std::thread::ThreadId, BufferState>>,
}

fn ts() -> usize {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as usize
}

fn bgl_uniform(binding: u32) -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding,
        visibility: ShaderStages::COMPUTE,
        ty: BindingType::Buffer {
            ty: BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn bgl_storage_ro(binding: u32) -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding,
        visibility: ShaderStages::COMPUTE,
        ty: BindingType::Buffer {
            ty: BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn bgl_storage_rw(binding: u32) -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding,
        visibility: ShaderStages::COMPUTE,
        ty: BindingType::Buffer {
            ty: BufferBindingType::Storage { read_only: false },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn as_bytes<T: Copy>(t: &T) -> &[u8] {
    unsafe { std::slice::from_raw_parts(t as *const T as *const u8, std::mem::size_of::<T>()) }
}

fn as_bytes_slice<T: Copy>(s: &[T]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(s.as_ptr() as *const u8, std::mem::size_of_val(s)) }
}

impl GpuProcessor {
    pub fn new() -> Self {
        let mut instance_desc = InstanceDescriptor::default();
        if instance_desc.backends.contains(Backends::DX12)
            && instance_desc.flags.contains(InstanceFlags::VALIDATION)
        {
            instance_desc.backends.remove(Backends::DX12);
        }

        let instance = Instance::new(&instance_desc);
        let adapter = pollster::block_on(instance.request_adapter(&RequestAdapterOptions {
            power_preference: PowerPreference::HighPerformance,
            ..Default::default()
        }))
        .expect("No GPU adapter");

        let (device, queue) = pollster::block_on(adapter.request_device(&DeviceDescriptor {
            label: None,
            required_features: adapter.features(),
            required_limits: adapter.limits(),
            memory_hints: MemoryHints::Performance,
            trace: Trace::Off,
        }))
        .expect("Failed to create GPU device");

        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("timeslice"),
            source: ShaderSource::Wgsl(std::borrow::Cow::Borrowed(include_str!("../shader.wgsl"))),
        });

        let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                bgl_uniform(0),
                bgl_storage_ro(1),
                bgl_storage_ro(2),
                bgl_storage_ro(3),
                bgl_storage_rw(4),
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            module: &shader,
            entry_point: Some("slit_scan_pass"),
            label: None,
            layout: Some(&pipeline_layout),
            compilation_options: Default::default(),
            cache: Default::default(),
        });

        Self {
            device,
            queue,
            pipeline,
            bind_group_layout,
            state: RwLock::new(HashMap::new()),
        }
    }

    fn create_buffers(&self, w: u32, h: u32, max_frames: u32) -> BufferState {
        let n = (w * h) as u64;
        let mk = |sz: u64, usage: BufferUsages| {
            self.device.create_buffer(&BufferDescriptor {
                size: sz,
                usage,
                label: None,
                mapped_at_creation: false,
            })
        };

        let ro = BufferUsages::STORAGE | BufferUsages::COPY_DST;
        let frames_buf = mk(n * 4 * max_frames as u64, ro);
        let original_buf = mk(n * 4, ro);
        let map_buf = mk((n * 4).max(4), ro);
        let output_buf = mk(
            n * 4,
            BufferUsages::STORAGE | BufferUsages::COPY_DST | BufferUsages::COPY_SRC,
        );
        let staging_buf = mk(n * 4, BufferUsages::MAP_READ | BufferUsages::COPY_DST);
        let params_buf = mk(
            std::mem::size_of::<GpuParams>() as u64,
            BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        );

        let bind_group = self.device.create_bind_group(&BindGroupDescriptor {
            label: None,
            layout: &self.bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: params_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: frames_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: original_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: map_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 4,
                    resource: output_buf.as_entire_binding(),
                },
            ],
        });

        BufferState {
            width: w,
            height: h,
            max_frames,
            frames_buf,
            original_buf,
            map_buf,
            output_buf,
            staging_buf,
            params_buf,
            bind_group,
            last_access: AtomicUsize::new(ts()),
        }
    }

    fn get_state(
        &self,
        w: u32,
        h: u32,
        num_frames: u32,
    ) -> parking_lot::RwLockUpgradableReadGuard<'_, HashMap<std::thread::ThreadId, BufferState>>
    {
        let lock = self.state.upgradable_read();
        let tid = std::thread::current().id();

        let needs_recreate = lock
            .get(&tid)
            .map(|s| s.width != w || s.height != h || s.max_frames < num_frames)
            .unwrap_or(true);

        if needs_recreate {
            let mut wlock = parking_lot::RwLockUpgradableReadGuard::upgrade(lock);
            let max = num_cpus::get().max(2) - 1;
            if wlock.len() > max {
                let mut keys: Vec<_> = wlock
                    .iter()
                    .map(|(k, v)| (*k, v.last_access.load(std::sync::atomic::Ordering::Relaxed)))
                    .collect();
                keys.sort_by_key(|a| a.1);
                for (k, _) in keys.iter().take(wlock.len() - max) {
                    wlock.remove(k);
                }
            }
            wlock.insert(tid, self.create_buffers(w, h, num_frames));
            let lock = parking_lot::RwLockWriteGuard::downgrade_to_upgradable(wlock);
            lock.get(&tid)
                .unwrap()
                .last_access
                .store(ts(), std::sync::atomic::Ordering::Relaxed);
            lock
        } else {
            lock.get(&tid)
                .unwrap()
                .last_access
                .store(ts(), std::sync::atomic::Ordering::Relaxed);
            lock
        }
    }

    pub fn process(
        &self,
        sp: &SlitScanParams,
        frames: &[Vec<u8>],
        w: usize,
        h: usize,
        map: &[f64],
        original: &[u8],
    ) -> Vec<u8> {
        let (ww, hh, n) = (w as u32, h as u32, w * h);
        let num_frames = frames.len() as u32;

        let lock = self.get_state(ww, hh, num_frames);
        let state = lock.get(&std::thread::current().id()).unwrap();

        // Upload raw ARGB bytes directly; the shader handles byte order.
        for (fi, frame) in frames.iter().enumerate() {
            let offset = (fi * n * 4) as u64;
            self.queue.write_buffer(&state.frames_buf, offset, frame);
        }

        self.queue.write_buffer(&state.original_buf, 0, original);

        // Convert the map from f64 to f32 for the GPU buffer.
        let map_f32: Vec<f32> = map.iter().map(|&v| v as f32).collect();
        self.queue
            .write_buffer(&state.map_buf, 0, as_bytes_slice(&map_f32));

        let gpu_params = GpuParams {
            width: ww,
            height: hh,
            num_frames,
            interpolation: sp.interpolation as u32,
            mix: sp.mix,
            _pad: [0; 3],
        };
        self.queue
            .write_buffer(&state.params_buf, 0, as_bytes(&gpu_params));

        // Single compute dispatch.
        let wg_x = (ww + 15) / 16;
        let wg_y = (hh + 15) / 16;

        let mut enc = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor { label: None });
        {
            let mut cp = enc.begin_compute_pass(&ComputePassDescriptor::default());
            cp.set_pipeline(&self.pipeline);
            cp.set_bind_group(0, &state.bind_group, &[]);
            cp.dispatch_workgroups(wg_x, wg_y, 1);
        }
        self.queue.submit(Some(enc.finish()));

        // Read back the processed pixels.
        let mut enc = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor { label: None });
        enc.copy_buffer_to_buffer(&state.output_buf, 0, &state.staging_buf, 0, (n * 4) as u64);
        self.queue.submit(Some(enc.finish()));

        let slice = state.staging_buf.slice(..);
        let (tx, rx) = futures_intrusive::channel::shared::oneshot_channel();
        slice.map_async(MapMode::Read, move |v| {
            tx.send(v).unwrap();
        });
        let _ = self.device.poll(PollType::Wait);

        // Bytes are already in ARGB order.
        let mut result = vec![0u8; n * 4];
        if let Some(Ok(())) = pollster::block_on(rx.receive()) {
            let data = slice.get_mapped_range();
            result.copy_from_slice(&data[..n * 4]);
            drop(data);
            state.staging_buf.unmap();
        }
        result
    }
}
