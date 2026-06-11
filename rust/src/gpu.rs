use crate::RefractParams;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;
use wgpu::*;

#[repr(C, align(16))]
#[derive(Clone, Copy)]
struct GpuParams {
    size: [u32; 4],
    mode: [u32; 4],
    optical0: [f32; 4],
    optical1: [f32; 4],
    optical2: [f32; 4],
    color: [f32; 4],
    light: [f32; 4],
    half_vec: [f32; 4],
    // x = height_source, y = height_invert (0/1), z = coverage_source, w = unused
    mode2: [u32; 4],
}

#[allow(dead_code)]
struct BufferState {
    width: u32,
    height: u32,
    source_buf: Buffer,
    mask_buf: Buffer,
    bg_buf: Buffer,
    height_a: Buffer,
    height_b: Buffer,
    mask_a: Buffer,
    comp_mask: Buffer,
    tmp_plane: Buffer,
    rgb_a: Buffer,
    rgb_b: Buffer,
    output_buf: Buffer,
    staging_buf: Buffer,
    params_buf: Buffer,
    bind_group: BindGroup,
    last_access: AtomicUsize,
}

struct GpuProcessor {
    device: Device,
    queue: Queue,
    init_pipeline: ComputePipeline,
    copy_mask_pipeline: ComputePipeline,
    blur_height_h_pipeline: ComputePipeline,
    blur_height_v_pipeline: ComputePipeline,
    blur_mask_h_pipeline: ComputePipeline,
    blur_mask_v_pipeline: ComputePipeline,
    render_pipeline: ComputePipeline,
    init_rgb_pipeline: ComputePipeline,
    blur_rgb_h_pipeline: ComputePipeline,
    blur_rgb_v_pipeline: ComputePipeline,
    apply_rgb_blur_pipeline: ComputePipeline,
    output_mode_pipeline: ComputePipeline,
    bind_group_layout: BindGroupLayout,
    state: RwLock<HashMap<std::thread::ThreadId, BufferState>>,
}

static GPU: OnceLock<Option<GpuProcessor>> = OnceLock::new();

pub fn available() -> bool {
    processor().is_some()
}

pub fn render(
    params: &RefractParams,
    src: &[u8],
    mask: Option<&[u8]>,
    bg: Option<&[u8]>,
    w: usize,
    h: usize,
) -> Option<Vec<u8>> {
    processor()?.process(params, src, mask.unwrap_or(src), bg.unwrap_or(src), w, h)
}

fn processor() -> Option<&'static GpuProcessor> {
    GPU.get_or_init(|| GpuProcessor::new().ok()).as_ref()
}

impl GpuProcessor {
    fn new() -> Result<Self, String> {
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
        .map_err(|_| "No GPU adapter found".to_string())?;

        let (device, queue) = pollster::block_on(adapter.request_device(&DeviceDescriptor {
            label: None,
            required_features: Features::empty(),
            required_limits: adapter.limits(),
            memory_hints: MemoryHints::Performance,
            trace: Trace::Off,
        }))
        .map_err(|e| format!("Failed to create GPU device: {e}"))?;

        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("refraction_dispersion_wgpu"),
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
                bgl_storage_rw(5),
                bgl_storage_rw(6),
                bgl_storage_rw(7),
                bgl_storage_rw(8),
                bgl_storage_rw(9),
                bgl_storage_rw(10),
                bgl_storage_rw(11),
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let mk = |entry: &str| {
            device.create_compute_pipeline(&ComputePipelineDescriptor {
                module: &shader,
                entry_point: Some(entry),
                label: Some(entry),
                layout: Some(&pipeline_layout),
                compilation_options: Default::default(),
                cache: Default::default(),
            })
        };

        Ok(Self {
            init_pipeline: mk("init_planes"),
            copy_mask_pipeline: mk("copy_mask"),
            blur_height_h_pipeline: mk("blur_height_h"),
            blur_height_v_pipeline: mk("blur_height_v"),
            blur_mask_h_pipeline: mk("blur_mask_h"),
            blur_mask_v_pipeline: mk("blur_mask_v"),
            render_pipeline: mk("render_main"),
            init_rgb_pipeline: mk("init_rgb_blur"),
            blur_rgb_h_pipeline: mk("blur_rgb_h"),
            blur_rgb_v_pipeline: mk("blur_rgb_v"),
            apply_rgb_blur_pipeline: mk("apply_rgb_blur"),
            output_mode_pipeline: mk("apply_output_mode"),
            device,
            queue,
            bind_group_layout,
            state: RwLock::new(HashMap::new()),
        })
    }

