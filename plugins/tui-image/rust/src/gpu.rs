use crate::TuiParams;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use wgpu::*;

// Requested buffer bytes, not driver allocation overhead or total VRAM.
const CACHE_BYTE_BUDGET:u64=256*1024*1024;
fn state_bytes(w:u32,h:u32)->u64 {
    u64::from(w)*u64::from(h)*12 + std::mem::size_of::<GpuParams>() as u64
}
fn cache_has_room(bytes:u64,count:usize,incoming:u64,max_count:usize,budget:u64)->bool {
    count<max_count && incoming<=budget && bytes<=budget-incoming
}

#[repr(C, align(16))]
#[derive(Clone, Copy)]
struct GpuParams {
    width: u32,
    height: u32,
    cell_w: u32,
    cell_h: u32,
    preset: u32,
    color_mode: u32,
    contrast: f32,
    gamma: f32,
    edge_boost: f32,
    invert: u32,
    source_mix: f32,
    source_flags: u32,
    foreground: [f32; 4],
    background: [f32; 4],
    glyph_layout: [f32; 4],
}

struct BufferState {
    width: u32,
    height: u32,
    source_buf: Buffer,
    output_buf: Buffer,
    staging_buf: Buffer,
    params_buf: Buffer,
    bind_group: BindGroup,
    last_access: AtomicUsize,
}

pub(crate) struct GpuProcessor {
    device: Device,
    queue: Queue,
    pipeline: ComputePipeline,
    bind_group_layout: BindGroupLayout,
    state: RwLock<HashMap<std::thread::ThreadId, BufferState>>,
    cache_byte_budget:u64,
}

