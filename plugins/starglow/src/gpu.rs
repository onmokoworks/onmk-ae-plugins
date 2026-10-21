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
    packed_byte: bool,
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
    cache_budget: u64,
    cache_entries: usize,
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
            cache_budget: 1024 * 1024 * 1024,
            cache_entries: 8,
            threshold_pipeline: mk("threshold_pass"),
            streak_pipeline: mk("streak_pass"),
            accumulate_pipeline: mk("accumulate_pass"),
            composite_pipeline: mk("composite_pass"),
            device, queue, bind_group_layout,
            state: RwLock::new(HashMap::new()),
        }
    }

    fn create_buffers(&self, w: u32, h: u32, packed_byte: bool) -> BufferState {
        let n = u64::from(w) * u64::from(h);
        let pixel_bytes = if packed_byte { 4 } else { 16 };
        let mk = |sz: u64, usage: BufferUsages| {
            self.device.create_buffer(&BufferDescriptor { size: sz, usage, label: None, mapped_at_creation: false })
        };

        let rw = BufferUsages::STORAGE | BufferUsages::COPY_DST;
        let source_buf = mk(n * pixel_bytes, rw | BufferUsages::COPY_SRC);
        let buf_a = mk(n * 16, rw);
        let buf_b = mk(n * 16, rw);
        let accum_buf = mk(n * 16, rw);
        let output_buf = mk(n * pixel_bytes, rw | BufferUsages::COPY_SRC);
        let staging_buf = mk(n * pixel_bytes, BufferUsages::MAP_READ | BufferUsages::COPY_DST);
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
            packed_byte,
            width: w, height: h,
            source_buf, buf_a, buf_b, accum_buf, output_buf, staging_buf, params_buf, map_buf,
            bind_group, last_access: AtomicUsize::new(ts()),
        }
    }

    fn get_state(&self, w: u32, h: u32, packed_byte: bool) -> Option<parking_lot::RwLockUpgradableReadGuard<'_, HashMap<std::thread::ThreadId, BufferState>>> {
        let requested=buffer_bytes(w,h,packed_byte)?;
        if requested>self.cache_budget || self.cache_entries==0 {return None;}
        let mut lock = self.state.upgradable_read();
        let tid = std::thread::current().id();
        if lock.get(&tid).map(|s| s.width != w || s.height != h || s.packed_byte != packed_byte).unwrap_or(true) {
            lock.with_upgraded(|x| {
                x.remove(&tid);
                let mut retained:u64=x.values().map(|s|buffer_bytes(s.width,s.height,s.packed_byte).unwrap()).sum();
                while x.len()>=self.cache_entries || retained>self.cache_budget-requested {
                    let oldest=x.iter().min_by_key(|(_,s)|s.last_access.load(std::sync::atomic::Ordering::Relaxed)).map(|(id,_)|*id);
                    let Some(id)=oldest else {break;};
                    let old=x.remove(&id).unwrap();
                    retained-=buffer_bytes(old.width,old.height,old.packed_byte).unwrap();
                }
                x.insert(tid, self.create_buffers(w, h, packed_byte));
            });
        }
        lock.get(&tid).unwrap().last_access.store(ts(), std::sync::atomic::Ordering::Relaxed);
        Some(lock)
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

    /// GPU-only result. The caller handles CPU fallback explicitly on failure.
    pub fn process_channels<T: crate::glow::Channel>(&self, ep: &EffectParams, src: &[T], w: usize, h: usize, luma_map: Option<&[f64]>) -> Result<Vec<T>, &'static str> {
        let n = w.checked_mul(h).ok_or("pixel count overflow")?;
        if w == 0 || h == 0 || w > u32::MAX as usize || h > u32::MAX as usize
            || n.checked_mul(4) != Some(src.len()) || luma_map.is_some_and(|m| m.len() != n) {
            return Err("invalid image dimensions");
        }
        let limits = self.device.limits();
        let buffer_size = n.checked_mul(16).ok_or("buffer size overflow")? as u64;
        if buffer_size > limits.max_buffer_size || buffer_size > u64::from(limits.max_storage_buffer_binding_size)
            || w.div_ceil(16) > limits.max_compute_workgroups_per_dimension as usize
            || h.div_ceil(16) > limits.max_compute_workgroups_per_dimension as usize {
            return Err("image exceeds GPU limits");
        }
        let (ww, hh, n) = (w as u32, h as u32, w * h);
        let lock = self.get_state(ww, hh, T::PACKED_BYTE).ok_or("GPU cache budget exceeded")?;
        let state = lock.get(&std::thread::current().id()).unwrap();

        let cmap = colormap::get_colormap(ep);

        // Upload source (ARGB u8 → packed u32)
        let src_packed = pack_source(src, n)?;
        let transport_bytes = std::mem::size_of_val(src_packed.as_slice()) as u64;
        self.queue.write_buffer(&state.source_buf, 0, as_bytes_slice(&src_packed));
        drop(src_packed);

        // Upload map (f64 → f32)
        let has_map = luma_map.is_some();
        if let Some(map) = luma_map {
            let mut map_f32 = reserve_vec(map.len())?;
            map_f32.extend(map.iter().map(|&v| v as f32));
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
            _pad: [u32::from(!T::PACKED_BYTE), u32::from(ep.affect_alpha)],
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

            // Visual length sets decay, not a hard cutoff. CPU IIR propagates
            // through the entire image along this direction, including tails.
            let span = if dx == 0 { h } else if dy == 0 { w } else { w.min(h) };
            let num_passes = (usize::BITS - (span - 1).leading_zeros()).max(1);
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
        enc.copy_buffer_to_buffer(&state.output_buf, 0, &state.staging_buf, 0, transport_bytes);
        self.queue.submit(Some(enc.finish()));

        let slice = state.staging_buf.slice(..);
        let (tx, rx) = futures_intrusive::channel::shared::oneshot_channel();
        slice.map_async(MapMode::Read, move |v| { let _ = tx.send(v); });
        let _unmap = UnmapOnDrop(&state.staging_buf);
        if self.device.poll(PollType::Wait).is_err() {
            return Err("GPU poll failed");
        }

        if let Some(Ok(())) = pollster::block_on(rx.receive()) {
            let data = slice.get_mapped_range();
            decode_readback(&data, n)
        } else {
            Err("GPU readback failed")
        }
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

struct UnmapOnDrop<'a>(&'a Buffer);
impl Drop for UnmapOnDrop<'_> {
    fn drop(&mut self) { self.0.unmap(); }
}

