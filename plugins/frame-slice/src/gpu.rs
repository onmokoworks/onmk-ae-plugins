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
    cache_budget: u64,
}

fn buffer_bytes(w:u32,h:u32,frames:u32)->Option<u64> {
    u64::from(w).checked_mul(u64::from(h))?.checked_mul(4)?
        .checked_mul(u64::from(frames)+4)?.checked_add(std::mem::size_of::<GpuParams>() as u64)
}

fn with_unmap<T>(work:impl FnOnce()->T,cleanup:impl FnOnce())->T {
    struct Guard<F:FnOnce()>(Option<F>);
    impl<F:FnOnce()> Drop for Guard<F> {
        fn drop(&mut self) {if let Some(f)=self.0.take(){f();}}
    }
    let _guard=Guard(Some(cleanup));
    work()
}

fn checked_layout(w: usize,h: usize,frames: usize,limits: &Limits) -> Option<(u32,u32,usize)> {
    if w==0 || h==0 || frames==0 { return None; }
    let ww=u32::try_from(w).ok()?;
    let hh=u32::try_from(h).ok()?;
    let n=w.checked_mul(h)?;
    let packed=n.checked_mul(frames)?;
    // The WGSL shader addresses packed pixels with u32 indices.
    u32::try_from(packed).ok()?;
    let bytes=packed.checked_mul(4)? as u64;
    if bytes>limits.max_buffer_size || bytes>u64::from(limits.max_storage_buffer_binding_size)
        || ww.div_ceil(16)>limits.max_compute_workgroups_per_dimension
        || hh.div_ceil(16)>limits.max_compute_workgroups_per_dimension { return None; }
    Some((ww,hh,n))
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
            label: Some("frameslice"),
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
            cache_budget: 512*1024*1024,
        }
    }

    fn create_buffers(&self, w: u32, h: u32, max_frames: u32) -> BufferState {
        let n = u64::from(w) * u64::from(h);
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
    ) -> Option<parking_lot::RwLockUpgradableReadGuard<'_, HashMap<std::thread::ThreadId, BufferState>>>
    {
        let required=buffer_bytes(w,h,num_frames)?;
        if required>self.cache_budget {return None;}
        let lock = self.state.upgradable_read();
        let tid = std::thread::current().id();

        let needs_recreate = lock
            .get(&tid)
            .map(|s| s.width != w || s.height != h || s.max_frames != num_frames)
            .unwrap_or(true);

        if needs_recreate {
            let mut wlock = parking_lot::RwLockUpgradableReadGuard::upgrade(lock);
            // Drop the old resolution before allocating its replacement.
            wlock.remove(&tid);
            let mut retained=wlock.values().try_fold(0u64,|sum,s|
                sum.checked_add(buffer_bytes(s.width,s.height,s.max_frames)?))?;
            while wlock.len()>=8 || retained>self.cache_budget-required {
                let oldest=wlock.iter().min_by_key(|(_,s)|s.last_access.load(std::sync::atomic::Ordering::Relaxed)).map(|(id,_)|*id)?;
                let old=wlock.remove(&oldest)?;
                retained-=buffer_bytes(old.width,old.height,old.max_frames)?;
            }
            wlock.insert(tid, self.create_buffers(w, h, num_frames));
            let lock = parking_lot::RwLockWriteGuard::downgrade_to_upgradable(wlock);
            lock.get(&tid)
                .unwrap()
                .last_access
                .store(ts(), std::sync::atomic::Ordering::Relaxed);
            Some(lock)
        } else {
            lock.get(&tid)
                .unwrap()
                .last_access
                .store(ts(), std::sync::atomic::Ordering::Relaxed);
            Some(lock)
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
    ) -> Option<Vec<u8>> {
        // The CPU contract for fewer than two temporal frames is original-input
        // passthrough, not blending the sole sampled frame. Use that same path.
        if frames.len() < 2 { return None; }
        let (ww, hh, n) = checked_layout(w,h,frames.len(),&self.device.limits())?;
        let row_bytes=n.checked_mul(4)?;
        if original.len()!=row_bytes || map.len()!=n || frames.iter().any(|f|f.len()!=row_bytes) { return None; }
        let num_frames = frames.len() as u32;

        // Reserve host working memory before submitting GPU work or mapping.
        let mut result=crate::byte_plane(row_bytes).ok()?;
        let mut map_f32=Vec::new();
        map_f32.try_reserve_exact(n).ok()?;
        if sp.interpolation == 1 {
            // Select in the CPU reference precision before f32 conversion;
            // otherwise values just below half-frame boundaries can jump ahead.
            if num_frames > (1 << 24) { return None; }
            let last=(num_frames-1) as f64;
            map_f32.extend(map.iter().map(|&v|
                ((v.clamp(0.0,1.0)*last).round() as usize).min(frames.len()-1) as f32));
        } else {
            map_f32.extend(map.iter().map(|&v|v as f32));
        }

        let lock = self.get_state(ww, hh, num_frames)?;
        let state = lock.get(&std::thread::current().id()).unwrap();

        // Upload raw ARGB bytes directly; the shader handles byte order.
        for (fi, frame) in frames.iter().enumerate() {
            let offset = (fi * n * 4) as u64;
            self.queue.write_buffer(&state.frames_buf, offset, frame);
        }

        self.queue.write_buffer(&state.original_buf, 0, original);

        // Nearest mode uploads exact selected indices; linear mode uploads map values.
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
        with_unmap(|| {
        slice.map_async(MapMode::Read, move |v| {
            let _ = tx.send(v);
        });
        self.device.poll(PollType::Wait).ok()?;

        // Bytes are already in ARGB order.
        if let Some(Ok(())) = pollster::block_on(rx.receive()) {
            let data = slice.get_mapped_range();
            if data.len()!=result.len() {return None;}
            result.copy_from_slice(&data);
            drop(data);
            Some(result)
        } else {
            None
        }
        },||state.staging_buf.unmap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn readback_unmaps_after_view_on_return_error_and_panic() {
        use std::cell::RefCell;
        struct View<'a>(&'a RefCell<Vec<&'static str>>);
        impl Drop for View<'_> {fn drop(&mut self){self.0.borrow_mut().push("view");}}
        for mode in 0..3 {
            let order=RefCell::new(Vec::new());
            let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||with_unmap(|| {
                let _view=View(&order);
                match mode {0=>Some(7),1=>None,_=>panic!("injected")}
            },||order.borrow_mut().push("unmap"))));
            assert_eq!(*order.borrow(),["view","unmap"]);
            if mode==2 {assert!(result.is_err());} else {assert_eq!(result.unwrap(),if mode==0 {Some(7)} else {None});}
        }
    }
    #[test]
    fn gpu_cache_bounds_requested_bytes_and_worker_entries() {
        let mut gpu=GpuProcessor::new();
        let bytes=buffer_bytes(17,31,2).unwrap();
        gpu.cache_budget=bytes*2;
        for _ in 0..10 {
            std::thread::scope(|s| {s.spawn(|| {
                let lock=gpu.get_state(17,31,2).unwrap();
                assert!(lock.len()<=2);
                let actual:u64=lock.values().map(|v|v.frames_buf.size()+v.original_buf.size()
                    +v.map_buf.size()+v.output_buf.size()+v.staging_buf.size()+v.params_buf.size()).sum();
                assert!(actual<=gpu.cache_budget);
            }).join().unwrap();});
        }
        assert!(gpu.get_state(170,310,2).is_none());
        {let lock=gpu.get_state(1,1,1).unwrap(); assert!(lock.len()<=2);}
        gpu.cache_budget=bytes*20;
        for _ in 0..12 {
            std::thread::scope(|s| {s.spawn(|| {assert!(gpu.get_state(17,31,2).unwrap().len()<=8);}).join().unwrap();});
        }
    }
    #[test]
    fn gpu_matches_cpu_rounding_before_original_mix() {
        let gpu=GpuProcessor::new();
        let params=SlitScanParams {time_frames:2,frame_step:1,time_direction:1,
            slice_mode:1,gradient_phase:0.0,center:(0.5,0.5),interpolation:2,mix:0.5,invert_map:false};
        let frames=vec![vec![255,0,0,0],vec![255,1,1,1]];
        let expected=crate::slit_scan::render(&params,&frames,1,1,&[0.5],&frames[0]).unwrap();
        let actual=gpu.process(&params,&frames,1,1,&[0.5],&frames[0]).unwrap();
        assert_eq!(actual,expected);
    }

    #[test]
    fn nearest_frame_boundary_preserves_cpu_selection() {
        let gpu=GpuProcessor::new();
        let params=SlitScanParams {time_frames:4,frame_step:1,time_direction:1,
            slice_mode:4,gradient_phase:0.0,center:(0.5,0.5),interpolation:1,mix:1.0,invert_map:false};
        let map:Vec<f64>=(0..3).flat_map(|boundary|[-1e-9,0.0,1e-9].map(|delta|
            (boundary as f64+0.5+delta)/3.0)).collect();
        let frames:Vec<Vec<u8>>=[0u8,70,140,210].into_iter().map(|v|
            (0..map.len()).flat_map(|_|[255,v,v,v]).collect()).collect();
        let expected=crate::slit_scan::render(&params,&frames,map.len(),1,&map,&frames[0]).unwrap();
        let actual=gpu.process(&params,&frames,map.len(),1,&map,&frames[0]).expect("GPU must execute");
        assert_eq!(actual,expected);
    }

    #[test]
    fn insufficient_temporal_frames_preserve_original_like_cpu() {
        let gpu=GpuProcessor::new();
        let original=[127u8,31,63,7];
        for frames in [vec![],vec![vec![255,200,100,50]]] {
            for interpolation in [1,2] {
                for mix in [0.0,0.5,1.0] {
                    let params=SlitScanParams {time_frames:2,frame_step:1,time_direction:1,
                        slice_mode:4,gradient_phase:0.0,center:(0.5,0.5),interpolation,mix,invert_map:false};
                    let expected=crate::slit_scan::render(&params,&frames,1,1,&[0.5],&original).unwrap();
                    assert!(gpu.process(&params,&frames,1,1,&[0.5],&original).is_none());
                    assert_eq!(expected,original);
                }
            }
        }
    }

    #[test]
    fn linear_temporal_boundaries_and_partial_alpha_match_cpu_within_one_code() {
        let gpu=GpuProcessor::new();
        let mut maximum=0u8;
        for count in [2usize,3,4,7] {
            let last=(count-1) as f64;
            let mut map=vec![0.0,1.0];
            for boundary in 0..count-1 {
                for fraction in [0.0,0.5/255.0,0.5,1.0] {
                    for delta in [-1e-9,0.0,1e-9] {
                        map.push(((boundary as f64+fraction+delta)/last).clamp(0.0,1.0));
                    }
                }
            }
            let frames:Vec<Vec<u8>>=(0..count).map(|f|(0..map.len()).flat_map(|pixel| {
                let a=((f*71+pixel*19)%256) as u8;
                [a,a/2,a.saturating_sub(13),a/3]
            }).collect()).collect();
            let original:Vec<u8>=(0..map.len()).flat_map(|pixel| {
                let a=((pixel*31+5)%256) as u8;
                [a,a/3,a/2,a]
            }).collect();
            for mix in [0.0,0.125,0.5,0.875,1.0] {
                let params=SlitScanParams {time_frames:count,frame_step:1,time_direction:1,
                    slice_mode:4,gradient_phase:0.0,center:(0.5,0.5),interpolation:2,mix,invert_map:false};
                let expected=crate::slit_scan::render(&params,&frames,map.len(),1,&map,&original).unwrap();
                let actual=gpu.process(&params,&frames,map.len(),1,&map,&original).expect("GPU must execute");
                let error=actual.iter().zip(&expected).map(|(a,b)|a.abs_diff(*b)).max().unwrap();
                maximum=maximum.max(error);
                assert!(error<=1,"count={count} mix={mix} max_error={error}");
                if mix==0.0 {assert_eq!(actual,original);}
            }
        }
        println!("linear temporal maximum channel error={maximum}/255 across20 GPU renders");
    }
    #[test]
    fn rejects_over_limit_layout_without_allocating() {
        let limits=Limits::default();
        assert!(checked_layout(13,7,3,&limits).is_some());
        assert!(checked_layout(1920,1080,1000,&limits).is_none());
        assert!(checked_layout(usize::MAX,2,2,&limits).is_none());
        assert!(checked_layout(0,1,1,&limits).is_none());
        assert!(checked_layout(1,1,0,&limits).is_none());
    }
    #[test]
    fn actual_gpu_resize_frame_count_and_cpu_parity() {
        let gpu=GpuProcessor::new();
        let mut params=SlitScanParams {time_frames:3,frame_step:1,time_direction:1,
            slice_mode:1,gradient_phase:0.0,center:(0.5,0.5),interpolation:1,mix:1.0,invert_map:false};
        for (w,h,count) in [(13,7,3),(13,7,2),(1,1,3),(17,31,4)] {
            let frames:Vec<Vec<u8>>=(0..count).map(|f|(0..w*h).flat_map(|_|[255,(f*60) as u8,20,40]).collect()).collect();
            let map=crate::slit_scan::generate_gradient_map(1,w,h,0.0,(0.5,0.5),false).unwrap();
            for interp in [1,2] {
                params.interpolation=interp;
                let actual=gpu.process(&params,&frames,w,h,&map,&frames[0]).expect("GPU must execute");
                let expected=crate::slit_scan::render(&params,&frames,w,h,&map,&frames[0]).unwrap();
                let mismatch=actual.iter().zip(&expected).enumerate().find(|(_, (a,b))|a.abs_diff(**b)>1);
                assert!(mismatch.is_none(), "{w}x{h} count={count} interp={interp} mismatch={mismatch:?}");
            }
            let lock=gpu.state.read();
            assert_eq!(lock.get(&std::thread::current().id()).unwrap().max_frames,count as u32);
        }
        assert!(gpu.process(&params,&[vec![0;4]],2,1,&[0.0;2],&[0;8]).is_none());
    }

    #[test]
    fn shared_processor_concurrent_workers_keep_frame_data_separate() {
        let gpu=GpuProcessor::new();
        let start=std::sync::Barrier::new(2);
        std::thread::scope(|scope| {
            let workers:Vec<_>=(0..2).map(|worker| {
                let gpu=&gpu; let start=&start;
                scope.spawn(move || {
                    let params=SlitScanParams {time_frames:3,frame_step:1,time_direction:1,
                        slice_mode:1,gradient_phase:0.0,center:(0.5,0.5),interpolation:2,mix:0.75,invert_map:false};
                    start.wait();
                    for (w,h) in [(7+worker*2,13),(1,7+worker*2),(7+worker*2,13)] {
                        let frames:Vec<Vec<u8>>=(0..3).map(|f|(0..w*h).flat_map(|i|
                            [255,((worker*91+f*47+i*3)%256) as u8,((i*7+f*19)%256) as u8,40]).collect()).collect();
                        let map=crate::slit_scan::generate_gradient_map(1,w,h,0.0,(0.5,0.5),false).unwrap();
                        let expected=crate::slit_scan::render(&params,&frames,w,h,&map,&frames[0]).unwrap();
                        let actual=gpu.process(&params,&frames,w,h,&map,&frames[0]).expect("shared GPU render failed");
                        assert_eq!(actual.len(),expected.len());
                        assert!(actual.iter().zip(expected).all(|(a,b)|a.abs_diff(b)<=1),
                            "worker{worker} {w}x{h} received incorrect frame data");
                    }
                })
            }).collect();
            for worker in workers {worker.join().unwrap();}
        });
        assert_eq!(gpu.state.read().len(),2);
    }
}