impl GpuProcessor {
    #[cfg(test)]
    pub(crate) fn limit_cache_to_two_small_frames(&mut self,w:u32,h:u32) {
        self.state.write().clear();
        self.cache_byte_budget=state_bytes(w,h)*2;
    }
    #[cfg(test)]
    pub(crate) fn cached_layouts(&self)->Vec<(u32,u32)> {
        self.state.read().values().map(|s|(s.width,s.height)).collect()
    }
    pub(crate) fn new() -> Result<Self, String> {
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
            required_features: adapter.features(),
            required_limits: adapter.limits(),
            memory_hints: MemoryHints::Performance,
            trace: Trace::Off,
        }))
        .map_err(|e| format!("Failed to create GPU device: {e}"))?;

        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("tui_image_renderer"),
            source: ShaderSource::Wgsl(std::borrow::Cow::Borrowed(include_str!("../shader.wgsl"))),
        });

        let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: None,
            entries: &[bgl_uniform(0), bgl_storage_ro(1), bgl_storage_rw(2)],
        });

        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            module: &shader,
            entry_point: Some("tui_pass"),
            label: None,
            layout: Some(&pipeline_layout),
            compilation_options: Default::default(),
            cache: Default::default(),
        });

        Ok(Self {
            device,
            queue,
            pipeline,
            bind_group_layout,
            state: RwLock::new(HashMap::new()),
            cache_byte_budget:CACHE_BYTE_BUDGET,
        })
    }

    pub(crate) fn process(
        &self,
        src: &[u8],
        w: usize,
        h: usize,
        ep: &TuiParams,
    ) -> Option<Vec<u8>> {
        let (ww,hh,n,bytes)=checked_layout(w,h,src.len(),&self.device.limits())?;
        if state_bytes(ww,hh)>self.cache_byte_budget {return None;}
        let lock = self.get_state(ww, hh);
        let state = lock.get(&std::thread::current().id())?;

        let mut result=Vec::new();
        result.try_reserve_exact(bytes).ok()?;
        result.resize(bytes,0);
        let mut src_packed=Vec::new();
        src_packed.try_reserve_exact(n).ok()?;
        src_packed.extend((0..n)
            .map(|i| {
                let o = i * 4;
                let a = src[o] as u32;
                let r = src[o + 1] as u32;
                let g = src[o + 2] as u32;
                let b = src[o + 3] as u32;
                (r << 24) | (g << 16) | (b << 8) | a
            }));
        self.queue
            .write_buffer(&state.source_buf, 0, as_bytes_slice(&src_packed));

        let params = GpuParams {
            width: ww,
            height: hh,
            cell_w: ep.cell_w as u32,
            cell_h: ep.cell_h as u32,
            preset: ep.preset as u32,
            color_mode: ep.color_mode as u32,
            contrast: ep.contrast,
            gamma: ep.gamma,
            edge_boost: ep.edge_boost,
            invert: if ep.invert { 1 } else { 0 },
            source_mix: ep.source_mix,
            source_flags: u32::from(ep.use_source_luma) | (u32::from(ep.preserve_source_color) << 1),
            foreground: [ep.foreground[0], ep.foreground[1], ep.foreground[2], 1.0],
            background: [ep.background[0], ep.background[1], ep.background[2], 1.0],
            glyph_layout: [
                (ep.glyph_scale_x.clamp(0.1,1.0)*(1.0-ep.column_gap.clamp(0.0,0.8))).clamp(0.05,1.0),
                ((if ep.uniform_scale {ep.glyph_scale_x} else {ep.glyph_scale_y}).clamp(0.1,1.0)*(1.0-ep.row_gap.clamp(0.0,0.8))).clamp(0.05,1.0),
                0.0,0.0,
            ],
        };
        self.queue
            .write_buffer(&state.params_buf, 0, as_bytes(&params));

        let mut enc = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor { label: None });
        {
            let mut cp = enc.begin_compute_pass(&ComputePassDescriptor::default());
            cp.set_pipeline(&self.pipeline);
            cp.set_bind_group(0, &state.bind_group, &[]);
            cp.dispatch_workgroups(ww.div_ceil(16), hh.div_ceil(16), 1);
        }
        enc.copy_buffer_to_buffer(&state.output_buf, 0, &state.staging_buf, 0, bytes as u64);
        self.queue.submit(Some(enc.finish()));

        let slice = state.staging_buf.slice(..);
        let (tx, rx) = futures_intrusive::channel::shared::oneshot_channel();
        let _unmap=ScopeCleanup(Some(||state.staging_buf.unmap()));
        slice.map_async(MapMode::Read, move |v| {let _=tx.send(v);});
        self.device.poll(PollType::Wait).ok()?;

        if let Some(Ok(())) = pollster::block_on(rx.receive()) {
            let data = slice.get_mapped_range();
            let packed: &[u32] =
                unsafe { std::slice::from_raw_parts(data.as_ptr() as *const u32, n) };
            for i in 0..n {
                let p = packed[i];
                let r = ((p >> 24) & 0xFF) as u8;
                let g = ((p >> 16) & 0xFF) as u8;
                let b = ((p >> 8) & 0xFF) as u8;
                let a = (p & 0xFF) as u8;
                result[i * 4] = a;
                result[i * 4 + 1] = r;
                result[i * 4 + 2] = g;
                result[i * 4 + 3] = b;
            }
            drop(data);
            Some(result)
        } else {
            None
        }
    }

    fn create_buffers(&self, w: u32, h: u32) -> BufferState {
        let n = u64::from(w) * u64::from(h);
        let mk = |size: u64, usage: BufferUsages| {
            self.device.create_buffer(&BufferDescriptor {
                size,
                usage,
                label: None,
                mapped_at_creation: false,
            })
        };

        let source_buf = mk(n * 4, BufferUsages::STORAGE | BufferUsages::COPY_DST);
        let output_buf = mk(n * 4, BufferUsages::STORAGE | BufferUsages::COPY_SRC);
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
                    resource: source_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: output_buf.as_entire_binding(),
                },
            ],
        });

        BufferState {
            width: w,
            height: h,
            source_buf,
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
                // Drop the superseded size before allocating its replacement.
                map.remove(&tid);
                let max = num_cpus::get().max(2);
                let incoming=state_bytes(w,h);
                let mut retained:u64=map.values().map(|s|state_bytes(s.width,s.height)).sum();
                while !cache_has_room(retained,map.len(),incoming,max,self.cache_byte_budget) {
                    let oldest=map.iter().min_by_key(|(_,s)|s.last_access.load(Ordering::Relaxed)).map(|(id,_)|*id);
                    let Some(id)=oldest else {break;};
                    if let Some(old)=map.remove(&id) {retained-=state_bytes(old.width,old.height);}
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
}

struct ScopeCleanup<F:FnOnce()>(Option<F>);
impl<F:FnOnce()> Drop for ScopeCleanup<F> {
    fn drop(&mut self) {if let Some(cleanup)=self.0.take() {cleanup();}}
}

fn checked_layout(w:usize,h:usize,source_len:usize,limits:&Limits)->Option<(u32,u32,usize,usize)> {
    if w==0 || h==0 {return None;}
    let ww=u32::try_from(w).ok()?;
    let hh=u32::try_from(h).ok()?;
    let n=w.checked_mul(h)?;
    let bytes=n.checked_mul(4)?;
    // Shader pixel indexing is u32; every buffer binding must fit the device.
    if n>u32::MAX as usize || bytes>isize::MAX as usize || source_len!=bytes
        || bytes as u64>limits.max_buffer_size || bytes as u64>u64::from(limits.max_storage_buffer_binding_size)
        || ww.div_ceil(16)>limits.max_compute_workgroups_per_dimension
        || hh.div_ceil(16)>limits.max_compute_workgroups_per_dimension {return None;}
    Some((ww,hh,n,bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cache_admission_bounds_total_bytes_and_entries() {
        let frame=state_bytes(1920,1080);
        assert_eq!(frame,24_883_200+std::mem::size_of::<GpuParams>() as u64);
        assert!(cache_has_room(0,0,frame,1,CACHE_BYTE_BUDGET));
        assert!(!cache_has_room(frame,1,frame,1,CACHE_BYTE_BUDGET));
        assert!(cache_has_room(CACHE_BYTE_BUDGET-frame,0,frame,2,CACHE_BYTE_BUDGET));
        assert!(!cache_has_room(CACHE_BYTE_BUDGET-frame+1,0,frame,2,CACHE_BYTE_BUDGET));
        assert!(!cache_has_room(0,0,CACHE_BYTE_BUDGET+1,2,CACHE_BYTE_BUDGET));
        // Model LRU removals until an incoming frame fits.
        let mut retained=frame*10;
        let mut count=10;
        while !cache_has_room(retained,count,frame*2,32,CACHE_BYTE_BUDGET) {retained-=frame;count-=1;}
        assert!(retained+frame*2<=CACHE_BYTE_BUDGET);
        assert_eq!(count,8);
    }
    #[test]
    fn cleanup_runs_once_on_success_early_return_and_panic() {
        for stage in 0..3 {
            let calls=std::cell::Cell::new(0);
            let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _guard=ScopeCleanup(Some(||calls.set(calls.get()+1)));
                if stage==1 {return None;}
                if stage==2 {panic!("injected readback panic");}
                Some(7)
            }));
            assert_eq!(calls.get(),1);
            match stage {0=>assert_eq!(result.unwrap(),Some(7)),1=>assert_eq!(result.unwrap(),None),_=>assert!(result.is_err())}
        }
    }
    #[test]
    fn layout_rejects_overflow_and_device_limits_before_allocation() {
        let mut limits=Limits::default();
        assert_eq!(checked_layout(17,31,17*31*4,&limits),Some((17,31,527,2108)));
        assert!(checked_layout(0,1,0,&limits).is_none());
        assert!(checked_layout(usize::MAX,2,0,&limits).is_none());
        assert!(checked_layout(17,31,100,&limits).is_none());
        limits.max_buffer_size=2107;
        assert!(checked_layout(17,31,2108,&limits).is_none());
        limits.max_buffer_size=4096;
        limits.max_storage_buffer_binding_size=2107;
        assert!(checked_layout(17,31,2108,&limits).is_none());
        limits.max_storage_buffer_binding_size=4096;
        limits.max_compute_workgroups_per_dimension=1;
        assert!(checked_layout(17,31,2108,&limits).is_none());
        assert_eq!(checked_layout(16,16,1024,&limits),Some((16,16,256,1024)));
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
        .unwrap()
        .as_millis() as usize
}