    fn process(
        &self,
        rp: &RefractParams,
        src: &[u8],
        mask: &[u8],
        bg: &[u8],
        w: usize,
        h: usize,
    ) -> Option<Vec<u8>> {
        if w == 0 || h == 0 || src.len() < w * h * 4 {
            return None;
        }
        if rp.output_mode == 2 {
            return Some(src.to_vec());
        }

        let n = w * h;
        let (ww, hh) = (w as u32, h as u32);
        let lock = self.get_state(ww, hh);
        let state = lock.get(&std::thread::current().id())?;

        let src_packed = pack_argb(src, n);
        let mask_packed = pack_argb(mask, n);
        let bg_packed = pack_argb(bg, n);
        self.queue
            .write_buffer(&state.source_buf, 0, as_bytes_slice(&src_packed));
        self.queue
            .write_buffer(&state.mask_buf, 0, as_bytes_slice(&mask_packed));
        self.queue
            .write_buffer(&state.bg_buf, 0, as_bytes_slice(&bg_packed));

        let mut params = make_params(rp, ww, hh);
        let wg_x = (ww + 15) / 16;
        let wg_y = (hh + 15) / 16;

        self.dispatch(state, &self.init_pipeline, &params, wg_x, wg_y);

        for radius in boxes_for_gauss(rp.height_blur) {
            if radius > 0 {
                params.mode[1] = radius as u32;
                self.dispatch(state, &self.blur_height_h_pipeline, &params, wg_x, wg_y);
                self.dispatch(state, &self.blur_height_v_pipeline, &params, wg_x, wg_y);
            }
        }

        if rp.edge_blur >= 0.5 {
            for radius in boxes_for_gauss(rp.edge_blur) {
                params.mode[1] = radius as u32;
                self.dispatch(state, &self.blur_mask_h_pipeline, &params, wg_x, wg_y);
                self.dispatch(state, &self.blur_mask_v_pipeline, &params, wg_x, wg_y);
            }
        } else {
            self.dispatch(state, &self.copy_mask_pipeline, &params, wg_x, wg_y);
        }

        self.dispatch(state, &self.render_pipeline, &params, wg_x, wg_y);

        if rp.affected_blur >= 0.5 {
            self.dispatch(state, &self.init_rgb_pipeline, &params, wg_x, wg_y);
            for radius in boxes_for_gauss(rp.affected_blur) {
                params.mode[1] = radius as u32;
                self.dispatch(state, &self.blur_rgb_h_pipeline, &params, wg_x, wg_y);
                self.dispatch(state, &self.blur_rgb_v_pipeline, &params, wg_x, wg_y);
            }
            self.dispatch(state, &self.apply_rgb_blur_pipeline, &params, wg_x, wg_y);
        }

        if rp.output_mode != 1 {
            self.dispatch(state, &self.output_mode_pipeline, &params, wg_x, wg_y);
        }

        let mut enc = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor { label: None });
        enc.copy_buffer_to_buffer(&state.output_buf, 0, &state.staging_buf, 0, (n * 4) as u64);
        self.queue.submit(Some(enc.finish()));

        let slice = state.staging_buf.slice(..);
        let (tx, rx) = futures_intrusive::channel::shared::oneshot_channel();
        slice.map_async(MapMode::Read, move |v| tx.send(v).unwrap());
        let _ = self.device.poll(PollType::Wait);

