use crate::core::EdgeParams;
use bytemuck::{Pod, Zeroable};
use std::sync::{mpsc, OnceLock};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    size_step: [u32; 4],
    edge0: [f32; 4],
    edge1: [f32; 4],
    motion: [f32; 4],
    seed_detail: [u32; 4],
}

struct Processor {
    device: wgpu::Device,
    queue: wgpu::Queue,
    shader: wgpu::ShaderModule,
    layout: wgpu::PipelineLayout,
    bind_layout: wgpu::BindGroupLayout,
}
static GPU: OnceLock<Option<Processor>> = OnceLock::new();

fn checked_frame(w:usize,h:usize,len:usize,limits:&wgpu::Limits)->Result<u64,String> {
    let invalid=||"invalid or unsupported GPU frame size".to_string();
    let ww=u32::try_from(w).map_err(|_|invalid())?;
    let hh=u32::try_from(h).map_err(|_|invalid())?;
    // Jump flood uses signed neighbor coordinates and a power-of-two step.
    if ww==0 || hh==0 || ww>i32::MAX as u32/2 || hh>i32::MAX as u32/2 {return Err(invalid());}
    let count=w.checked_mul(h).and_then(|v|v.checked_mul(4)).ok_or_else(invalid)?;
    if count!=len || count>u32::MAX as usize {return Err(invalid());}
    let bytes=count.checked_mul(4).filter(|&v|v<=isize::MAX as usize).ok_or_else(invalid)? as u64;
    if bytes>limits.max_buffer_size || bytes>u64::from(limits.max_storage_buffer_binding_size)
        || ww.div_ceil(8)>limits.max_compute_workgroups_per_dimension
        || hh.div_ceil(8)>limits.max_compute_workgroups_per_dimension {return Err(invalid());}
    Ok(bytes)
}

fn processor() -> Option<&'static Processor> {
    GPU.get_or_init(|| Processor::new().ok()).as_ref()
}

fn with_unmap<T>(work:impl FnOnce()->T,cleanup:impl FnOnce())->T {
    struct Guard<F:FnOnce()>(Option<F>);
    impl<F:FnOnce()> Drop for Guard<F> {fn drop(&mut self){if let Some(f)=self.0.take(){f();}}}
    let _guard=Guard(Some(cleanup));
    work()
}

fn decode_readback(bytes:&[u8],channels:usize,out:&mut Vec<f32>)->Result<(),String> {
    if channels.checked_mul(4)!=Some(bytes.len()) || out.capacity()<channels {
        return Err("invalid GPU readback length or capacity".into());
    }
    out.clear();
    out.extend(bytes.chunks_exact(4).map(|b|f32::from_ne_bytes([b[0],b[1],b[2],b[3]])));
    Ok(())
}