fn reserve_vec<T>(count: usize) -> Result<Vec<T>, &'static str> {
    let mut result = Vec::new();
    result.try_reserve_exact(count).map_err(|_| "GPU transport allocation failed")?;
    Ok(result)
}

fn pack_source<T: crate::glow::Channel>(src: &[T], pixels: usize) -> Result<Vec<u32>, &'static str> {
    if pixels.checked_mul(4) != Some(src.len()) { return Err("invalid source length"); }
    let mut result = reserve_vec(if T::PACKED_BYTE { pixels } else { src.len() })?;
    if T::PACKED_BYTE {
        result.extend(src.chunks_exact(4).map(|p| p.iter().fold(0u32, |v,c|
            (v << 8) | (c.normalized_f32() * 255.0).round() as u32)));
    } else {
        result.extend(src.iter().map(|c| c.normalized_f32().to_bits()));
    }
    Ok(result)
}

fn decode_readback<T: crate::glow::Channel>(data: &[u8], pixels: usize) -> Result<Vec<T>, &'static str> {
    let channels = pixels.checked_mul(4).ok_or("channel count overflow")?;
    if pixels.checked_mul(if T::PACKED_BYTE { 4 } else { 16 }) != Some(data.len()) {
        return Err("invalid readback length");
    }
    let mut result = reserve_vec(channels)?;
    result.resize(channels, T::default());
    for (i, bytes) in data.chunks_exact(4).enumerate() {
        let p = u32::from_ne_bytes(bytes.try_into().unwrap());
        if T::PACKED_BYTE {
            for c in 0..4 { result[i*4+c] = T::rgb(((p >> (24-c*8)) & 255) as f32 / 255.0); }
        } else {
            result[i] = if i % 4 == 0 { T::alpha(f32::from_bits(p)) } else { T::rgb(f32::from_bits(p)) };
        }
    }
    Ok(result)
}

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