        if let Some(Ok(())) = pollster::block_on(rx.receive()) {
            let data = slice.get_mapped_range();
            let packed: &[u32] =
                unsafe { std::slice::from_raw_parts(data.as_ptr() as *const u32, n) };
            let mut result = vec![0u8; n * 4];
            unpack_argb(packed, &mut result);
            drop(data);
            state.staging_buf.unmap();
            Some(result)
        } else {
            None
        }
    }

    fn create_buffers(&self, w: u32, h: u32) -> BufferState {
        let n = (w * h) as u64;
        let mk = |size: u64, usage: BufferUsages| {
            self.device.create_buffer(&BufferDescriptor {
                size,
                usage,
                label: None,
                mapped_at_creation: false,
            })
        };

        let packed = n * 4;
        let plane = n * 4;
        let rgb = n * 16;
        let ro = BufferUsages::STORAGE | BufferUsages::COPY_DST;
        let rw = BufferUsages::STORAGE | BufferUsages::COPY_DST | BufferUsages::COPY_SRC;

        let source_buf = mk(packed, ro);
        let mask_buf = mk(packed, ro);
        let bg_buf = mk(packed, ro);
        let height_a = mk(plane, rw);
        let height_b = mk(plane, rw);
        let mask_a = mk(plane, rw);
        let comp_mask = mk(plane, rw);
        let tmp_plane = mk(plane, rw);
        let rgb_a = mk(rgb, rw);
        let rgb_b = mk(rgb, rw);
        let output_buf = mk(packed, rw);
        let staging_buf = mk(packed, BufferUsages::MAP_READ | BufferUsages::COPY_DST);
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
                    resource: source_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: mask_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: bg_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 4,
                    resource: height_a.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 5,
                    resource: height_b.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 6,
                    resource: mask_a.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 7,
                    resource: comp_mask.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 8,
                    resource: tmp_plane.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 9,
                    resource: rgb_a.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 10,
                    resource: rgb_b.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 11,
                    resource: output_buf.as_entire_binding(),
                },
            ],
        });

        BufferState {
            width: w,
            height: h,
            source_buf,
            mask_buf,
            bg_buf,
            height_a,
            height_b,
            mask_a,
            comp_mask,
            tmp_plane,
            rgb_a,
            rgb_b,
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
    ) -> parking_lot::RwLockUpgradableReadGuard<'_, HashMap<std::thread::ThreadId, BufferState>>
    {
        let mut lock = self.state.upgradable_read();
        let tid = std::thread::current().id();
        if lock
            .get(&tid)
            .map(|s| s.width != w || s.height != h)
            .unwrap_or(true)
        {
            lock.with_upgraded(|map| {
                let max = num_cpus::get().max(2) - 1;
                if map.len() > max {
                    let mut keys: Vec<_> = map
                        .iter()
                        .map(|(k, v)| (*k, v.last_access.load(Ordering::Relaxed)))
                        .collect();
                    keys.sort_by_key(|a| a.1);
                    for (k, _) in keys.iter().take(map.len() - max) {
                        map.remove(k);
                    }
                }
                map.insert(tid, self.create_buffers(w, h));
            });
        }
        lock.get(&tid)
            .unwrap()
            .last_access
            .store(ts(), Ordering::Relaxed);
        lock
    }

    fn dispatch(
        &self,
        state: &BufferState,
        pipeline: &ComputePipeline,
        params: &GpuParams,
        wg_x: u32,
        wg_y: u32,
    ) {
        self.queue
            .write_buffer(&state.params_buf, 0, as_bytes(params));
        let mut enc = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor { label: None });
        {
            let mut cp = enc.begin_compute_pass(&ComputePassDescriptor::default());
            cp.set_pipeline(pipeline);
            cp.set_bind_group(0, &state.bind_group, &[]);
            cp.dispatch_workgroups(wg_x, wg_y, 1);
        }
        self.queue.submit(Some(enc.finish()));
    }
}

fn make_params(rp: &RefractParams, w: u32, h: u32) -> GpuParams {
    let ax = rp.light_angle_x.to_radians();
    let ay = rp.light_angle_y.to_radians();
    let lx = ay.cos() * ax.sin();
    let ly = ay.sin();
    let lz = ay.cos() * ax.cos();
    let (lx, ly, lz) = normalize(lx, ly, lz);
    let (hx, hy, hz) = normalize(lx, ly, 1.0 + lz);

    let (chroma_x, chroma_y) = if rp.use_per_axis_chroma {
        (rp.chromatic_ab_x, rp.chromatic_ab_y)
    } else {
        (rp.chromatic_ab, rp.chromatic_ab)
    };

    GpuParams {
        size: [
            w,
            h,
            rp.samples.clamp(1, 64) as u32,
            if rp.use_6ch { 1 } else { 0 },
        ],
        mode: [
            rp.output_mode.max(1) as u32,
            0,
            rp.ior_mode.max(1) as u32,
            rp.edge_mode.clamp(1, 3) as u32,
        ],
        optical0: [rp.ior_r, rp.ior_g, rp.ior_b, rp.refract_power],
        optical1: [chroma_x, chroma_y, rp.fresnel_power, rp.shininess],
        optical2: [
            rp.diffuseness,
            rp.saturation,
            rp.height_strength,
            rp.height_blur as f32,
        ],
        color: [rp.mix, rp.brightness, rp.contrast, rp.base_ior],
        light: [lx, ly, lz, 0.0],
        half_vec: [hx, hy, hz, 0.0],
        mode2: [
            rp.height_source.clamp(1, 7) as u32,
            if rp.height_invert { 1 } else { 0 },
            rp.coverage_source.clamp(1, 4) as u32,
            0,
        ],
    }
}