impl Processor {
    fn new() -> Result<Self, String> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
        }))
        .map_err(|e| format!("no GPU adapter: {e}"))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("EdgeSmith GPU"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            memory_hints: wgpu::MemoryHints::Performance,
            trace: wgpu::Trace::Off,
        }))
        .map_err(|e| format!("GPU device failed: {e}"))?;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("EdgeSmith shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shader.wgsl").into()),
        });
        let storage = |binding, read_only| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("EdgeSmith bind layout"),
            entries: &[
                storage(0, true),
                storage(1, true),
                storage(2, false),
                storage(3, false),
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("EdgeSmith pipeline layout"),
            bind_group_layouts: &[&bind_layout],
            push_constant_ranges: &[],
        });
        Ok(Self {
            device,
            queue,
            shader,
            layout,
            bind_layout,
        })
    }

    fn render(
        &self,
        input: &[f32],
        w: usize,
        h: usize,
        p: &EdgeParams,
    ) -> Result<Vec<f32>, String> {
        let bytes = checked_frame(w,h,input.len(),&self.device.limits())?;
        let mut result=Vec::new();
        result.try_reserve_exact(input.len()).map_err(|_|"GPU readback allocation failed".to_string())?;
        let source = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("source"),
                contents: bytemuck::cast_slice(input),
                usage: wgpu::BufferUsages::STORAGE,
            });
        let seed_usage = wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_SRC
            | wgpu::BufferUsages::COPY_DST;
        let seeds_a = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("seeds a"),
            size: bytes,
            usage: seed_usage,
            mapped_at_creation: false,
        });
        let seeds_b = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("seeds b"),
            size: bytes,
            usage: seed_usage,
            mapped_at_creation: false,
        });
        let output = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("output"),
            size: bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: bytes,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut u = Uniforms {
            size_step: [w as u32, h as u32, 0, 0],
            edge0: [p.border, p.roughness, p.scale.max(1.0), p.balance / 100.0],
            edge1: [
                p.sharpness / 100.0,
                p.stretch / 100.0,
                p.preserve / 100.0,
                p.mix,
            ],
            motion: [p.angle.to_radians(), p.evolution, 0.0, 0.0],
            seed_detail: [p.seed, p.detail.clamp(1, 8), 0, 0],
        };
        let uniform = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("uniform"),
                contents: bytemuck::bytes_of(&u),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
        let init = self.pipeline("init_seeds");
        let jump = self.pipeline("jump");
        let finish = self.pipeline("finish");
        let group = |read: &wgpu::Buffer, write: &wgpu::Buffer| {
            self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.bind_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: source.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: read.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: write.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: output.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: uniform.as_entire_binding(),
                    },
                ],
            })
        };
        self.dispatch(&init, &group(&seeds_b, &seeds_a), w, h);
        let mut step = 1u32;
        while step < (w.max(h) as u32) {
            step <<= 1;
        }
        step >>= 1;
        let mut a_is_read = true;
        while step > 0 {
            u.size_step[2] = step;
            self.queue.write_buffer(&uniform, 0, bytemuck::bytes_of(&u));
            let (read, write) = if a_is_read {
                (&seeds_a, &seeds_b)
            } else {
                (&seeds_b, &seeds_a)
            };
            self.dispatch(&jump, &group(read, write), w, h);
            a_is_read = !a_is_read;
            step >>= 1;
        }
        let final_seeds = if a_is_read { &seeds_a } else { &seeds_b };
        let spare = if a_is_read { &seeds_b } else { &seeds_a };
        self.dispatch(&finish, &group(final_seeds, spare), w, h);
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("EdgeSmith readback"),
            });
        enc.copy_buffer_to_buffer(&output, 0, &readback, 0, bytes);
        self.queue.submit(Some(enc.finish()));
        let (tx, rx) = mpsc::channel();
        let mut result=with_unmap(|| {
        readback.map_async(wgpu::MapMode::Read, .., move |r| {
            let _ = tx.send(r);
        });
        self.device
            .poll(wgpu::PollType::wait())
            .map_err(|e| format!("GPU wait failed: {e}"))?;
        rx.recv()
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        let mapped = readback.get_mapped_range(..);
        decode_readback(&mapped,input.len(),&mut result)?;
        drop(mapped);
        Ok::<_,String>(result)
        },||readback.unmap())?;
        crate::core::restore_propagated_rgb(input,w,h,&mut result)?;
        Ok(result)
    }
    fn pipeline(&self, entry: &str) -> wgpu::ComputePipeline {
        self.device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&self.layout),
                module: &self.shader,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
    }
    fn dispatch(
        &self,
        pipeline: &wgpu::ComputePipeline,
        group: &wgpu::BindGroup,
        w: usize,
        h: usize,
    ) {
        let mut e = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = e.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, group, &[]);
            pass.dispatch_workgroups((w as u32).div_ceil(8), (h as u32).div_ceil(8), 1);
        }
        self.queue.submit(Some(e.finish()));
    }
}

