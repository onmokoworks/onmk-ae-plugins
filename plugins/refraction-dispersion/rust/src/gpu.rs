use crate::RefractParams;
use crate::refract::Channel;
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
    // x = height_source, y = height_invert (0/1), z = coverage_source, w = native float transport
    mode2: [u32; 4],
}

#[allow(dead_code)]
struct BufferState {
    packed_byte: bool,
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
    cache_budget: u64,
    cache_entries: usize,
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

pub fn render<T: Channel>(
    params: &RefractParams,
    src: &[T],
    mask: Option<&[T]>,
    bg: Option<&[T]>,
    w: usize,
    h: usize,
) -> Option<Vec<T>> {
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
            cache_budget: 1024 * 1024 * 1024,
            cache_entries: 8,
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

    fn process<T: Channel>(
        &self,
        rp: &RefractParams,
        src: &[T],
        mask: &[T],
        bg: &[T],
        w: usize,
        h: usize,
    ) -> Option<Vec<T>> {
        let count = w.checked_mul(h)?.checked_mul(4)?;
        if w == 0 || h == 0 || w > u32::MAX as usize || h > u32::MAX as usize
            || src.len() != count || mask.len() != count || bg.len() != count {
            return None;
        }
        if rp.output_mode == 2 {
            let mut result = Vec::new();
            result.try_reserve_exact(src.len()).ok()?;
            result.extend_from_slice(src);
            return Some(result);
        }

        let n = w * h;
        let (ww, hh) = (w as u32, h as u32);
        let limits = self.device.limits();
        let bytes = n.checked_mul(16)? as u64;
        if bytes > limits.max_buffer_size || bytes > u64::from(limits.max_storage_buffer_binding_size)
            || w.div_ceil(16) > limits.max_compute_workgroups_per_dimension as usize
            || h.div_ceil(16) > limits.max_compute_workgroups_per_dimension as usize { return None; }
        let lock = self.get_state(ww, hh, T::PACKED_BYTE)?;
        let state = lock.get(&std::thread::current().id())?;

        let packed_bytes = n.checked_mul(if T::PACKED_BYTE { 4 } else { 16 })? as u64;
        for (input, buffer) in [(src,&state.source_buf),(mask,&state.mask_buf),(bg,&state.bg_buf)] {
            let packed = pack_argb(input,n)?;
            self.queue.write_buffer(buffer,0,as_bytes_slice(&packed));
            // write_buffer copies into queue-owned staging; release this CPU
            // conversion before preparing the next auxiliary input.
        }

        let mut params = make_params(rp, ww, hh);
        params.mode2[3] = u32::from(!T::PACKED_BYTE);
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
        enc.copy_buffer_to_buffer(&state.output_buf, 0, &state.staging_buf, 0, packed_bytes);
        self.queue.submit(Some(enc.finish()));

        let slice = state.staging_buf.slice(..);
        let (tx, rx) = futures_intrusive::channel::shared::oneshot_channel();
        slice.map_async(MapMode::Read, move |v| { let _ = tx.send(v); });
        let _unmap = UnmapOnDrop(&state.staging_buf);
        if self.device.poll(PollType::Wait).is_err() { return None; }

        if let Some(Ok(())) = pollster::block_on(rx.receive()) {
            let data = slice.get_mapped_range();
            decode_readback(&data,n)
        } else {
            None
        }
    }

    fn create_buffers(&self, w: u32, h: u32, packed_byte: bool) -> BufferState {
        let n = u64::from(w) * u64::from(h);
        let mk = |size: u64, usage: BufferUsages| {
            self.device.create_buffer(&BufferDescriptor {
                size,
                usage,
                label: None,
                mapped_at_creation: false,
            })
        };

        let packed = n * if packed_byte { 4 } else { 16 };
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
            packed_byte,
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
        packed_byte: bool,
    ) -> Option<parking_lot::RwLockUpgradableReadGuard<'_, HashMap<std::thread::ThreadId, BufferState>>>
    {
        let requested = buffer_bytes(w,h,packed_byte)?;
        if requested > self.cache_budget || self.cache_entries == 0 { return None; }
        let mut lock = self.state.upgradable_read();
        let tid = std::thread::current().id();
        if lock
            .get(&tid)
            .map(|s| s.width != w || s.height != h || s.packed_byte != packed_byte)
            .unwrap_or(true)
        {
            lock.with_upgraded(|map| {
                // Drop a replaced frame before allocating its replacement.
                // The write guard excludes all active users of retained states.
                map.remove(&tid);
                let mut retained: u64 = map.values().map(|s| buffer_bytes(s.width,s.height,s.packed_byte).unwrap()).sum();
                while map.len() >= self.cache_entries || retained > self.cache_budget - requested {
                    let oldest = map.iter().min_by_key(|(_,s)|s.last_access.load(Ordering::Relaxed)).map(|(id,_)|*id);
                    let Some(id) = oldest else { break; };
                    let old = map.remove(&id).unwrap();
                    retained -= buffer_bytes(old.width,old.height,old.packed_byte).unwrap();
                }
                map.insert(tid, self.create_buffers(w, h, packed_byte));
            });
        }
        lock.get(&tid)
            .unwrap()
            .last_access
            .store(ts(), Ordering::Relaxed);
        Some(lock)
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

fn buffer_bytes(w: u32, h: u32, packed_byte: bool) -> Option<u64> {
    // Five transport buffers, five scalar planes, two RGBA float planes,
    // and one uniform buffer. Driver/pipeline allocation is not included.
    u64::from(w).checked_mul(u64::from(h))?
        .checked_mul(if packed_byte { 72 } else { 132 })?
        .checked_add(std::mem::size_of::<GpuParams>() as u64)
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

fn pack_argb<T: Channel>(src: &[T], n: usize) -> Option<Vec<u32>> {
    let channels = n.checked_mul(4)?;
    if src.len()!=channels { return None; }
    let mut packed = Vec::new();
    packed.try_reserve_exact(if T::PACKED_BYTE { n } else { channels }).ok()?;
    for p in src.chunks_exact(4) {
        if T::PACKED_BYTE {
            packed.push(((p[1].units() as u32) << 24) | ((p[2].units() as u32) << 16) | ((p[3].units() as u32) << 8) | p[0].units() as u32);
        } else {
            packed.extend([p[1].units().to_bits(),p[2].units().to_bits(),p[3].units().to_bits(),p[0].units().to_bits()]);
        }
    }
    Some(packed)
}

struct UnmapOnDrop<'a>(&'a Buffer);
impl Drop for UnmapOnDrop<'_> {
    fn drop(&mut self) { self.0.unmap(); }
}

fn decode_readback<T: Channel>(bytes: &[u8], pixels: usize) -> Option<Vec<T>> {
    let pixel_bytes = if T::PACKED_BYTE { 4 } else { 16 };
    if bytes.len() != pixels.checked_mul(pixel_bytes)? { return None; }
    let channels = pixels.checked_mul(4)?;
    let mut result = Vec::new();
    result.try_reserve_exact(channels).ok()?;
    result.resize(channels,T::default());
    for (src,dst) in bytes.chunks_exact(pixel_bytes).zip(result.chunks_exact_mut(4)) {
        if T::PACKED_BYTE {
            let p = u32::from_ne_bytes(src.try_into().ok()?);
            for (d,shift) in [0,24,16,8].into_iter().enumerate() {
                dst[d] = T::from_units(((p >> shift) & 255) as f32);
            }
        } else {
            for (d,s) in [3usize,0,1,2].into_iter().enumerate() {
                dst[d] = T::from_units(f32::from_ne_bytes(src[s*4..s*4+4].try_into().ok()?));
            }
        }
    }
    Some(result)
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
    fn readback_decodes_native_channels_and_rejects_wrong_lengths() {
        let byte: Vec<u8> = (0..=255).collect();
        assert_eq!(decode_readback::<u8>(as_bytes_slice(&pack_argb(&byte,64).unwrap()),64).unwrap(),byte);
        let deep: Vec<u16> = (0..=32768).flat_map(|v|[32768,v,32768-v,1]).collect();
        assert_eq!(decode_readback::<u16>(as_bytes_slice(&pack_argb(&deep,32769).unwrap()),32769).unwrap(),deep);
        let hdr = [0.5f32,-0.25,4.125,0.50001];
        let actual = decode_readback::<f32>(as_bytes_slice(&pack_argb(&hdr,1).unwrap()),1).unwrap();
        for (a,b) in actual.iter().zip(hdr) { assert!((a-b).abs()<1e-6); }
        for len in [0,1,3,5,15,17] {
            assert!(decode_readback::<f32>(&vec![0;len],1).is_none());
            if len!=4 { assert!(decode_readback::<u8>(&vec![0;len],1).is_none()); }
        }
        assert!(decode_readback::<f32>(&[],usize::MAX).is_none());
        assert!(decode_readback::<u8>(&[],0).unwrap().is_empty());
        assert!(pack_argb::<u8>(&[],usize::MAX).is_none());
        assert!(pack_argb::<u16>(&[0;3],1).is_none());
        assert!(pack_argb::<f32>(&[0.0;5],1).is_none());
        assert!(pack_argb::<u8>(&[],0).unwrap().is_empty());
    }

    #[test]
    fn retained_gpu_buffers_obey_byte_and_worker_limits() {
        let mut gpu = GpuProcessor::new().expect("actual GPU required");
        // A 5x3 float state plus a 3x2 state exceeds this limit even when
        // the two-entry limit alone would permit both.
        gpu.cache_budget=2500;
        gpu.cache_entries=2;
        let rp = RefractParams { samples:1,coverage_source:3,saturation:1.0,
            ior_r:1.5,ior_g:1.5,ior_b:1.5,base_ior:1.5,
            fresnel_power:2.0,mix:0.0,output_mode:1,..Default::default() };
        std::thread::scope(|scope| {
            for index in 0..12 {
                let gpu=&gpu;
                let rp=&rp;
                scope.spawn(move || {
                    let (w,h) = [(3,2),(5,3),(1,1)][index%3];
                    let source = [0.5f32,-0.25,2.0,0.125].repeat(w*h);
                    assert_eq!(gpu.process(rp,&source,&source,&source,w,h).unwrap(),source);
                    let state = gpu.state.read();
                    assert!(state.len()<=2);
                    let mut total=0;
                    for s in state.values() {
                        let actual = [&s.source_buf,&s.mask_buf,&s.bg_buf,&s.output_buf,&s.staging_buf,
                            &s.height_a,&s.height_b,&s.mask_a,&s.comp_mask,&s.tmp_plane,
                            &s.rgb_a,&s.rgb_b,&s.params_buf].iter().map(|b|b.size()).sum::<u64>();
                        assert_eq!(Some(actual),buffer_bytes(s.width,s.height,s.packed_byte));
                        total+=actual;
                    }
                    assert!(total<=gpu.cache_budget);
                }).join().unwrap();
            }
        });
        let retained=gpu.state.read().len();
        assert!(gpu.get_state(4000,4000,false).is_none());
        assert_eq!(gpu.state.read().len(),retained);
        assert!(buffer_bytes(u32::MAX,u32::MAX,false).is_none());
    }

    #[test]
    fn native_gpu_precision_and_format_switching() {
        let gpu = GpuProcessor::new().expect("actual GPU required");
        let mut rp = RefractParams { samples: 1, coverage_source: 3, saturation: 1.0,
            ior_r: 1.5, ior_g: 1.5, ior_b: 1.5, base_ior: 1.5,
            fresnel_power: 2.0, mix: 0.0, output_mode: 1, ..Default::default() };
        let codes: Vec<u16> = (0..=32768).flat_map(|v| [32768,v,v,v]).collect();
        assert_eq!(gpu.process(&rp,&codes,&codes,&codes,32769,1).unwrap(),codes);
        for (w,h) in [(13,7),(1,1),(17,31),(13,7)] {
            let src: Vec<f32> = (0..w*h).flat_map(|_| [0.375,-0.25,4.125,0.50001]).collect();
            let actual = gpu.process(&rp,&src,&src,&src,w,h).unwrap();
            for (a,b) in actual.iter().zip(&src) { assert!((a-b).abs() < 1e-6); }
            let bytes: Vec<u8> = (0..w*h).flat_map(|i| [255,(i%256) as u8,127,3]).collect();
            assert_eq!(gpu.process(&rp,&bytes,&bytes,&bytes,w,h).unwrap(),bytes);
        }
        // Exercise the effect, not just mix=0. Flat normals sample each native
        // background value without displacement; affected blur keeps low bits.
        rp.mix=1.0;
        rp.affected_blur=2.0;
        let deep: Vec<u16> = (0..15).flat_map(|_| [32768,16385,16387,1]).collect();
        let actual = gpu.process(&rp,&deep,&deep,&deep,5,3).unwrap();
        for (a,b) in actual.iter().zip(&deep) { assert!(a.abs_diff(*b) <= 1, "{a} != {b}"); }
    }

    #[test]
    fn wgpu_backend_renders_smoke_frame() {
        let gpu = GpuProcessor::new().expect("wgpu processor should initialize");
        let mut rp = RefractParams {
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
        // Same-size A/B/A replay: a new processor provides a cold-state
        // reference, so persistent buffers/uniforms cannot validate themselves.
        for (index, mode) in [1,3,4,5].into_iter().enumerate() {
            let mut changed_source = src.clone();
            let mut changed_mask = mask.clone();
            let mut changed_background = src.clone();
            for pixel in 0..w*h {
                let offset = pixel*4;
                changed_source[offset+1] = ((pixel*13+index*7)%256) as u8;
                changed_source[offset+2] = ((pixel*3+71)%256) as u8;
                let coverage = if (pixel/w+pixel%w)%3==0 { 64 } else { 255 };
                changed_mask[offset..offset+4].fill(coverage);
                changed_background[offset+1] = ((pixel*5+37)%256) as u8;
                changed_background[offset+3] = ((pixel*17+9)%256) as u8;
            }
            rp.output_mode=mode;
            rp.map_blur=1.5;
            rp.bg_blur=2.0;
            rp.affected_blur=1.0;
            rp.brightness=0.1;
            rp.mix=0.625;
            let changed = gpu.process(&rp,&changed_source,&changed_mask,&changed_background,w,h).unwrap();
            let cold = GpuProcessor::new().expect("cold GPU reference required");
            let expected = cold.process(&rp,&changed_source,&changed_mask,&changed_background,w,h).unwrap();
            assert_eq!(changed,expected,"warm/cold mismatch in output mode {mode}");
            assert_ne!(changed,out,"state-change fixture must discriminate mode {mode}");
            rp.output_mode=1;
            rp.map_blur=0.0;
            rp.bg_blur=0.0;
            rp.affected_blur=0.0;
            rp.brightness=0.0;
            rp.mix=1.0;
            let restored = gpu.process(&rp,&src,&mask,&src,w,h).unwrap();
            assert_eq!(restored,out,"baseline not restored after mode {mode}");
        }
        // Exercise readback contents and same-device buffer resizing, not just
        // allocation length. Zero effect must preserve every input channel.
        rp.mix = 0.0;
        for (rw, rh) in [(16, 16), (13, 7), (1, 1), (16, 16)] {
            let input = &src[..rw * rh * 4];
            let coverage = &mask[..rw * rh * 4];
            let identity = gpu.process(&rp, input, coverage, input, rw, rh)
                .expect("wgpu resized identity render should return a frame");
            assert_eq!(identity, input, "GPU zero-mix identity at {rw}x{rh}");
        }
    }
}