fn boxes_for_gauss(sigma: f64) -> [usize; 3] {
    if sigma < 0.5 {
        return [0, 0, 0];
    }

    let n = 3.0f64;
    let w_ideal = ((12.0 * sigma * sigma / n) + 1.0).sqrt();
    let mut wl = w_ideal.floor() as usize;
    if wl % 2 == 0 && wl > 0 {
        wl -= 1;
    }
    if wl == 0 {
        wl = 1;
    }
    let wu = wl + 2;
    let m =
        ((12.0 * sigma * sigma - (n * wl as f64 * wl as f64) - (4.0 * n * wl as f64) - (3.0 * n))
            / (-4.0 * wl as f64 - 4.0))
            .round() as usize;

    let mut sizes = [0usize; 3];
    for i in 0..3 {
        sizes[i] = if i < m { wl } else { wu };
    }
    for s in &mut sizes {
        *s = (*s).max(1) / 2;
    }
    sizes
}

fn pack_argb(src: &[u8], n: usize) -> Vec<u32> {
    (0..n)
        .map(|i| {
            let o = i * 4;
            let a = src[o] as u32;
            let r = src[o + 1] as u32;
            let g = src[o + 2] as u32;
            let b = src[o + 3] as u32;
            (r << 24) | (g << 16) | (b << 8) | a
        })
        .collect()
}

fn unpack_argb(packed: &[u32], dst: &mut [u8]) {
    for (i, &p) in packed.iter().enumerate() {
        let o = i * 4;
        dst[o] = (p & 0xFF) as u8;
        dst[o + 1] = ((p >> 24) & 0xFF) as u8;
        dst[o + 2] = ((p >> 16) & 0xFF) as u8;
        dst[o + 3] = ((p >> 8) & 0xFF) as u8;
    }
}

fn normalize(x: f32, y: f32, z: f32) -> (f32, f32, f32) {
    let m = (x * x + y * y + z * z).sqrt().max(1e-8);
    (x / m, y / m, z / m)
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
        .unwrap()
        .as_millis() as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wgpu_backend_renders_smoke_frame() {
        let gpu = GpuProcessor::new().expect("wgpu processor should initialize");
        let rp = RefractParams {
            ior_r: 1.15,
            ior_g: 1.18,
            ior_b: 1.22,
            ior_mode: 1,
            base_ior: 1.18,
            refract_power: 25.0,
            chromatic_ab: 0.6,
            use_per_axis_chroma: false,
            chromatic_ab_x: 0.6,
            chromatic_ab_y: 0.6,
            samples: 4,
            fresnel_power: 2.0,
            shininess: 40.0,
            diffuseness: 0.08,
            light_angle_x: -45.0,
            light_angle_y: 45.0,
            saturation: 1.0,
            height_strength: 1.0,
            height_blur: 2.0,
            edge_blur: 0.0,
            use_6ch: false,
            map_blur: 0.0,
            height_source: 1,
            height_invert: false,
            coverage_source: 1,
            edge_enable: false,
            edge_width: 12.0,
            edge_rim_blur: 8.0,
            bevel_mode: 1,
            bevel_height: 2.0,
            rim_highlight: 0.0,
            rim_frost: 0.0,
            bg_blur: 0.0,
            mix: 1.0,
            use_gpu: true,
            edge_mode: 1,
            brightness: 0.0,
            contrast: 0.0,
            affected_blur: 0.0,
            output_mode: 1,
        };
        let w = 16usize;
        let h = 16usize;
        let mut src = vec![0u8; w * h * 4];
        let mut mask = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let off = (y * w + x) * 4;
                src[off] = 255;
                src[off + 1] = (x * 16) as u8;
                src[off + 2] = (y * 16) as u8;
                src[off + 3] = 128;
                mask[off] = 255;
                mask[off + 1] = 255;
                mask[off + 2] = 255;
                mask[off + 3] = 255;
            }
        }

        let out = gpu
            .process(&rp, &src, &mask, &src, w, h)
            .expect("wgpu render should return a frame");
        assert_eq!(out.len(), src.len());
    }
}
