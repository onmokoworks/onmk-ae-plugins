//! wgpu compute backend used *inside ordinary SmartRender / Render*.
//! This is not AE-native GPU rendering (no SmartRenderGPU / GPU suite).
//!
//! Device, queue, pipelines and layout are created once and shared
//! (`Mutex<Slot>` + `Arc<Ctx>`); buffers are per frame. GPU frames are
//! serialised per device so error scopes and polling are unambiguous.

use crate::engine::{GpuRun, Prepared};
use crate::spec::{Blend, Dilation, Plan};
use bytemuck::{Pod, Zeroable};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};
use wgpu::util::DeviceExt;

const WG: u32 = 16;
const RETRY_AFTER: Duration = Duration::from_secs(10);
const MAP_TIMEOUT: Duration = Duration::from_secs(30);

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default)]
struct Uniform {
    w: u32,
    h: u32,
    out_w: u32,
    out_h: u32,
    i: [i32; 4],
    f: [f32; 8],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdapterIdentity {
    pub name: String,
    pub backend: String,
    pub device_type: String,
    pub driver: String,
    pub driver_info: String,
    pub vendor: u32,
    pub device: u32,
}

impl std::fmt::Display for AdapterIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} [{} {}] vendor=0x{:04x} device=0x{:04x} driver={} {}",
            self.name,
            self.backend,
            self.device_type,
            self.vendor,
            self.device,
            self.driver,
            self.driver_info
        )
    }
}

struct GpuError {
    message: String,
    /// Device/driver failure: drop the cached context.
    fatal: bool,
}

fn soft(message: impl Into<String>) -> GpuError {
    GpuError {
        message: message.into(),
        fatal: false,
    }
}

fn fatal(message: impl Into<String>) -> GpuError {
    GpuError {
        message: message.into(),
        fatal: true,
    }
}

pub struct Ctx {
    device: wgpu::Device,
    queue: wgpu::Queue,
    bind_layout: wgpu::BindGroupLayout,
    level: wgpu::ComputePipeline,
    accum: wgpu::ComputePipeline,
    blur_h: wgpu::ComputePipeline,
    blur_v: wgpu::ComputePipeline,
    composite: wgpu::ComputePipeline,
    dummy: wgpu::Buffer,
    pub identity: AdapterIdentity,
    exec: Mutex<()>,
}

enum Slot {
    Empty,
    Ready(Arc<Ctx>),
    Failed(Instant, String),
}

static SLOT: Mutex<Slot> = Mutex::new(Slot::Empty);

/// Shared hardware GPU context, created lazily on first GPU use only.
pub fn context() -> Result<Arc<Ctx>, String> {
    let mut slot = SLOT.lock().unwrap_or_else(|p| p.into_inner());
    match &*slot {
        Slot::Ready(ctx) => return Ok(ctx.clone()),
        Slot::Failed(at, reason) if at.elapsed() < RETRY_AFTER => return Err(reason.clone()),
        _ => {}
    }
    let created = std::panic::catch_unwind(Ctx::new)
        .unwrap_or_else(|_| Err("GPU initialisation panicked".to_string()));
    match created {
        Ok(ctx) => {
            let ctx = Arc::new(ctx);
            *slot = Slot::Ready(ctx.clone());
            Ok(ctx)
        }
        Err(reason) => {
            *slot = Slot::Failed(Instant::now(), reason.clone());
            Err(reason)
        }
    }
}

/// Identity of the cached hardware adapter (initialises the GPU).
pub fn adapter_identity() -> Result<AdapterIdentity, String> {
    context().map(|c| c.identity.clone())
}

fn invalidate(ctx: &Arc<Ctx>, reason: &str) {
    let mut slot = SLOT.lock().unwrap_or_else(|p| p.into_inner());
    let same = matches!(&*slot, Slot::Ready(cur) if Arc::ptr_eq(cur, ctx));
    if same {
        *slot = Slot::Failed(Instant::now(), reason.to_string());
    }
}