pub fn render(input: &[f32], w: usize, h: usize, p: &EdgeParams) -> Result<Vec<f32>, String> {
    processor()
        .ok_or_else(|| "GPU unavailable".to_string())?
        .render(input, w, h, p)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn readback_bytes_preserve_bits_and_reject_invalid_lengths() {
        let bits=[0x80000000u32,0x41000000,0xc0000000,0x7fc00123];
        let mut bytes=vec![0u8];
        for value in bits {bytes.extend_from_slice(&value.to_ne_bytes());}
        let mut out=Vec::with_capacity(4);
        decode_readback(&bytes[1..],4,&mut out).unwrap();
        assert_eq!(out.iter().map(|v|v.to_bits()).collect::<Vec<_>>(),bits);
        for (data,count) in [(&bytes[1..16],4),(&bytes[1..],usize::MAX)] {
            assert!(decode_readback(data,count,&mut out).is_err());
            assert_eq!(out.iter().map(|v|v.to_bits()).collect::<Vec<_>>(),bits);
        }
        assert!(decode_readback(&bytes[1..],4,&mut Vec::new()).is_err());
    }

    #[test]
    fn readback_cleanup_follows_view_drop_on_all_exits() {
        use std::cell::RefCell;
        struct View<'a>(&'a RefCell<Vec<&'static str>>);
        impl Drop for View<'_> {fn drop(&mut self){self.0.borrow_mut().push("view");}}
        for mode in 0..3 {
            let order=RefCell::new(Vec::new());
            let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||with_unmap(|| {
                let _view=View(&order);
                match mode {0=>Ok(()),1=>Err("map"),_=>panic!("injected")}
            },||order.borrow_mut().push("unmap"))));
            assert_eq!(*order.borrow(),["view","unmap"]);
            if mode==2 {assert!(result.is_err());} else {assert_eq!(result.unwrap(),if mode==0 {Ok(())} else {Err("map")});}
        }
    }
    #[test]
    fn gpu_frame_limits_checked_before_buffer_creation() {
        let mut limits=wgpu::Limits::downlevel_defaults();
        for (w,h,len) in [(0,1,0),(1,1,3),(1,1,5),(usize::MAX,2,0),
            (i32::MAX as usize,1,0)] {assert!(checked_frame(w,h,len,&limits).is_err());}
        assert_eq!(checked_frame(17,31,17*31*4,&limits).unwrap(),17*31*16);
        assert!(checked_frame(50000,1,50000*4,&limits).is_ok());
        limits.max_buffer_size=16;
        assert!(checked_frame(2,1,8,&limits).is_err());
        assert_eq!(checked_frame(1,1,4,&limits).unwrap(),16);
        limits.max_buffer_size=1024;
        limits.max_storage_buffer_binding_size=16;
        assert!(checked_frame(2,1,8,&limits).is_err());
        limits.max_storage_buffer_binding_size=1024;
        limits.max_compute_workgroups_per_dimension=1;
        assert!(checked_frame(9,1,36,&limits).is_err());
        assert!(checked_frame(8,1,32,&limits).is_ok());
    }
    #[test]
    fn single_seed_matches_cpu_chamfer_metric() {
        let gpu=processor().expect("actual GPU required for distance characterization");
        let (w,h)=(7,5);
        let mut input=vec![0.0;w*h*4];
        input[..4].copy_from_slice(&[0.2,0.4,0.8,1.0]);
        let p=EdgeParams {roughness:0.0,border:2.3,sharpness:100.0,..Default::default()};
        let cpu=crate::core::render_rgba(&input,w,h,&p);
        let actual=gpu.render(&input,w,h,&p).unwrap();
        let alpha=|distance:f32| {
            let t=((p.border-distance+0.35)/0.7).clamp(0.0,1.0);
            t*t*(3.0-2.0*t)
        };
        for y in 0..h {for x in 0..w {
            if x==0 && y==0 {continue;}
            let chamfer=(x.max(y)-x.min(y)) as f32+x.min(y) as f32*std::f32::consts::SQRT_2;
            let i=(y*w+x)*4+3;
            assert!((actual[i]-alpha(chamfer)).abs()<2e-6,"GPU ({x},{y})");
            assert!((cpu[i]-alpha(chamfer)).abs()<2e-6,"CPU ({x},{y})");
        }}
        let i=(w+2)*4+3;
        assert!((cpu[i]-actual[i]).abs()<2e-6);
    }
    #[test]
    fn multiple_seeds_match_cpu_alpha_without_noise() {
        let gpu=processor().expect("actual GPU required");
        for (w,h) in [(1,9),(9,1),(7,5),(17,13)] {
            for seed in 1..=4u32 {
                let mut rng=seed;
                let input:Vec<f32>=(0..w*h).flat_map(|_| {
                    rng=rng.wrapping_mul(1664525).wrapping_add(1013904223);
                    [0.2,0.4,0.8,if rng%5==0 {1.0} else {0.0}]
                }).collect();
                for border in [-1.3,0.0,2.3] {
                    let p=EdgeParams {roughness:0.0,border,sharpness:70.0,..Default::default()};
                    let cpu=crate::core::render_rgba(&input,w,h,&p);
                    let actual=gpu.render(&input,w,h,&p).unwrap();
                    for i in 0..w*h {assert!((cpu[i*4+3]-actual[i*4+3]).abs()<3e-6,
                        "{w}x{h} seed={seed} border={border} pixel={i} cpu={} gpu={}",cpu[i*4+3],actual[i*4+3]);}
                }
            }
        }
    }
    #[test]
    fn symmetric_seed_color_selection_matches_cpu() {
        let gpu=processor().expect("actual GPU required for color selection");
        let (w,h)=(5,5);
        let mut input=vec![0.0;w*h*4];
        for (x,y,color) in [(0,0,[1.,0.,0.,1.]),(4,0,[0.,1.,0.,1.]),
            (0,4,[0.,0.,1.,1.]),(4,4,[1.,1.,0.,1.])] {
            let i=(y*w+x)*4; input[i..i+4].copy_from_slice(&color);
        }
        let p=EdgeParams {roughness:0.0,border:4.0,..Default::default()};
        let cpu=crate::core::render_rgba(&input,w,h,&p);
        let actual=gpu.render(&input,w,h,&p).unwrap();
        let mut differences=0;
        for i in 0..w*h {
            assert!((cpu[i*4+3]-actual[i*4+3]).abs()<3e-6);
            if cpu[i*4..i*4+3]!=actual[i*4..i*4+3] {
                let x=(i%w) as i32; let y=(i/w) as i32;
                let distance=|sx:i32,sy:i32| {
                    let dx=(x-sx).abs(); let dy=(y-sy).abs();
                    (dx.max(dy)-dx.min(dy)) as f32+dx.min(dy) as f32*std::f32::consts::SQRT_2
                };
                let candidates=[(0,0,[1.,0.,0.]),(4,0,[0.,1.,0.]),(0,4,[0.,0.,1.]),(4,4,[1.,1.,0.])];
                let best=candidates.iter().map(|(sx,sy,_)|distance(*sx,*sy)).fold(f32::INFINITY,f32::min);
                for selected in [&cpu[i*4..i*4+3],&actual[i*4..i*4+3]] {
                    assert!(candidates.iter().any(|(sx,sy,color)|selected==color && (distance(*sx,*sy)-best).abs()<1e-6));
                }
                differences+=1;
                eprintln!("Color selection difference({},{}) CPU={:?} GPU={:?}",i%w,i/w,&cpu[i*4..i*4+3],&actual[i*4..i*4+3]);
            }
        }
        eprintln!("Symmetric color selection differences={differences}");
        assert_eq!(differences,0);
    }
    #[test]
    fn noisy_alpha_matches_cpu_parameter_samples() {
        let gpu=processor().expect("actual GPU required for noise parity");
        let (w,h)=(17,13);
        let input:Vec<f32>=(0..w*h).flat_map(|i| {
            let (x,y)=(i%w,i/w);
            [0.2,0.4,0.8,if (3..14).contains(&x)&&(2..11).contains(&y) {1.0} else {0.0}]
        }).collect();
        let mut max_error=0.0f32;
        for seed in [0,17] {for evolution in [0.0,123.5] {for angle in [0.0,37.0] {for stretch in [-99.0,99.0] {
            let p=EdgeParams {roughness:4.0,scale:3.5,detail:5,balance:25.0,
                border:0.3,seed,evolution,angle,stretch,..Default::default()};
            let cpu=crate::core::render_rgba(&input,w,h,&p);
            let actual=gpu.render(&input,w,h,&p).unwrap();
            for i in 0..w*h {
                let error=(cpu[i*4+3]-actual[i*4+3]).abs();
                max_error=max_error.max(error);
                assert!(error<2e-4,"seed={seed} evolution={evolution} angle={angle} stretch={stretch} pixel={i} error={error}");
            }
        }}}}
        eprintln!("Noisy alpha16 cases max error={max_error}");
    }
    #[test]
    fn gpu_preserves_partial_alpha_pixel_color() {
        let gpu=processor().expect("actual GPU required for color regression");
        let input=[1.0,0.0,0.0,1.0, 0.0,0.0,2.0,0.25];
        for mix in [0.0,0.5,1.0] {
            let p=EdgeParams {roughness:0.0,mix,..Default::default()};
            let cpu=crate::core::render_rgba(&input,2,1,&p);
            let actual=gpu.render(&input,2,1,&p).unwrap();
            for (i,(a,b)) in actual.iter().zip(&cpu).enumerate() {
                assert!((a-b).abs()<1e-6,"mix={mix} channel={i}: GPU={a} CPU={b}");
            }
        }
    }
    #[test]
    fn gpu_shader_renders_when_adapter_is_available() {
        let mut src = vec![0.0f32; 8 * 8 * 4];
        for y in 2..6 {
            for x in 2..6 {
                src[(y * 8 + x) * 4 + 3] = 1.0;
            }
        }
        let Some(gpu) = processor() else {
            eprintln!("GPU test not exercised: adapter/device unavailable");
            return;
        };
        assert!(gpu.render(&[],usize::MAX,2,&EdgeParams::default()).is_err());
        assert!(gpu.render(&[0.;5],1,1,&EdgeParams::default()).is_err());
        // Once a device exists, a shader/readback failure is a failed test,
        // not an implicit skip.
        let out = gpu.render(&src, 8, 8, &EdgeParams::default()).expect("GPU render failed");
        assert_eq!(out.len(), src.len());
        assert!(out.iter().all(|v| v.is_finite()));
        // RGB should survive the shader unchanged when Mix is zero, including
        // negative/HDR channels and partially transparent pixels.
        let hdr = [8.0, -2.0, 0.25, 0.5].repeat(8 * 8);
        let params = EdgeParams { mix: 0.0, ..Default::default() };
        let hdr_out = gpu.render(&hdr, 8, 8, &params).expect("GPU HDR render failed");
        assert_eq!(hdr_out, crate::core::render_rgba(&hdr, 8, 8, &params));
        assert_eq!(hdr_out, hdr);
        eprintln!("GPU shader and readback exercised: 8x8");
    }
}