fn buffer_bytes(w:u32,h:u32,packed_byte:bool)->Option<u64> {
    if w==0 || h==0 {return None;}
    // Three transport buffers, three RGBA float planes, scalar map and uniform.
    u64::from(w).checked_mul(u64::from(h))?
        .checked_mul(if packed_byte {64} else {100})?
        .checked_add(std::mem::size_of::<GpuParams>() as u64)
}

fn ts() -> usize {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_preserves_native_codes_and_rejects_invalid_lengths() {
        let bytes: Vec<u8> = (0..=255u8).flat_map(|v| [v,v,v,v]).collect();
        let packed = pack_source(&bytes,256).unwrap();
        assert_eq!(decode_readback::<u8>(as_bytes_slice(&packed),256).unwrap(),bytes);
        let deep: Vec<u16> = (0..=32768u16).flat_map(|v| [v,v,v,v]).collect();
        let packed = pack_source(&deep,32769).unwrap();
        assert_eq!(decode_readback::<u16>(as_bytes_slice(&packed),32769).unwrap(),deep);
        let hdr = [0.375f32,-0.25,4.125,0.50001];
        let packed = pack_source(&hdr,1).unwrap();
        assert_eq!(decode_readback::<f32>(as_bytes_slice(&packed),1).unwrap(),hdr);
        for length in [0,3,5,15,17] {
            assert!(decode_readback::<f32>(&vec![0;length],1).is_err());
        }
        assert!(decode_readback::<u8>(&[0;5],1).is_err());
        assert!(decode_readback::<u16>(&[],usize::MAX).is_err());
        assert!(pack_source(&[0u8;3],1).is_err());
        assert!(pack_source::<u16>(&[],usize::MAX).is_err());
        assert!(reserve_vec::<u32>(usize::MAX).is_err());
        assert!(decode_readback::<f32>(&[],0).unwrap().is_empty());
    }

    #[test]
    fn cache_bounds_survive_worker_and_format_turnover() {
        let mut gpu=GpuProcessor::new();
        gpu.cache_entries=2;
        gpu.cache_budget=2000;
        let ep=EffectParams {source_opacity:1.0,transfer_mode:1,..Default::default()};
        std::thread::scope(|scope| {
            for i in 0..12 {
                let gpu=&gpu;
                let ep=&ep;
                scope.spawn(move || {
                    let (w,h)=[(3,2),(5,3),(1,1)][i%3];
                    if i%2==0 {
                        let src=[255u8,7,83,211].repeat(w*h);
                        assert_eq!(gpu.process_channels(ep,&src,w,h,None).unwrap(),src);
                    } else {
                        let src=[0.5f32,-0.25,4.125,0.125].repeat(w*h);
                        assert_eq!(gpu.process_channels(ep,&src,w,h,None).unwrap(),src);
                    }
                    let state=gpu.state.read();
                    assert!(state.len()<=gpu.cache_entries);
                    let mut total=0;
                    for s in state.values() {
                        let actual=[&s.source_buf,&s.output_buf,&s.staging_buf,&s.buf_a,&s.buf_b,
                            &s.accum_buf,&s.map_buf,&s.params_buf].iter().map(|b|b.size()).sum::<u64>();
                        assert_eq!(Some(actual),buffer_bytes(s.width,s.height,s.packed_byte));
                        total+=actual;
                    }
                    assert!(total<=gpu.cache_budget);
                }).join().unwrap();
            }
        });
        let count=gpu.state.read().len();
        assert!(gpu.get_state(4000,4000,false).is_none());
        assert_eq!(gpu.state.read().len(),count);
        assert!(buffer_bytes(u32::MAX,u32::MAX,false).is_none());
    }

    #[test]
    fn streak_tails_match_cpu_beyond_visual_length_with_maps() {
        let gpu=GpuProcessor::new();
        let (w,h)=(63,35);
        let mut src=vec![0.0f32;w*h*4];
        for (x,y) in [(3,4),(31,17),(57,30)] {
            src[(y*w+x)*4..(y*w+x)*4+4].copy_from_slice(&[1.0,0.7,0.4,0.2]);
        }
        let map:Vec<f64>=(0..w*h).map(|i|[0.25,0.5,1.0][i%3]).collect();
        let mut ep=EffectParams { source_opacity:0.0,starglow_opacity:1.0,
            boost_light:0.5,input_channel:4,colors:[[1.0;3];5],
            lengths:[1.0;8],decay_rate:0.5,transfer_mode:4,affect_alpha:true,
            ..Default::default() };
        for (amount,phase) in [(0.0,0.0),(0.35,1.75),(0.8,-2.25)] {
        ep.shimmer_amount=amount;
        ep.shimmer_phase=phase;
        ep.shimmer_detail=7.0;
        for shape in [1,3,4,5] {
            ep.glow_shape=shape;
            for length in [5.0,19.0,41.0] {
                ep.streak_length=length;
                for mapped in [false,true] {
                    let luma=if mapped {Some(map.as_slice())} else {None};
                    let actual=gpu.process_channels(&ep,&src,w,h,luma).unwrap();
                    let expected=crate::glow::process_channels(&ep,&src,w,h,luma).unwrap();
                    let error=actual.iter().zip(&expected).map(|(a,b)|(a-b).abs()).fold(0.0f32,f32::max);
                    assert!(error<2e-6,"shimmer={amount}, phase={phase}, shape={shape}, length={length}, mapped={mapped}, error={error}");
                }
            }
        }
        }
    }

    #[test]
    fn colored_gradient_tips_and_shimmer_match_cpu() {
        let gpu=GpuProcessor::new();
        let (w,h)=(17,9);
        let mut src=vec![0.0f32;w*h*4];
        for (x,y) in [(2,2),(8,4),(14,6)] {
            src[(y*w+x)*4..(y*w+x)*4+4].copy_from_slice(&[1.0,0.8,0.4,0.2]);
        }
        let mut ep=EffectParams { source_opacity:0.0,starglow_opacity:1.0,
            boost_light:0.5,input_channel:4,colors:[[1.0,0.0,0.0],[0.0,1.0,0.0],
                [0.0,0.0,1.0],[1.0,1.0,0.0],[0.0,1.0,1.0]],
            glow_shape:1,lengths:[1.0;8],streak_length:19.0,decay_rate:0.5,
            transfer_mode:4,affect_alpha:true,shimmer_detail:7.0,shimmer_phase:1.75,
            ..Default::default() };
        for preset in [1,2,3,5,6,7,8,9,10,11] {
            ep.colormap_preset=preset;
            for shimmer in [0.0,0.35] {
                ep.shimmer_amount=shimmer;
                let actual=gpu.process_channels(&ep,&src,w,h,None).unwrap();
                let expected=crate::glow::process_channels(&ep,&src,w,h,None).unwrap();
                assert!(expected.iter().any(|v|*v>0.01));
                let error=actual.iter().zip(&expected).map(|(a,b)|(a-b).abs()).fold(0.0f32,f32::max);
                assert!(error<2e-6,"preset={preset}, shimmer={shimmer}, error={error}");
            }
        }
        ep.colormap_preset=13;
        for offset in [-0.001,0.0,59.999,60.0,120.0,359.999,360.0,721.0] {
            ep.spectrum_offset=offset;
            for density in [0.0,1.0,2.5] {
                ep.spectrum_density=density;
                for random in [0.0,0.75] {
                    ep.spectrum_random=random;
                    for shimmer in [0.0,0.35] {
                        ep.shimmer_amount=shimmer;
                        let actual=gpu.process_channels(&ep,&src,w,h,None).unwrap();
                        let expected=crate::glow::process_channels(&ep,&src,w,h,None).unwrap();
                        let error=actual.iter().zip(&expected).map(|(a,b)|(a-b).abs()).fold(0.0f32,f32::max);
                        assert!(actual.iter().all(|v|v.is_finite()));
                        assert!(error<2e-6,"spectrum offset={offset}, density={density}, random={random}, shimmer={shimmer}, error={error}");
                    }
                }
            }
        }
    }

    #[test]
    fn native_gpu_identity_and_format_resize() {
        // No CPU fallback: adapter, shader, dispatch, or readback failures fail this test.
        let gpu = GpuProcessor::new();
        let ep = EffectParams { source_opacity: 1.0, transfer_mode: 1, ..Default::default() };
        let codes: Vec<u16> = (0..=32768u16).flat_map(|v| [32768, v, v, v]).collect();
        assert_eq!(gpu.process_channels(&ep, &codes, 32769, 1, None).unwrap(), codes);
        for (w, h) in [(13, 7), (1, 1), (17, 31), (13, 7)] {
            let floats: Vec<f32> = (0..w*h).flat_map(|_| [0.375, -0.25, 4.125, 0.50001]).collect();
            assert_eq!(gpu.process_channels(&ep, &floats, w, h, None).unwrap(), floats);
            let bytes: Vec<u8> = (0..w*h).flat_map(|i| [255, (i % 256) as u8, 127, 3]).collect();
            assert_eq!(gpu.process_channels(&ep, &bytes, w, h, None).unwrap(), bytes);
            let deep: Vec<u16> = (0..w*h).flat_map(|_| [32768, 16385, 16387, 1]).collect();
            assert_eq!(gpu.process_channels(&ep, &deep, w, h, None).unwrap(), deep);
        }
        assert!(gpu.process_channels(&ep, &[0u8; 4], 2, 1, None).is_err());
    }

    #[test]
    fn native_gpu_weak_glow_and_composite_modes() {
        let gpu = GpuProcessor::new();
        let mut ep = EffectParams { source_opacity: 0.0, starglow_opacity: 1.0,
            boost_light: 0.001, input_channel: 4, colors: [[1.0; 3]; 5],
            glow_shape: 4, lengths: [1.0; 8], streak_length: 5.0,
            decay_rate: 0.5, transfer_mode: 4, affect_alpha: true, ..Default::default() };
        let deep = [0u16,0,0,0,32768,32768,32768,32768,0,0,0,0];
        let out = gpu.process_channels(&ep, &deep, 3, 1, None).unwrap();
        assert!(out[1..4].iter().any(|&v| v > 0 && v < 128), "{out:?}");
        assert!(out[0] > 0, "alpha must include visible glow");
        ep.affect_alpha = false;
        let preserved = gpu.process_channels(&ep, &deep, 3, 1, None).unwrap();
        for i in 0..3 {
            assert_eq!(preserved[i*4],deep[i*4]);
            assert_eq!(&preserved[i*4+1..i*4+4],&out[i*4+1..i*4+4]);
        }
        ep.affect_alpha = true;
        assert_eq!(gpu.process_channels(&ep, &deep, 3, 1, None).unwrap(),out);
        let src = [0.25f32,0.125,0.25,0.375, 1.0,1.0,0.75,0.5, 0.0,0.0,0.0,0.0];
        ep.source_opacity = 0.75;
        ep.boost_light = 0.3;
        for mode in [1,2,4,5,6,7,8,9,11,12,14,15,16] {
            ep.transfer_mode = mode;
            let actual = gpu.process_channels(&ep, &src, 3, 1, None).unwrap();
            let expected = crate::glow::process_channels(&ep, &src, 3, 1, None).unwrap();
            let error = actual.iter().zip(&expected).map(|(a,b)| (a-b).abs()).fold(0.0f32, f32::max);
            assert!(error < 2e-6, "mode={mode}, error={error}, actual={actual:?}, expected={expected:?}");
        }
    }
}