/// GPU backend entry used by `engine::render`.
pub fn shade(prep: &Prepared, plan: &Plan) -> Result<(Vec<[f32; 4]>, GpuRun), String> {
    let ctx = context()?;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ctx.shade(prep, plan)));
    match result {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => {
            if e.fatal {
                invalidate(&ctx, &e.message);
            }
            Err(e.message)
        }
        Err(_) => {
            invalidate(&ctx, "GPU backend panicked");
            Err("GPU backend panicked".to_string())
        }
    }
}

fn to_u32(v: usize) -> Result<u32, GpuError> {
    u32::try_from(v).map_err(|_| soft("frame dimension exceeds u32"))
}

fn to_i32(v: i64) -> Result<i32, GpuError> {
    i32::try_from(v).map_err(|_| soft("frame offset exceeds i32"))
}

fn buffer_bytes(count: usize, elem: u64, limits: &wgpu::Limits) -> Result<u64, GpuError> {
    let bytes = (count as u64)
        .checked_mul(elem)
        .ok_or_else(|| soft("GPU buffer size overflow"))?;
    if bytes == 0
        || bytes > limits.max_buffer_size
        || bytes > u64::from(limits.max_storage_buffer_binding_size)
    {
        return Err(soft(format!(
            "frame needs a {bytes}-byte GPU buffer; device limits are {} (buffer) / {} (storage binding)",
            limits.max_buffer_size, limits.max_storage_buffer_binding_size
        )));
    }
    Ok(bytes)
}

fn grid(w: u32, h: u32, limits: &wgpu::Limits) -> Result<(u32, u32), GpuError> {
    let g = (w.div_ceil(WG), h.div_ceil(WG));
    if g.0 > limits.max_compute_workgroups_per_dimension
        || g.1 > limits.max_compute_workgroups_per_dimension
    {
        return Err(soft("frame exceeds max_compute_workgroups_per_dimension"));
    }
    Ok(g)
}

fn decode(bytes: &[u8], count: usize) -> Result<Vec<[f32; 4]>, GpuError> {
    if count.checked_mul(16) != Some(bytes.len()) {
        return Err(fatal("GPU readback length mismatch"));
    }
    let mut out = Vec::new();
    out.try_reserve_exact(count)
        .map_err(|_| soft("readback allocation failed"))?;
    for c in bytes.chunks_exact(16) {
        let f = |i: usize| f32::from_ne_bytes([c[i], c[i + 1], c[i + 2], c[i + 3]]);
        out.push([f(0), f(4), f(8), f(12)]);
    }
    Ok(out)
}

fn entry(binding: u32, buffer: &wgpu::Buffer) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: buffer.as_entire_binding(),
    }
}

impl Ctx {
    fn new() -> Result<Ctx, String> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
        }))
        .map_err(|e| format!("no GPU adapter: {e}"))?;
        let info = adapter.get_info();
        let identity = AdapterIdentity {
            name: info.name.clone(),
            backend: format!("{:?}", info.backend),
            device_type: format!("{:?}", info.device_type),
            driver: info.driver.clone(),
            driver_info: info.driver_info.clone(),
            vendor: info.vendor,
            device: info.device,
        };
        if info.device_type == wgpu::DeviceType::Cpu {
            return Err(format!(
                "software adapter rejected (hardware GPU required): {identity}"
            ));
        }
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("onmk Inner Shadow"),
            required_features: wgpu::Features::empty(),
            required_limits: adapter.limits(),
            memory_hints: wgpu::MemoryHints::Performance,
            trace: wgpu::Trace::Off,
        }))
        .map_err(|e| format!("GPU device request failed on {identity}: {e}"))?;

        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("onmk Inner Shadow shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shader.wgsl").into()),
        });
        let storage = |binding: u32, read_only: bool| wgpu::BindGroupLayoutEntry {
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
            label: Some("onmk Inner Shadow bindings"),
            entries: &[
                storage(0, true),
                storage(1, true),
                storage(2, false),
                storage(3, true),
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
            label: Some("onmk Inner Shadow layout"),
            bind_group_layouts: &[&bind_layout],
            push_constant_ranges: &[],
        });
        let pipeline = |entry_point: &str| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry_point),
                layout: Some(&layout),
                module: &shader,
                entry_point: Some(entry_point),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let level = pipeline("level");
        let accum = pipeline("accum");
        let blur_h = pipeline("blur_h");
        let blur_v = pipeline("blur_v");
        let composite = pipeline("composite");
        let dummy = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("onmk Inner Shadow dummy"),
            size: 16,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        if let Some(e) = pollster::block_on(device.pop_error_scope()) {
            return Err(format!("GPU pipeline creation failed on {identity}: {e}"));
        }
        Ok(Ctx {
            device,
            queue,
            bind_layout,
            level,
            accum,
            blur_h,
            blur_v,
            composite,
            dummy,
            identity,
            exec: Mutex::new(()),
        })
    }

    fn shade(&self, prep: &Prepared, plan: &Plan) -> Result<(Vec<[f32; 4]>, GpuRun), GpuError> {
        let _exec = self.exec.lock().unwrap_or_else(|p| p.into_inner());
        let limits = self.device.limits();
        let (w, h) = (to_u32(prep.omega_w)?, to_u32(prep.omega_h)?);
        let (ow, oh) = (to_u32(prep.out_w)?, to_u32(prep.out_h)?);
        if prep.e.len() != prep.omega_w * prep.omega_h || prep.src.len() != prep.out_w * prep.out_h
        {
            return Err(soft("prepared frame is inconsistent"));
        }
        let plane_bytes = buffer_bytes(prep.e.len(), 4, &limits)?;
        let rgba_bytes = buffer_bytes(prep.src.len(), 16, &limits)?;
        let plane_grid = grid(w, h, &limits)?;
        let out_grid = grid(ow, oh, &limits)?;
        let base = (to_i32(prep.base.0)?, to_i32(prep.base.1)?);

        self.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let run = self.run(
            prep,
            plan,
            (w, h, ow, oh),
            plane_bytes,
            rgba_bytes,
            plane_grid,
            out_grid,
            base,
        );
        let validation = pollster::block_on(self.device.pop_error_scope());
        let oom = pollster::block_on(self.device.pop_error_scope());
        if let Some(e) = validation {
            return Err(fatal(format!("GPU validation error: {e}")));
        }
        if let Some(e) = oom {
            return Err(fatal(format!("GPU out of memory: {e}")));
        }
        run
    }

    #[allow(clippy::too_many_arguments)]
    fn run(
        &self,
        prep: &Prepared,
        plan: &Plan,
        (w, h, ow, oh): (u32, u32, u32, u32),
        plane_bytes: u64,
        rgba_bytes: u64,
        plane_grid: (u32, u32),
        out_grid: (u32, u32),
        base: (i32, i32),
    ) -> Result<(Vec<[f32; 4]>, GpuRun), GpuError> {
        let dev = &self.device;
        let mut keep: Vec<wgpu::Buffer> = Vec::new();
        let mut groups: Vec<wgpu::BindGroup> = Vec::new();
        let storage = wgpu::BufferUsages::STORAGE;
        let e = dev.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("E"),
            contents: bytemuck::cast_slice(&prep.e),
            usage: storage,
        });
        let plane = |label: &'static str| {
            dev.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: plane_bytes,
                usage: storage,
                mapped_at_creation: false,
            })
        };
        let la = plane("level a");
        let lb = plane("level b");
        let d_lo = plan.dilate_lo.as_ref().map(|_| plane("dilate lo"));
        let d_hi = plan.dilate_hi.as_ref().map(|_| plane("dilate hi"));
        let rgba = dev.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rgba"),
            contents: bytemuck::cast_slice(&prep.src),
            usage: storage | wgpu::BufferUsages::COPY_SRC,
        });
        let readback = dev.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: rgba_bytes,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let kx = dev.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("kernel x"),
            contents: bytemuck::cast_slice(&plan.kernel_x),
            usage: storage,
        });
        let ky = dev.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("kernel y"),
            contents: bytemuck::cast_slice(&plan.kernel_y),
            usage: storage,
        });

        let mut enc = dev.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("onmk Inner Shadow"),
        });
        let base_u = Uniform {
            w,
            h,
            out_w: ow,
            out_h: oh,
            ..Default::default()
        };
        let mut dispatches = 0u32;
        if let (Some(d), Some(out)) = (&plan.dilate_lo, &d_lo) {
            dispatches += self.dilate(
                &mut enc,
                &mut keep,
                &mut groups,
                (&e, &la, &lb, out),
                d,
                base_u,
                plane_grid,
            );
        }
        if let (Some(d), Some(out)) = (&plan.dilate_hi, &d_hi) {
            dispatches += self.dilate(
                &mut enc,
                &mut keep,
                &mut groups,
                (&e, &la, &lb, out),
                d,
                base_u,
                plane_grid,
            );
        }
        let lo_ref = d_lo.as_ref().unwrap_or(&e);
        let hi_ref = d_hi.as_ref().unwrap_or(&e);

        let mut u = base_u;
        u.i[0] = (plan.kernel_x.len() / 2) as i32;
        u.f[0] = plan.dilate_mix;
        self.dispatch(
            &mut enc,
            &mut keep,
            &mut groups,
            &self.blur_h,
            [lo_ref, hi_ref, &la, &kx],
            u,
            plane_grid,
        );
        let mut u = base_u;
        u.i[0] = (plan.kernel_y.len() / 2) as i32;
        self.dispatch(
            &mut enc,
            &mut keep,
            &mut groups,
            &self.blur_v,
            [&la, &self.dummy, &lb, &ky],
            u,
            plane_grid,
        );

        let (wx0, wx1) = plan.weights_x();
        let (wy0, wy1) = plan.weights_y();
        let blend = match plan.blend {
            Blend::Normal => 1,
            Blend::Multiply => 2,
        };
        let c = plan.color;
        let u = Uniform {
            i: [base.0, base.1, blend, 0],
            f: [wx0, wx1, wy0, wy1, plan.opacity, c[0], c[1], c[2]],
            ..base_u
        };
        self.dispatch(
            &mut enc,
            &mut keep,
            &mut groups,
            &self.composite,
            [&lb, &self.dummy, &rgba, &self.dummy],
            u,
            out_grid,
        );
        dispatches += 3;

        enc.copy_buffer_to_buffer(&rgba, 0, &readback, 0, rgba_bytes);
        self.queue.submit(Some(enc.finish()));

        let (tx, rx) = mpsc::channel();
        readback.map_async(wgpu::MapMode::Read, .., move |r| {
            let _ = tx.send(r);
        });
        // A blocking PollType::wait before recv_timeout defeats the timeout.
        // Poll without waiting, so a hung submission can reach our deadline.
        let deadline = Instant::now() + MAP_TIMEOUT;
        loop {
            self.device
                .poll(wgpu::PollType::Poll)
                .map_err(|e| fatal(format!("GPU poll failed: {e}")))?;
            match rx.try_recv() {
                Ok(Ok(())) => break,
                Ok(Err(e)) => return Err(fatal(format!("GPU readback map failed: {e}"))),
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err(fatal("GPU readback callback disconnected"))
                }
                Err(mpsc::TryRecvError::Empty) if Instant::now() >= deadline => {
                    return Err(fatal("GPU readback timed out"))
                }
                Err(mpsc::TryRecvError::Empty) => std::thread::sleep(Duration::from_millis(1)),
            }
        }
        // Only a successfully mapped buffer is unmapped; the view is dropped first.
        let pixels = {
            let view = readback.get_mapped_range(..);
            decode(&view, prep.src.len())
        };
        readback.unmap();
        let pixels = pixels?;
        drop(groups);
        drop(keep);
        Ok((
            pixels,
            GpuRun {
                adapter: self.identity.name.clone(),
                backend: self.identity.backend.clone(),
                device_type: self.identity.device_type.clone(),
                driver: format!("{} {}", self.identity.driver, self.identity.driver_info),
                dispatches,
            },
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn dilate(
        &self,
        enc: &mut wgpu::CommandEncoder,
        keep: &mut Vec<wgpu::Buffer>,
        groups: &mut Vec<wgpu::BindGroup>,
        (e, la, lb, out): (&wgpu::Buffer, &wgpu::Buffer, &wgpu::Buffer, &wgpu::Buffer),
        d: &Dilation,
        base: Uniform,
        plane_grid: (u32, u32),
    ) -> u32 {
        let spans: Vec<i32> = d.spans.iter().flat_map(|s| [s.dy, s.half_width]).collect();
        let aux = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("spans"),
                contents: bytemuck::cast_slice(&spans),
                usage: wgpu::BufferUsages::STORAGE,
            });
        let mut n = 0u32;
        let mut cur: &wgpu::Buffer = e;
        let mut next_a = true;
        let mut first = true;
        let mut start = 0usize;
        for level in 0..d.levels {
            if level > 0 {
                let dst = if next_a { la } else { lb };
                let mut u = base;
                u.i[0] = 1 << (level - 1);
                self.dispatch(
                    enc,
                    keep,
                    groups,
                    &self.level,
                    [cur, &self.dummy, dst, &self.dummy],
                    u,
                    plane_grid,
                );
                n += 1;
                cur = dst;
                next_a = !next_a;
            }
            let end = start
                + d.spans[start..]
                    .iter()
                    .take_while(|s| s.level == level)
                    .count();
            if end > start {
                let mut u = base;
                u.i = [1 << level, start as i32, (end - start) as i32, first as i32];
                self.dispatch(
                    enc,
                    keep,
                    groups,
                    &self.accum,
                    [cur, &self.dummy, out, &aux],
                    u,
                    plane_grid,
                );
                n += 1;
                first = false;
            }
            start = end;
        }
        keep.push(aux);
        n
    }

    #[allow(clippy::too_many_arguments)]
    fn dispatch(
        &self,
        enc: &mut wgpu::CommandEncoder,
        keep: &mut Vec<wgpu::Buffer>,
        groups: &mut Vec<wgpu::BindGroup>,
        pipeline: &wgpu::ComputePipeline,
        bufs: [&wgpu::Buffer; 4],
        u: Uniform,
        grid: (u32, u32),
    ) {
        let ub = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("onmk Inner Shadow uniform"),
                contents: bytemuck::bytes_of(&u),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.bind_layout,
            entries: &[
                entry(0, bufs[0]),
                entry(1, bufs[1]),
                entry(2, bufs[2]),
                entry(3, bufs[3]),
                entry(4, &ub),
            ],
        });
        {
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(grid.0, grid.1, 1);
        }
        groups.push(group);
        keep.push(ub);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniform_layout_matches_wgsl_struct() {
        assert_eq!(std::mem::size_of::<Uniform>(), 64);
    }

    #[test]
    fn readback_decode_preserves_bits_and_rejects_bad_lengths() {
        let bits = [0x8000_0000u32, 0x4100_0000, 0xc000_0000, 0x7fc0_0123];
        let bytes: Vec<u8> = bits.iter().flat_map(|b| b.to_ne_bytes()).collect();
        let v = decode(&bytes, 1).ok().unwrap();
        assert_eq!(v[0].map(f32::to_bits), bits);
        assert!(decode(&bytes[..15], 1).is_err());
        assert!(decode(&bytes, 2).is_err());
    }

    #[test]
    fn limits_are_checked_before_buffer_creation() {
        let mut limits = wgpu::Limits::downlevel_defaults();
        assert!(buffer_bytes(0, 4, &limits).is_err());
        assert_eq!(buffer_bytes(10, 16, &limits).ok(), Some(160));
        limits.max_buffer_size = 100;
        assert!(buffer_bytes(10, 16, &limits).is_err());
        limits.max_buffer_size = 1 << 40;
        limits.max_storage_buffer_binding_size = 100;
        assert!(buffer_bytes(10, 16, &limits).is_err());
        limits.max_compute_workgroups_per_dimension = 2;
        assert!(grid(33, 1, &limits).is_err());
        assert_eq!(grid(32, 17, &limits).ok(), Some((2, 2)));
        assert!(to_i32(1 << 40).is_err());
    }
}
